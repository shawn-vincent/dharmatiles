//! The thing build: `buildThing`, the editor's draw of one thing alone.
//!
//! A thing (a rock, a tree, a flower, a definition whose stack is one of them) is made from its own controls and a
//! seed, so the page can show it with no tile: one view of it at a seed, and a strip of draws at many.  This is one
//! verb with one meaning for both transports: the program's HTTP handler and a web worker's `Service` call
//! [`build_thing`], which reads the document (`dt_eval::thing_build::subject`: the same reader that reads a tile), makes
//! each draw (`Subject::draw`: `Thing::make`, the join and the gate's overhang paint) and answers each as the page's one
//! mesh payload ([`crate::payload::from_built`]).
//!
//! A thing's draw is not kept as a whole (the build store's key is made of a whole tile), but its **body** is, as in a tile:
//! the host's shelf ([`PlatesHost::thing_shelf`]: the disk store, or the worker's memory) holds each rock under the thing's
//! own key, so asking for a seed again reads the body and builds nothing ([`ThingRes::used`] says how many of each).  A
//! tree or a flower is built where it stands, so it is built every time.

use serde::{Deserialize, Serialize};

use crate::call::{Refusal, Status, Verb};
use crate::export_calls::base64;
use crate::plates_calls::PlatesHost;
use crate::tile_calls::CheckHost;

/// The most draws one call makes: the strip is about twelve.
pub const MOST_DRAWS: usize = 48;

/// Which thing of the document is meant.
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, Eq, ts_rs::TS)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum ThingRef {
    /// A `define` of that name, called with its params at their defaults.
    #[serde(rename_all = "camelCase")]
    Define { name: String },
    /// A stack entry by the id a check gave it (`StackItem::key`); `child` is one of a scatter's things, by its place.
    #[serde(rename_all = "camelCase")]
    Entry { key: String, child: Option<u32> },
}

/// `buildThing`: draw one thing at each of `seeds`.  `text` is the text the page holds of `file` (a file the tile uses), or of the
/// tile's own document when there is none: it is laid over what the host stores, as a check's is.  What is read is the tile for an
/// entry of its stack (a call of a thing a part defines is an entry of the tile, which the part's text may be being edited under),
/// and the file for a `define` the page is in.
#[derive(Deserialize, Serialize, Debug, ts_rs::TS)]
#[ts(optional_fields = nullable)]
#[serde(deny_unknown_fields)]
pub struct ThingReq {
    pub text: String,
    /// A file the tile uses, instead of its own document.
    pub file: Option<String>,
    pub which: ThingRef,
    /// One seed for the view, or many for the strip; each is drawn on its own.
    pub seeds: Vec<u64>,
    /// How finely: `full` (joined and through the gate's overhang paint: what prints) when left out, `low` for a quick look (a
    /// tree takes tens of seconds at full, and a fraction of one at low, as in a tile).
    pub tier: Option<dt_eval::Tier>,
}

/// A box, mm as printed on the build's target: along the main axis, across it, and up.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, ts_rs::TS)]
pub struct Dims {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl From<dt_elements::Box3> for Dims {
    fn from(b: dt_elements::Box3) -> Self {
        Self {
            x: b.x,
            y: b.y,
            z: b.z,
        }
    }
}

/// The room a thing takes: the smallest and the largest box it can draw, mm as printed on the target, and the same two as
/// the document writes them (dmm: 1/35 of a square, so they scale with the system where the printed ones do).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct ThingExtent {
    pub least: Dims,
    pub most: Dims,
    pub least_dmm: Dims,
    pub most_dmm: Dims,
}

/// A control as the thing was built with it: its name, the value as a document writes it, and where it comes from.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, ts_rs::TS)]
pub struct ThingControl {
    pub control: String,
    pub text: String,
    pub from: String,
}

/// One draw: the seed, the mesh the page reads (the gate's overhang paint as built, frame z up) and what it came to.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct ThingDraw {
    pub seed: u64,
    /// The triangles of the mesh.
    pub triangles: usize,
    /// This draw's own box, mm as printed.
    pub size: Dims,
    /// Whether the gate checked this draw: a full one is, a low one is not (each face flagged by its angle alone).
    pub checked: bool,
    /// The mesh payload (`web/preview/js/payload.js` reads it), as standard base64: an answer is JSON.
    pub mesh: String,
}

/// What `buildThing` answers.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
pub struct ThingRes {
    /// The element it is (`rock`).
    pub element: String,
    /// Its controls as built.
    pub shown: Vec<ThingControl>,
    /// The smallest and largest box it can draw.
    pub extent: ThingExtent,
    /// One for each seed asked, in order.
    pub draws: Vec<ThingDraw>,
    /// What the draws did for their bodies: made, and read from the shelf (a seed asked again is read, not made).
    pub used: dt_kit::ThingUse,
    /// What went wrong with the shelf (a damaged entry dropped and made again, a body that could not be kept), in words: the
    /// draws are real either way, and the page shows each of these.
    pub problems: Vec<String>,
}

pub const BUILD_THING: Verb<ThingReq, ThingRes> = Verb::new("buildThing");

/// Draw the thing `v.which` means, at each seed.  `base` is what the host stores (the tile, the parts it uses, the library) and
/// `tile` the tile's own document in it; the request's text goes over `v.file`, or over the tile, and the thing is read from the
/// tile for an entry, from `v.file` for a `define` (see [`ThingReq`]).
///
/// # Errors
/// [`Refusal`]: no seed or too many (400); no such define or entry (404); it is a layer, or the document does not read
/// (422); a draw could not be made or encoded (500).
pub fn build_thing<H: PlatesHost + CheckHost>(
    host: &H,
    base: &dyn dt_eval::Source,
    tile: &str,
    v: &ThingReq,
) -> Result<ThingRes, Refusal> {
    let held = v.file.as_deref().unwrap_or(tile);
    let document = match (&v.which, v.file.as_deref()) {
        (ThingRef::Define { .. }, Some(file)) => file,
        _ => tile,
    };
    let src = crate::read_calls::Overlay {
        files: std::collections::BTreeMap::from([(held.to_owned(), v.text.clone())]),
        base,
    };
    let src: &dyn dt_eval::Source = &src;
    if v.seeds.is_empty() || v.seeds.len() > MOST_DRAWS {
        return Err(Refusal::refused(
            Status::BadRequest,
            format!("ask for 1 to {MOST_DRAWS} draws, not {}", v.seeds.len()),
        ));
    }
    let mut opts = host.options();
    opts.tier = v.tier.unwrap_or_default();
    let which = match &v.which {
        ThingRef::Define { name } => dt_eval::thing_build::Which::Define(name),
        ThingRef::Entry { key, child } => dt_eval::thing_build::Which::Entry {
            key,
            child: child.map(|c| c as usize),
        },
    };
    let subject = dt_eval::thing_build::subject(src, document, which, &opts).map_err(|r| match r {
        dt_eval::thing_build::Refused::Mistakes(errs) => Refusal::of_mistakes(&errs),
        dt_eval::thing_build::Refused::NoDefine(n) => {
            Refusal::refused(Status::NotFound, format!("there is no `define {n}` here"))
        }
        dt_eval::thing_build::Refused::NoEntry(k) => {
            Refusal::refused(Status::NotFound, format!("there is no entry `{k}` here"))
        }
        dt_eval::thing_build::Refused::Layer { name, element } => Refusal::refused(
            Status::Unprocessable,
            format!(
                "`{name}` is a layer (`{element}` reads what is under it and changes it), not a thing: only a rock, a tree, a flower or a definition of one is drawn alone"
            ),
        ),
    })?;
    let palette = host.palette();
    let (e, written) = (subject.extent(&opts), subject.written());
    let shelf = host.thing_shelf(&opts);
    let bodies = dt_kit::Bodies::new(shelf.as_deref());
    let mut draws = Vec::with_capacity(v.seeds.len());
    for &seed in &v.seeds {
        let built = subject
            .draw(seed, &opts, &bodies)
            .map_err(|errs| Refusal::of_mistakes(&errs))?;
        let mesh = crate::payload::from_built(&built, opts.printer.overhang_deg, &palette)
            .map_err(|e| {
                Refusal::failure(
                    Status::Internal,
                    format!("cannot make the mesh of seed {seed}: {e}"),
                )
            })?;
        let (lo, hi) = dt_print::box_of(&built.export.mesh);
        draws.push(ThingDraw {
            seed,
            triangles: built.export.mesh.triangles.len(),
            size: Dims {
                x: hi[0] - lo[0],
                y: hi[1] - lo[1],
                z: hi[2] - lo[2],
            },
            checked: built.checked,
            mesh: base64(&mesh),
        });
    }
    let problems = bodies.finish();
    Ok(ThingRes {
        element: subject.element.clone(),
        shown: subject
            .shown
            .iter()
            .map(|s| ThingControl {
                control: s.control.clone(),
                text: s.text.clone(),
                from: s.from.clone(),
            })
            .collect(),
        extent: ThingExtent {
            least: e.least.into(),
            most: e.most.into(),
            least_dmm: written.least.into(),
            most_dmm: written.most.into(),
        },
        draws,
        used: bodies.used(),
        problems,
    })
}
