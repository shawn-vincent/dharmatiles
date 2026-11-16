//! The thing build: `buildThing`, the editor's draw of one thing alone.
//!
//! A thing (a rock, a tree, a flower, a definition whose stack is one of them) is made from its own controls and a
//! seed, so the page can show it with no tile: one view of it at a seed, and a strip of draws at many.  This is one
//! verb with one meaning for both transports: the program's HTTP handler and a web worker's `Service` call
//! [`build_thing`], which reads the document (`dt_eval::thing_build::subject`: the same reader that reads a tile), makes
//! each draw (`Subject::draw`: `Thing::make`, the join and the gate's overhang paint) and answers each as the page's one
//! mesh payload ([`crate::payload::from_built`]).
//!
//! A thing's draw is **not kept**: the build store's key is made of a whole tile (its text, path and target), and a
//! thing has no key of its own yet.  Every call builds again, which is cheap for one thing (a rock is a few tenths of a
//! second) and is what keeps the view live as the document is typed.

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

/// `buildThing`: draw one thing of `text` (the tile's document, or `file`, a file it uses) at each of `seeds`.
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
}

pub const BUILD_THING: Verb<ThingReq, ThingRes> = Verb::new("buildThing");

/// Draw the thing `v.which` means in the document `src` holds at `document` (the tile's own file, or a part it uses), at each seed.
///
/// # Errors
/// [`Refusal`]: no seed or too many (400); no such define or entry (404); it is a layer, or the document does not read
/// (422); a draw could not be made or encoded (500).
pub fn build_thing<H: PlatesHost + CheckHost>(
    host: &H,
    src: &dyn dt_eval::Source,
    document: &str,
    v: &ThingReq,
) -> Result<ThingRes, Refusal> {
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
    let mut draws = Vec::with_capacity(v.seeds.len());
    for &seed in &v.seeds {
        let built = subject
            .draw(seed, &opts)
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
    })
}
