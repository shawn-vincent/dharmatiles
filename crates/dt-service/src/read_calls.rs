//! The reading calls: what a text says without being built (its names, its colours, a typed value, what a length
//! prints, what the Add menu may offer) and what the editor's page reads at startup (`about`, the element schema,
//! the language's words, a document's text).  Their types and the one function each that the program's HTTP handler
//! and a web worker's `Service` both call; the reading ones take the text to read as a [`Source`] the caller made.

use std::collections::BTreeMap;

use dt_doc::elements::ELEMENTS as SCHEMA_ELEMENTS;
use dt_elements::{Child, Plan};
use dt_eval::Source;
use dt_eval::plan::{Refusal as PlanRefusal, schedule};
use serde::{Deserialize, Serialize};

use crate::call::{Refusal, Status, Verb};
use crate::tile_calls::CheckHost;
use crate::tile_host::{TileHost, TileRef};

/// A text of a file laid over what the host reads: the document being edited.
pub struct Overlay<'a> {
    pub files: BTreeMap<String, String>,
    pub base: &'a dyn Source,
}

impl Source for Overlay<'_> {
    fn read(&self, path: &str) -> Result<String, String> {
        self.files
            .get(path)
            .cloned()
            .map_or_else(|| self.base.read(path), Ok)
    }
}

#[derive(Deserialize, Serialize, Debug, ts_rs::TS)]
#[ts(optional_fields = nullable)]
#[serde(deny_unknown_fields)]
pub struct NamesReq {
    pub text: String,
}

/// The names a text makes; `ok` false (and `message`) when it does not parse: its
/// mistakes are the check's to show, and the page keeps the names it had.
#[derive(Serialize, Deserialize, Debug, ts_rs::TS)]
pub struct NamesRes {
    pub ok: bool,
    pub message: Option<String>,
    pub defs: Vec<String>,
    pub regions: Vec<String>,
    pub lines: Vec<String>,
    pub points: Vec<String>,
}

/// A text to colour.
#[derive(Deserialize, Serialize, Debug, ts_rs::TS)]
#[ts(optional_fields = nullable)]
#[serde(deny_unknown_fields)]
pub struct TokensReq {
    pub text: String,
}

/// How a text is coloured, by `dt_lang::highlight`'s one set of rules: `classes` are the CSS classes (`tk-node`,
/// …), and `pieces` runs three numbers for each coloured piece: its first and last-plus-one UTF-8 byte in the
/// text and its place in `classes`.  What no piece covers is plain.
#[derive(Serialize, Deserialize, Debug, ts_rs::TS)]
pub struct TokensRes {
    pub classes: Vec<String>,
    pub pieces: Vec<u32>,
}

#[derive(Deserialize, Serialize, Debug, ts_rs::TS)]
#[ts(optional_fields = nullable)]
#[serde(deny_unknown_fields)]
pub struct CheckValueReq {
    pub element: String,
    pub control: String,
    pub typed: String,
}

/// What the reader refuses in a typed value (`message` null when it takes it).
#[derive(Serialize, Deserialize, Debug, ts_rs::TS)]
pub struct CheckValueRes {
    pub message: Option<String>,
    pub label: Option<String>,
    pub help: Option<String>,
}

/// One written value to say the printed size of: the control it is for and
/// the text as written.
#[derive(Deserialize, Serialize, Debug, ts_rs::TS)]
#[ts(optional_fields = nullable)]
#[serde(deny_unknown_fields)]
pub struct PrintedItem {
    pub element: String,
    pub control: String,
    pub typed: String,
}

/// `/api/tiles/ID/printed`: the sizes `items` print on the current target,
/// reading the `$` heights and `let`s of `text` (the tile's document).
#[derive(Deserialize, Serialize, Debug, ts_rs::TS)]
#[ts(optional_fields = nullable)]
#[serde(deny_unknown_fields)]
pub struct PrintedReq {
    pub text: String,
    pub items: Vec<PrintedItem>,
}

/// What a value prints, mm to 0.01: one end for a number, two for a range;
/// `editable` when the printed size can be typed in.
#[derive(Serialize, Deserialize, Debug, ts_rs::TS)]
pub struct PrintedSize {
    pub ends: Vec<String>,
    pub editable: bool,
}

/// One answer for each item, in order: `null` where no printed size is shown
/// (not a length, printing as written, or naming what the document does not
/// give).
#[derive(Serialize, Deserialize, Debug, ts_rs::TS)]
pub struct PrintedRes {
    pub items: Vec<Option<PrintedSize>>,
}

/// `/api/printed-set`: the value `typed` with its end `end` set so that it
/// prints `mm`, in its own unit.
#[derive(Deserialize, Serialize, Debug, ts_rs::TS)]
#[ts(optional_fields = nullable)]
#[serde(deny_unknown_fields)]
pub struct PrintedSetReq {
    pub typed: String,
    pub end: u32,
    pub mm: f64,
}

/// The value as it is now written; the page writes it like any typed value.
#[derive(Serialize, Deserialize, Debug, ts_rs::TS)]
pub struct PrintedSetRes {
    pub typed: String,
}

/// `/api/tiles/ID/offers`.
#[derive(Deserialize, Serialize, Debug, ts_rs::TS)]
#[ts(optional_fields = nullable)]
#[serde(deny_unknown_fields)]
pub struct OffersReq {
    pub text: String,
    /// The place in the stack: 0 is the top.
    pub at: usize,
    pub file: Option<String>,
}

/// One thing the Add menu may offer.
#[derive(Serialize, Deserialize, Debug, ts_rs::TS)]
pub struct Offer {
    pub name: String,
    pub family: String,
    pub about: String,
    /// The line of KDL it would write.
    pub node: String,
    /// Whether the plan lets it stand there.
    pub ok: bool,
    /// When it does not, why.
    pub why: Option<String>,
}

/// What may be offered; `ok` false (with `why` and the `errors`) when the
/// tile has a mistake and there is no plan to ask.
#[derive(Serialize, Deserialize, Debug, ts_rs::TS)]
pub struct OffersRes {
    pub ok: bool,
    pub at: Option<usize>,
    pub offers: Vec<Offer>,
    pub why: Option<String>,
    pub errors: Option<Vec<crate::api::Mistake>>,
}

/// A thing to offer: its name, what it writes into the stack and whether a
/// tile may hold more than one.
struct Candidate {
    name: &'static str,
    family: &'static str,
    about: &'static str,
    node: String,
    plan: Result<Plan, String>,
}

/// The element named `name` as a stack entry with nothing set, planned.
fn default_plan(name: &str) -> Result<Plan, String> {
    // a scatter is what its children are: one rock is the plainest
    let kids = if name == "scatter" {
        vec![Child {
            name: "rock".to_owned(),
            controls: Vec::new(),
            weight: 1.0,
        }]
    } else {
        Vec::new()
    };
    dt_elements::element_with(name, &[], &kids)
        .map(|e| e.plan())
        .map_err(|e| e.message)
}

/// The line of KDL that adds the element with nothing set but what it must
/// have: a wall runs along something, so along the first drawn line (else
/// the north edge); a scatter holds a thing and says how many; a second dirt
/// says the band it fills, which the first is given by the library's edge.
fn node_of(name: &str, lines: &[&String], has_dirt: bool) -> String {
    match name {
        "dirt" if has_dirt => "dirt top=\"$dirtEdgeHeight + 35dmm\"".to_owned(),
        "wall" => format!("wall at={}", lines.first().map_or("north", |l| l.as_str())),
        "scatter" => "scatter count=3 {\n    rock\n}".to_owned(),
        other => other.to_owned(),
    }
}

/// The candidates, in the order the schema lists the elements, the base last.
fn candidates(lines: &[&String], has_dirt: bool) -> Vec<Candidate> {
    let built: Vec<&str> = dt_elements::built().collect();
    SCHEMA_ELEMENTS
        .iter()
        .filter(|e| built.contains(&e.name))
        .map(|e| Candidate {
            name: e.name,
            family: e.family.as_str(),
            about: e.about,
            node: node_of(e.name, lines, has_dirt),
            plan: default_plan(e.name),
        })
        .collect()
}

/// Why the plan refuses `with` (the stack with the candidate inserted at
/// `k`), in words that name the candidate "it" and any other entry by its
/// element; `names` are the elements of the stack as written.
fn why(r: &PlanRefusal, k: usize, names: &[String]) -> String {
    let who = |i: usize| -> String {
        if i == k {
            "it".to_owned()
        } else {
            let was = if i > k { i - 1 } else { i };
            format!(
                "the `{}` listed there",
                names.get(was).map_or("base", String::as_str)
            )
        }
    };
    match r {
        PlanRefusal::NoWriter { entry, kind } => format!(
            "{} reads every {kind}, and nothing in the stack makes one: add one above first",
            who(*entry)
        ),
        PlanRefusal::SurfaceReadsSolid { entry } => {
            format!(
                "{} is a surface that reads solids, which cannot be yet",
                who(*entry)
            )
        }
        PlanRefusal::Early { entry, kind, .. } => {
            let (what, fix) = match kind {
                Some(dt_elements::Kind::Terrain) => (
                    "stands on the ground, and the ground is painted after it here".to_owned(),
                    "list a `dirt` before it".to_owned(),
                ),
                Some(read) => (
                    format!("reads every {read}, and the first one is listed after it here"),
                    "list it after what it reads".to_owned(),
                ),
                None => (
                    "lies on the ground, and a surface would be painted after it".to_owned(),
                    "list it after every `dirt` it lies on".to_owned(),
                ),
            };
            format!("{} {what}: {fix}", who(*entry))
        }
    }
}

/// `names`: the names the text makes, by the parser, for completion between checks.  A text that does not parse says
/// so (`{ok: false, message}`): its mistakes are the check's to show, and the page keeps the names it had.
#[must_use]
pub fn names(v: &NamesReq) -> NamesRes {
    match dt_doc::nodeid::names_in(&v.text) {
        Ok(n) => NamesRes {
            ok: true,
            message: None,
            defs: n.defs,
            regions: n.regions,
            lines: n.lines,
            points: n.points,
        },
        Err(e) => NamesRes {
            ok: false,
            message: Some(e.to_string()),
            defs: Vec::new(),
            regions: Vec::new(),
            lines: Vec::new(),
            points: Vec::new(),
        },
    }
}

/// `tokens`: the text's colouring, for the editor's source pane.  Any text colours (a broken one as far as it reads);
/// a text too long to number in 32 bits is the one failure.
///
/// # Errors
/// [`Refusal`] (bad request): the text is too long to colour.
pub fn tokens(v: &TokensReq) -> Result<TokensRes, Refusal> {
    let too_long = |_| Refusal::refused(Status::BadRequest, "the text is too long to colour");
    let mut pieces = Vec::new();
    for p in dt_lang::highlight::tokens(&v.text) {
        pieces.push(u32::try_from(p.start).map_err(too_long)?);
        pieces.push(u32::try_from(p.end).map_err(too_long)?);
        pieces.push(u32::try_from(p.class.index()).map_err(too_long)?);
    }
    Ok(TokensRes {
        classes: dt_lang::highlight::CLASSES
            .iter()
            .map(|c| c.css().to_owned())
            .collect(),
        pieces,
    })
}

/// `checkValue`: the reader's check of one typed value, with units as the host's build target reads them.
#[must_use]
pub fn check_value<H: CheckHost>(host: &H, v: &CheckValueReq) -> CheckValueRes {
    // a control the schema does not know (the tile's own, a line's) has nothing to check against here
    let opts = host.options();
    let (element, control, typed) = (&v.element, &v.control, &v.typed);
    let flaw = dt_doc::schema::element(element)
        .and_then(|e| e.control_at(control))
        .and_then(|c| dt_doc::decode::check_text_in(c, element, typed, opts.units()));
    // the reader's check first; then the weighting's numbers against their limits, which the reader only meets when
    // the value is lowered
    match flaw {
        Some(r) => CheckValueRes {
            message: Some(r.message),
            label: Some(r.label.to_owned()),
            help: r.help,
        },
        None => match dt_eval::weighting_flaw_in(typed, opts.units()) {
            Some(w) => CheckValueRes {
                message: Some(w.message),
                label: Some("here".to_owned()),
                help: w.help,
            },
            None => CheckValueRes {
                message: None,
                label: None,
                help: None,
            },
        },
    }
}

/// `printed`: what each written value prints on the host's target (the system's square, the printer's layer), worked
/// out by [`dt_doc::printed`] against the heights of the document `tile` read from `src`.
#[expect(
    clippy::disallowed_methods,
    reason = "permitted silence: a probe where None means not a form this reads; a document with a mistake gives no heights, and the check call reports the mistake where the text is typed"
)]
#[must_use]
pub fn printed<H: CheckHost>(host: &H, src: &dyn Source, tile: &str, v: &PrintedReq) -> PrintedRes {
    let units = host.options().units();
    // A document with a mistake in it gives no heights: a value that reads a `$` name then has no printed size.  The
    // mistake itself is the check's to show, as the text is typed.
    let read = dt_eval::doc::read_with(src, tile, units).ok();
    let consts = read
        .as_ref()
        .and_then(|r| r.tile.as_ref())
        .map(|t| &t.heights.consts);
    let items = v
        .items
        .iter()
        .map(|i| {
            let axis = dt_doc::schema::element(&i.element)
                .and_then(|e| e.control_at(&i.control))
                .and_then(|c| c.value)?;
            dt_doc::printed::printed_size(axis, &i.typed, units, consts).map(|p| PrintedSize {
                ends: p.ends,
                editable: p.editable,
            })
        })
        .collect();
    PrintedRes { items }
}

/// `printedSet`: a printed size typed in, written in the value's own unit (see [`dt_doc::printed::set_printed`]).
///
/// # Errors
/// [`Refusal`] (unprocessable): the value cannot be set to print that.
pub fn printed_set<H: CheckHost>(host: &H, v: &PrintedSetReq) -> Result<PrintedSetRes, Refusal> {
    dt_doc::printed::set_printed(&v.typed, v.end as usize, v.mm, host.options().units())
        .map(|typed| PrintedSetRes { typed })
        .map_err(|e| Refusal::refused(Status::Unprocessable, e.0))
}

/// The offers at place `at` of the stack of the tile `tile`, read from `src`.
///
/// # Errors
/// [`Refusal`] (bad request): the file has no tile.
pub fn offers<H: CheckHost>(
    host: &H,
    src: &dyn Source,
    tile: &str,
    at: usize,
) -> Result<OffersRes, Refusal> {
    let opts = host.options();
    let checked = match dt_eval::check(src, tile, &opts) {
        Ok(c) => c,
        Err(errs) => {
            // a tile with a mistake has no plan to ask: the mistake is what to fix first
            let first = errs.first().map_or_else(String::new, |e| e.message.clone());
            return Ok(OffersRes {
                ok: false,
                at: None,
                offers: Vec::new(),
                why: Some(first),
                errors: Some(crate::api::mistakes(&errs)),
            });
        }
    };
    let Some(t) = &checked.tile else {
        return Err(Refusal::refused(
            Status::BadRequest,
            "this file has no tile",
        ));
    };
    let plans = checked.plans();
    // the entries of the stack, then the connector when the tile has a base
    let entries = t.stack.len();
    let k = at.min(entries);
    let names: Vec<String> = t.stack.iter().map(|e| e.what.value.clone()).collect();
    let lines: Vec<&String> = t.lines.keys().collect();
    let mut out = Vec::new();
    for c in candidates(&lines, names.iter().any(|n| n == "dirt")) {
        let verdict = match &c.plan {
            Err(e) => Err(format!("cannot be added with nothing set: {e}")),
            Ok(p) => {
                let mut with = plans.clone();
                with.insert(k, p.clone());
                schedule(&with).map(|_| ()).map_err(|r| why(&r, k, &names))
            }
        };
        out.push(Offer {
            name: c.name.to_string(),
            family: c.family.to_string(),
            about: c.about.to_string(),
            node: c.node.to_string(),
            ok: verdict.is_ok(),
            why: verdict.err(),
        });
    }
    // the base is the connector the build target picks; a tile has one
    out.push(Offer {
        name: "base".to_owned(),
        family: "base".to_owned(),
        about: "the connector under zero: the build target picks DungeonBlocks or OpenLOCK"
            .to_owned(),
        node: "base".to_owned(),
        ok: !t.has_base,
        why: t.has_base.then(|| "the tile already has a base".to_owned()),
    });
    Ok(OffersRes {
        ok: true,
        at: Some(k),
        offers: out,
        why: None,
        errors: None,
    })
}

/// `about`: the program's version and the tile formats it reads.
#[derive(Serialize, Deserialize, Debug, ts_rs::TS)]
pub struct AboutRes {
    pub version: String,
    pub formats: String,
}

/// `text`: a document's text for the text pane.
#[derive(Serialize, Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct TextRes {
    pub file: String,
    pub text: String,
    pub writable: bool,
    /// The revision of the file the text is of; 0 for a library file.
    pub doc_rev: u64,
}

/// `text`'s request: the file, when not the tile's own.
#[derive(Deserialize, Serialize, Debug, Default, ts_rs::TS)]
#[ts(optional_fields = nullable)]
#[serde(deny_unknown_fields)]
pub struct TextReq {
    pub file: Option<String>,
}

/// What a call that takes nothing takes.
#[derive(Deserialize, Serialize, Debug, Default, ts_rs::TS)]
#[ts(type = "Record<string, never>")]
#[serde(deny_unknown_fields)]
pub struct Nothing {}

/// `about`: the host's version, and the formats the language reads.
#[must_use]
pub fn about<H: TileHost>(host: &H) -> AboutRes {
    AboutRes {
        version: host.version(),
        formats: dt_doc::header::formats(),
    }
}

/// `text`: the text of the tile's own document or of a file it uses, with its revision; `None` when the file is not
/// one of the tile's or is not a document.
///
/// # Errors
/// [`Refusal`] (internal): the file cannot be read.
pub fn text<H: TileHost>(
    host: &H,
    tile: &TileRef,
    file: Option<&str>,
) -> Result<Option<TextRes>, Refusal> {
    let Ok(path) = tile.resolve_file(file) else {
        return Ok(None);
    };
    if !path.ends_with(".dtile") && !path.ends_with(".dmap") {
        return Ok(None);
    }
    let unread = |e: &dyn std::fmt::Display| {
        Refusal::failure(
            Status::Internal,
            format!("cannot read the file {path}: {e}"),
        )
    };
    // a library file is read only, and has no revision to edit against
    let (text, doc_rev) = if path.starts_with(dt_eval::LIB) {
        (host.library().read(&path).map_err(|e| unread(&e))?, 0)
    } else {
        host.docs().read(&path).map_err(|e| unread(&e))?
    };
    let writable = crate::tile_host::writable(host.storage(), &path);
    Ok(Some(TextRes {
        file: path,
        text,
        writable,
        doc_rev,
    }))
}

/// `elements`: every element with its controls.
#[must_use]
pub fn elements() -> Vec<dt_doc::wire::ElementJson> {
    crate::elements_json::all_json()
}

/// `language`: the language's own tables (nodes, units, function words, `$` names).
#[must_use]
pub fn language() -> dt_doc::wire::LanguageJson {
    crate::elements_json::language_json()
}

pub const NAMES: Verb<NamesReq, NamesRes> = Verb::new("names");
pub const TOKENS: Verb<TokensReq, TokensRes> = Verb::new("tokens");
pub const CHECK_VALUE: Verb<CheckValueReq, CheckValueRes> = Verb::new("checkValue");
pub const PRINTED: Verb<PrintedReq, PrintedRes> = Verb::new("printed");
pub const PRINTED_SET: Verb<PrintedSetReq, PrintedSetRes> = Verb::new("printedSet");
pub const OFFERS: Verb<OffersReq, OffersRes> = Verb::new("offers");
pub const ABOUT: Verb<Nothing, AboutRes> = Verb::new("about");
pub const ELEMENTS: Verb<Nothing, Vec<dt_doc::wire::ElementJson>> = Verb::new("elements");
pub const LANGUAGE: Verb<Nothing, dt_doc::wire::LanguageJson> = Verb::new("language");
pub const TEXT: Verb<TextReq, Option<TextRes>> = Verb::new("text");
