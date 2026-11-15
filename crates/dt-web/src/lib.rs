//! dt-web: the engine in the browser.
//!
//! What `dharmatiles check`, the preview's outline and the editor's printed
//! sizes already do, and now what `dharmatiles build` does, behind
//! `wasm-bindgen`: [`dt_eval`] reads a document and its library, plans it and
//! builds it; [`dt_doc`] describes it and says what a length prints;
//! [`dt_value`] draws from a weighted range.  Every call returns JSON (the
//! page parses it), typed arrays, or throws: nothing is swallowed on the way.
//!
//! The library (`parts/heights.dtile`, the printer, and the files a document
//! `use`s) is passed in from the page with [`set_library`], as one JSON
//! object of `lib:` path to text; nothing of it is compiled in.  The engine
//! reads it through [`dt_eval::Source`], the same trait the program reads its
//! disk through.

use std::cell::RefCell;
use std::collections::BTreeMap;

use dt_doc::consts::ConstTable;
use dt_doc::model::{ConstKind, Entry, Setting};
use dt_doc::paths::HEIGHTS;
use dt_doc::schema::{OPENLOCK_SQUARE_MM, SQUARE_MM};
use dt_doc::units::Units;
use dt_eval::FixJson;
use dt_eval::{Diag, Options, Source, Tier};
use dt_value::{Rng, Seed};
use serde::{Deserialize, Serialize};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

/// Off the browser (the host's tests), an error is its message: wasm-bindgen is a dependency of the browser
/// build only, so the host build is the program's own (its proc macros would otherwise turn on another
/// `syn`, and every crate that derives serde would be compiled a second time for the tests).
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug)]
pub struct JsError(String);

#[cfg(not(target_arch = "wasm32"))]
impl JsError {
    /// An error saying `message`.
    #[must_use]
    pub fn new(message: &str) -> Self {
        Self(message.to_owned())
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl std::fmt::Display for JsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// The path the page's document is read as.
const MAIN: &str = "tile.dtile";

/// The printer's layer on both systems (the default printer's).
const LAYER_MM: f64 = 0.2;

thread_local! {
    static LIBRARY: RefCell<BTreeMap<String, String>> = const { RefCell::new(BTreeMap::new()) };
}

#[cfg(target_arch = "wasm32")]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
extern "C" {
    #[wasm_bindgen(js_namespace = performance, js_name = now)]
    fn performance_now() -> f64;
}

#[cfg(target_arch = "wasm32")]
fn clock_secs() -> f64 {
    performance_now() / 1000.0
}

#[cfg(target_arch = "wasm32")]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
extern "C" {
    #[wasm_bindgen(js_namespace = console, js_name = error)]
    fn console_error(message: &str);
}

/// The engine's version: the program's own (`Cargo.toml`), which a failure the page reports names, as the program's do.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
#[must_use]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

/// Give the module the library: a JSON object of `lib:` path to text.  In the
/// browser it also gives the engine the page's clock (`performance.now()`),
/// which a build times its stages with.
///
/// # Errors
/// The text is not a JSON object of strings.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn set_library(json: &str) -> Result<(), JsError> {
    let map: BTreeMap<String, String> = serde_json::from_str(json)
        .map_err(|e| JsError::new(&format!("the library is not a JSON object of text: {e}")))?;
    LIBRARY.with(|l| *l.borrow_mut() = map);
    #[cfg(target_arch = "wasm32")]
    {
        dt_eval::clock::set_clock(clock_secs);
        // a Rust panic is a wasm trap (`unreachable`) with nothing to read: say
        // what it was on the console first, where the page's worker picks it up
        std::panic::set_hook(Box::new(|info| console_error(&format!("panic: {info}"))));
    }
    Ok(())
}

/// The page's library and its document, as the engine's [`Source`]: the
/// document is `tile.dtile`, every other path is a library file.
struct Overlay<'a> {
    library: &'a BTreeMap<String, String>,
    text: &'a str,
}

impl Source for Overlay<'_> {
    fn read(&self, path: &str) -> Result<String, String> {
        if path == MAIN {
            return Ok(self.text.to_owned());
        }
        self.library
            .get(path)
            .cloned()
            .ok_or_else(|| format!("the page's library has no `{path}`"))
    }
}

/// What `mm`, `in` and `layer` are worth on a build target (`db` or `ol`).
fn units_for(system: &str) -> Result<Units, JsError> {
    let square_mm = match system {
        "db" => SQUARE_MM,
        "ol" => OPENLOCK_SQUARE_MM,
        other => {
            return Err(JsError::new(&format!(
                "unknown build target `{other}` (db or ol)"
            )));
        }
    };
    Ok(Units {
        square_mm,
        layer_mm: LAYER_MM,
    })
}

/// The engine's options for a target: the library's default printer, read
/// through the page's library.
fn options(source: &dyn Source, system: &str) -> Result<Options, JsError> {
    units_for(system)?;
    let mut o = Options::in_source(source).map_err(|ds| {
        let why: Vec<&str> = ds.iter().map(|d| d.message.as_str()).collect();
        JsError::new(&format!(
            "the library's default printer does not read: {}",
            why.join("; ")
        ))
    })?;
    system.clone_into(&mut o.system);
    Ok(o)
}

/// UTF-16 units before byte offset `at` of `text`: what a browser's text
/// area counts.
fn utf16_at(text: &str, at: usize) -> usize {
    let at = at.min(text.len());
    text.char_indices()
        .take_while(|(i, _)| *i < at)
        .map(|(_, c)| c.len_utf16())
        .sum()
}

/// 1-based line and column (in characters) of byte offset `at`.
fn line_col(text: &str, at: usize) -> (usize, usize) {
    let at = at.min(text.len());
    let before = text.get(..at).unwrap_or_default();
    let line = before.matches('\n').count() + 1;
    let col = before
        .rfind('\n')
        .map_or(before.chars().count(), |i| before[i + 1..].chars().count())
        + 1;
    (line, col)
}

/// One mistake or note of the page's document, with the places a text area and a card need (the program's own
/// [`dt_eval::Diag`] made wire).  Its fix is the program's [`FixJson`]: the same type its messages and checks carry,
/// made where both the program and this crate reach it (`dt_eval::FixJson`, beside `Mistake`), so a fix has one definition.
#[derive(Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub code: String,
    pub message: String,
    pub label: String,
    pub help: Option<String>,
    /// The file it is in (`tile.dtile` is the page's document).
    pub file: String,
    /// Byte offset and length in that file.
    pub span: [usize; 2],
    /// UTF-16 offset and length, for a text area.
    pub span16: [usize; 2],
    pub line: usize,
    pub col: usize,
    /// An edit that mends it, when the program has one: write `replacement` over the `len` bytes at `offset`.
    pub fix: Option<FixJson>,
}

fn diagnostic(e: &Diag) -> Diagnostic {
    let (file, text, o, l) = e.at.as_ref().map_or_else(
        || (String::new(), "", 0, 0),
        |(src, span)| {
            (
                src.name().to_owned(),
                src.inner().as_str(),
                span.offset(),
                span.len(),
            )
        },
    );
    let (line, col) = line_col(text, o);
    let s16 = utf16_at(text, o);
    let e16 = utf16_at(text, o + l);
    Diagnostic {
        code: e.code.to_owned(),
        message: e.message.clone(),
        label: e.label.clone(),
        help: e.help.clone(),
        file,
        span: [o, l],
        span16: [s16, e16 - s16],
        line,
        col,
        fix: e.fix.as_deref().map(FixJson::from),
    }
}

/// What a length prints on each target, as the editor shows it.
#[derive(Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct PrintedBoth {
    /// DungeonBlocks: the printed mm of each end, empty when it prints as written.
    pub db: Vec<String>,
    /// OpenLOCK.
    pub ol: Vec<String>,
}

/// A control written on an outline entry, with what it prints on both targets.
#[derive(Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct OutlineControl {
    pub name: String,
    /// The value as written, without its quotes.
    pub value: String,
    pub span16: [usize; 2],
    /// What the schema says it measures (`Dmm`), when it is a known control.
    pub axis: Option<String>,
    pub about: Option<String>,
    pub printed: Option<PrintedBoth>,
}

/// One entry of the document's outline (`analyze`).  Not the program's `StackItem` (the plan's entry, made from a
/// planned tile with ids and clouds): the outline reads only the text, to light the written places of a text area.
#[derive(Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct OutlineItem {
    pub what: String,
    /// `element` or `definition`.
    pub callee: String,
    pub line: usize,
    pub span16: [usize; 2],
    pub controls: Vec<OutlineControl>,
    pub children: Vec<OutlineItem>,
}

/// The tile line of a document.
#[derive(Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct TileInfo {
    pub name: String,
    pub cols: u32,
    pub rows: u32,
    pub seed: Option<i64>,
    pub about: Option<String>,
}

/// `analyze`'s answer.
#[derive(Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Analysis {
    /// False when the document has a mistake: `diagnostics` says where.
    pub ok: bool,
    pub diagnostics: Vec<Diagnostic>,
    pub tile: Option<TileInfo>,
    pub stack: Vec<OutlineItem>,
}

struct Ctx<'a> {
    text: &'a str,
    consts: &'a ConstTable,
    db: Units,
    ol: Units,
}

fn control(c: &Setting, cx: &Ctx<'_>) -> OutlineControl {
    let (o, l) = c.value.as_ref().map_or_else(
        || (c.span.offset(), c.span.len()),
        |v| (v.span.offset(), v.span.len()),
    );
    let raw = cx.text.get(o..o + l).unwrap_or_default();
    let value = raw
        .trim()
        .trim_matches('"')
        .trim_start_matches('#')
        .trim_matches('"')
        .to_owned();
    let (s16, e16) = (utf16_at(cx.text, o), utf16_at(cx.text, o + l));
    let axis = c.control.and_then(|k| k.value);
    let printed = axis.map(|axis| {
        let both = |u| {
            dt_doc::printed::printed_size(axis, &value, u, Some(cx.consts))
                .map_or_else(Vec::new, |p| p.ends)
        };
        PrintedBoth {
            db: both(cx.db),
            ol: both(cx.ol),
        }
    });
    OutlineControl {
        name: c.name.value.clone(),
        value,
        span16: [s16, e16 - s16],
        axis: axis.map(|a| format!("{a:?}")),
        about: c.control.map(|k| k.about.to_owned()),
        printed,
    }
}

fn item(e: &Entry, cx: &Ctx<'_>) -> OutlineItem {
    let (o, l) = (e.span.offset(), e.span.len());
    let (s16, e16) = (utf16_at(cx.text, o), utf16_at(cx.text, o + l));
    OutlineItem {
        what: e.what.value.clone(),
        callee: if e.element().is_some() {
            "element"
        } else {
            "definition"
        }
        .to_owned(),
        line: line_col(cx.text, o).0,
        span16: [s16, e16 - s16],
        controls: e.controls.iter().map(|c| control(c, cx)).collect(),
        children: e.children.iter().map(|c| item(c, cx)).collect(),
    }
}

fn json<T: Serialize>(v: &T) -> Result<String, JsError> {
    serde_json::to_string(v).map_err(|e| JsError::new(&format!("cannot write the answer: {e}")))
}

/// Read and check `text` as a `.dtile` for the build target `system` (`db`
/// or `ol`) with the engine's own reader and planner, and describe it: its
/// diagnostics (code, message, help, spans), its tile and its stack with each
/// control as written and as printed on both targets.  JSON.
///
/// # Errors
/// An unknown target, or the library was never set.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn analyze(text: &str, system: &str) -> Result<String, JsError> {
    let (db, ol) = (units_for("db")?, units_for("ol")?);
    LIBRARY.with(|lib| {
        let lib = lib.borrow();
        if !lib.contains_key(HEIGHTS) {
            return Err(JsError::new(
                "the library has no parts/heights.dtile: call set_library first",
            ));
        }
        let source = Overlay {
            library: &lib,
            text,
        };
        let opts = options(&source, system)?;
        let checked = match dt_eval::check(&source, MAIN, &opts) {
            Ok(c) => c,
            Err(ds) => {
                return json(&Analysis {
                    ok: false,
                    diagnostics: ds.iter().map(diagnostic).collect(),
                    tile: None,
                    stack: Vec::new(),
                });
            }
        };
        let Some(def) = checked.file.doc.tile() else {
            return json(&Analysis {
                ok: true,
                diagnostics: Vec::new(),
                tile: None,
                stack: Vec::new(),
            });
        };
        let empty = ConstTable::default();
        let consts = checked.tile.as_ref().map_or(&empty, |t| &t.heights.consts);
        let cx = Ctx {
            text,
            consts,
            db,
            ol,
        };
        json(&Analysis {
            ok: true,
            diagnostics: Vec::new(),
            tile: def.header().map(|h| TileInfo {
                name: def.name.value.clone(),
                cols: h.cols,
                rows: h.rows,
                seed: h.seed,
                about: def.about.clone(),
            }),
            stack: def.stack.iter().map(|e| item(e, &cx)).collect(),
        })
    })
}

/// What a check of the page's document found, for the cards: its mistakes and
/// notes (the same fields as [`analyze`]'s diagnostics, so the page draws one
/// kind of message), the draft lattice's map (the editor's overlay JSON, from
/// [`dt_eval::map::TileMap`], made by the one constructor the program's server uses too) and
/// what one square is on the target.
#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Checked {
    pub ok: bool,
    pub errors: Vec<Diagnostic>,
    pub notes: Vec<Diagnostic>,
    /// The tile's map, or null when the document has no tile or did not read.
    pub map: Option<dt_eval::map::TileMap>,
    /// One square's side on the target, mm.
    pub square_mm: f64,
}

/// Check `text` for `system` and describe what the overlay draws: JSON
/// ([`Checked`]).  A document with mistakes is not an error here: its
/// mistakes are in `errors`.
///
/// # Errors
/// An unknown target, or the library was never set.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn check_tile(text: &str, system: &str) -> Result<String, JsError> {
    let units = units_for(system)?;
    LIBRARY.with(|lib| {
        let lib = lib.borrow();
        if !lib.contains_key(HEIGHTS) {
            return Err(JsError::new(
                "the library has no parts/heights.dtile: call set_library first",
            ));
        }
        let source = Overlay {
            library: &lib,
            text,
        };
        let opts = options(&source, system)?;
        let checked = match dt_eval::check(&source, MAIN, &opts) {
            Ok(c) => c,
            Err(ds) => {
                return json(&Checked {
                    ok: false,
                    errors: ds.iter().map(diagnostic).collect(),
                    notes: Vec::new(),
                    map: None,
                    square_mm: units.square_mm,
                });
            }
        };
        let notes = checked.notes().iter().map(diagnostic).collect();
        match dt_eval::map::json(&checked, text, units) {
            Ok(map) => json(&Checked {
                ok: true,
                errors: Vec::new(),
                notes,
                map,
                square_mm: units.square_mm,
            }),
            Err(ds) => json(&Checked {
                ok: false,
                errors: ds.iter().map(diagnostic).collect(),
                notes,
                map: None,
                square_mm: units.square_mm,
            }),
        }
    })
}

/// Lay the program's fix for a mistake into `text`: `fix` is the JSON of a check's `errors[].fix` (a
/// [`FixJson`]), and the edit is [`dt_doc::ops::Op::Fix`], the one operation the program's editor and
/// `dharmatiles fix` use.  Returns the new text.
///
/// # Errors
/// The fix is not that JSON, or the text no longer reads what the fix was made for.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn fix_tile(text: &str, fix: &str) -> Result<String, JsError> {
    let f: FixJson = serde_json::from_str(fix)
        .map_err(|e| JsError::new(&format!("the fix is not a FixJson: {e}")))?;
    let fixes = [dt_eval::Fix::from(&f)];
    let done = dt_doc::ops::apply(dt_doc::ops::Kind::Tile, text, &dt_doc::ops::Op::Fix(&fixes))
        .map_err(|e| JsError::new(&e.to_string()))?;
    Ok(done.text)
}

/// What a length prints on both targets, read against the library's heights:
/// `7dmm`, `3sq`, `[1dmm .. 6dmm]`, `$groundHeight + 2dmm`.  JSON: the
/// printed mm of each end for DungeonBlocks and OpenLOCK (empty where it
/// prints as written), or null where the value is not a length.
///
/// # Errors
/// The library was never set, or its heights do not read.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn printed(typed: &str) -> Result<String, JsError> {
    let (db, ol) = (units_for("db")?, units_for("ol")?);
    LIBRARY.with(|lib| {
        let lib = lib.borrow();
        let source = Overlay {
            library: &lib,
            text: "",
        };
        let read = dt_eval::doc::read_with(&source, HEIGHTS, db).map_err(|ds| {
            let why: Vec<&str> = ds.iter().map(|d| d.message.as_str()).collect();
            JsError::new(&format!(
                "the library's heights do not read: {}",
                why.join("; ")
            ))
        })?;
        let library_consts: Vec<_> = read
            .file
            .doc
            .consts
            .iter()
            .filter(|c| c.kind == ConstKind::Let)
            .cloned()
            .collect();
        let consts = ConstTable::root(&library_consts);
        let at = |u| {
            dt_doc::printed::printed_size(dt_doc::schema::Axis::Dmm, typed, u, Some(&consts))
                .map(|p| p.ends)
        };
        let (a, b) = (at(db), at(ol));
        json(&match (a, b) {
            (None, None) => None,
            (a, b) => Some(PrintedBoth {
                db: a.unwrap_or_default(),
                ol: b.unwrap_or_default(),
            }),
        })
    })
}

/// A tile as the page draws it: three floats a corner, a colour a corner.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub struct Tile {
    meta: String,
    stl: Vec<u8>,
    positions: Vec<f32>,
    colours: Vec<f32>,
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
impl Tile {
    /// What the build found, as JSON.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(getter))]
    #[must_use]
    pub fn meta(&self) -> String {
        self.meta.clone()
    }

    /// The binary STL a printer is given: made for a full-detail build
    /// (`cells` 0) and empty for a draft, which is never exported.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(getter))]
    #[must_use]
    pub fn stl(&self) -> Vec<u8> {
        self.stl.clone()
    }

    /// Corner positions, mm, three floats each, three corners a triangle.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(getter))]
    #[must_use]
    pub fn positions(&self) -> Vec<f32> {
        self.positions.clone()
    }

    /// Corner colours, linear 0 to 1, three floats each: the editor's paint
    /// (overhang past the band red, the 55 to 65 degree band orange).
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(getter))]
    #[must_use]
    pub fn colours(&self) -> Vec<f32> {
        self.colours.clone()
    }
}

/// What a build in the page says beside its mesh: the build's record, the program's own (`dt_eval::TileRecord`, the
/// gate's verdict and numbers, the stage hashes and timings), and what only the page needs.
#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct BuildMeta {
    /// False when the document did not build: `diagnostics` says why.
    pub built: bool,
    pub diagnostics: Vec<Diagnostic>,
    /// Cells per square the build used; 0 is the build's own finest.
    pub cells: u32,
    pub footprint: [f64; 2],
    /// SHA-256 of the STL's triangles (the file after its 80-byte header,
    /// which names the engine and document), as hex.
    pub stl_sha256: String,
    pub stl_bytes: usize,
    /// The build's record; none when it did not build.
    pub record: Option<dt_eval::TileRecord>,
}

/// One build, as the program does it: the text of a document, a build
/// target, a seed (negative keeps the tile's own) and cells per square (0
/// keeps the build's finest).  The one function the browser and the native
/// parity test both call.
///
/// # Errors
/// The document's mistakes, as the program's diagnostics.
pub fn build_with(
    source: &dyn Source,
    system: &str,
    seed: i64,
    cells: u32,
    tier: Tier,
    progress: &dt_eval::Progress,
) -> Result<(dt_eval::Built, Options), Vec<Diag>> {
    let mut opts = Options::in_source(source)?;
    opts.tier = tier;
    system.clone_into(&mut opts.system);
    opts.seed = u64::try_from(seed).ok();
    opts.cells = (cells > 0).then_some(cells);
    let built = dt_eval::build_with(source, MAIN, &opts, progress)?;
    Ok((built, opts))
}

#[cfg(target_arch = "wasm32")]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
extern "C" {
    /// The page's worker hears each stage: `globalThis.dtProgress(json)`, the JSON of a [`StageTick`].
    #[wasm_bindgen(js_namespace = globalThis, js_name = dtProgress)]
    fn js_progress(tick: &str);
}

/// Nobody listens off the browser.
#[cfg(not(target_arch = "wasm32"))]
fn js_progress(_tick: &str) {}

/// One tick of a build in the page: the stage it is in and how far the whole build has come.  A build here is
/// timed by the page (its worker's clock), so this carries no times, unlike the program's own progress record.
#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct StageTick {
    /// 0 to 0.99 (a finished build is the page's `built` answer).
    pub fraction: f64,
    pub label: String,
    /// Stages begun before this one, and the stages the build will announce.
    pub done: u32,
    pub total: u32,
}

/// How many stages a build of `source` will announce: its stack, the union and the audit.  A document that
/// does not check builds nothing, and the build reports why, so the count only has to be a number.
fn stage_total(source: &dyn Source, system: &str) -> usize {
    let stack = options(source, system)
        .ok()
        .and_then(|o| dt_eval::check(source, MAIN, &o).ok())
        .and_then(|c| c.tile.as_ref().map(|t| t.stack.len()));
    stack.unwrap_or(1) + 2
}

/// Reports a build's stages to the page, as [`StageTick`]s: the program's progress events (`dt_kit::Event`) made into
/// the one number the page's bar needs.
struct Ticks {
    total: usize,
    begun: std::sync::atomic::AtomicUsize,
    label: std::sync::Mutex<String>,
}

impl dt_kit::Reporter for Ticks {
    fn report(&self, event: &dt_kit::Event<'_>) {
        use std::sync::atomic::Ordering::Relaxed;
        let total = self.total.max(1);
        let at = |done: usize, part: f64, label: &str| {
            #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
            let (f, d, t) = (
                ((done as f64 + part) / total as f64).clamp(0.0, 0.99),
                done as u32,
                total as u32,
            );
            let tick = StageTick {
                fraction: f,
                label: label.to_owned(),
                done: d,
                total: t,
            };
            // a reporter cannot return an error, and a tick of numbers and a label cannot fail to write: if it
            // ever does, the console says so (where the worker picks it up) rather than dropping it unseen
            match serde_json::to_string(&tick) {
                Ok(json) => js_progress(&json),
                #[cfg(target_arch = "wasm32")]
                Err(e) => console_error(&format!("cannot write a progress tick: {e}")),
                #[cfg(not(target_arch = "wasm32"))]
                Err(e) => eprintln!("cannot write a progress tick: {e}"),
            }
        };
        match event {
            dt_kit::Event::Begin { label, .. } => {
                let n = self.begun.fetch_add(1, Relaxed);
                if let Ok(mut l) = self.label.lock() {
                    (*label).clone_into(&mut l);
                }
                at(n, 0.0, label);
            }
            dt_kit::Event::Sub { fraction, .. } => {
                let n = self.begun.load(Relaxed).saturating_sub(1);
                let label = self.label.lock().map(|l| l.clone()).unwrap_or_default();
                at(n, *fraction, &label);
            }
            dt_kit::Event::End { .. } => {}
        }
    }
}

fn ticks(total: usize) -> dt_eval::Progress {
    dt_eval::Progress::new(std::sync::Arc::new(Ticks {
        total,
        begun: std::sync::atomic::AtomicUsize::new(0),
        label: std::sync::Mutex::new(String::new()),
    }))
}

/// The STL of a build, and the hash the parity check compares: SHA-256 of
/// everything after the 80-byte header.
#[must_use]
pub fn stl_sha256(passed: &dt_eval::Passed) -> (String, usize) {
    stl_hash(&dt_print::stl(passed))
}

/// The hash of an STL file already made.
fn stl_hash(bytes: &[u8]) -> (String, usize) {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(&bytes[80..]);
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    (hex, bytes.len())
}

/// Build `text` for `system` and hand back the mesh, painted, with the
/// gate's findings.  `seed` below 0 keeps the tile's own; `cells` is cells
/// per square (0: the build's finest).  A document with a mistake is not an
/// error: the result is unbuilt and says where, in `meta`.
///
/// # Errors
/// An unknown target, or the library was never set.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn build_tile(
    text: &str,
    system: &str,
    seed: i32,
    cells: u32,
    tier: &str,
) -> Result<Tile, JsError> {
    units_for(system)?;
    let tier = tier.parse::<Tier>().map_err(|e| JsError::new(&e))?;
    LIBRARY.with(|lib| {
        let lib = lib.borrow();
        if !lib.contains_key(HEIGHTS) {
            return Err(JsError::new(
                "the library has no parts/heights.dtile: call set_library first",
            ));
        }
        let source = Overlay {
            library: &lib,
            text,
        };
        let (built, opts) = match build_with(
            &source,
            system,
            i64::from(seed),
            cells,
            tier,
            &ticks(stage_total(&source, system)),
        ) {
            Ok(b) => b,
            Err(ds) => {
                let meta = BuildMeta {
                    built: false,
                    diagnostics: ds.iter().map(diagnostic).collect(),
                    cells,
                    footprint: [0.0; 2],
                    stl_sha256: String::new(),
                    stl_bytes: 0,
                    record: None,
                };
                return Ok(Tile {
                    meta: json(&meta)?,
                    stl: Vec::new(),
                    positions: Vec::new(),
                    colours: Vec::new(),
                });
            }
        };
        let mesh = &built.export.mesh;
        let mut positions = Vec::with_capacity(mesh.triangles.len() * 9);
        for t in &mesh.triangles {
            for &i in t {
                for x in mesh.positions[i as usize] {
                    #[allow(clippy::cast_possible_truncation)]
                    positions.push(x as f32);
                }
            }
        }
        let mut colours = Vec::with_capacity(mesh.triangles.len() * 9);
        for c in built.colours() {
            for _ in 0..3 {
                colours.extend_from_slice(&c);
            }
        }
        // only a build that passed the gate has an STL: a draft, a low build or a failing one has none
        let stl_file = built
            .verdict(&opts.printer)
            .passed()
            .map(|p| dt_print::stl(&p));
        let (stl_sha256, stl_bytes) = stl_file
            .as_deref()
            .map_or_else(|| (String::new(), 0), stl_hash);
        let build_s = built.report.stage_secs.iter().map(|s| s.1).sum();
        let meta = BuildMeta {
            built: true,
            diagnostics: Vec::new(),
            cells,
            footprint: built.footprint,
            stl_sha256,
            stl_bytes,
            record: Some(dt_eval::TileRecord::of(&built, &opts.printer, build_s)),
        };
        Ok(Tile {
            meta: json(&meta)?,
            stl: if cells == 0 {
                stl_file.unwrap_or_default()
            } else {
                Vec::new()
            },
            positions,
            colours,
        })
    })
}

/// A weighted range, drawn.
#[derive(Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Explored {
    /// The odds' function: `flat`, `low`, `list`, ...
    pub function: String,
    pub args: Vec<f64>,
    /// As the program writes the range (`low(1..6, 4)`), in dmm.
    pub written: String,
    /// The ends, dmm at the 35 mm reference.
    pub lo_dmm: f64,
    pub hi_dmm: f64,
    /// The ends printed on the target, mm.
    pub lo_mm: f64,
    pub hi_mm: f64,
    /// The program's own curve: each of `bins` slices' share, as a multiple
    /// of a flat range's.  Null for a list.
    pub density: Option<Vec<f64>>,
    /// A list's values: position along the range and share of the draws.
    pub masses: Option<Vec<(f64, f64)>>,
    /// How many of `draws` landed in each of `bins` equal slices.
    pub counts: Vec<u32>,
    pub draws: u32,
    /// The mean of the draws, dmm; null for no draws (JSON has no NaN).
    pub mean_dmm: Option<f64>,
}

/// Read `typed` as one weighted range (`low(1dmm .. 6dmm, 4)`, `1..6`,
/// `[1:4, 2:5, 3]`) and draw `draws` values from it with `seed`, as a build
/// draws them.  JSON.
///
/// # Errors
/// The value is not one range with odds (the program's own refusal, in
/// words), or `bins` is 0.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn explore(
    typed: &str,
    system: &str,
    bins: u32,
    draws: u32,
    seed: u32,
) -> Result<String, JsError> {
    if bins == 0 {
        return Err(JsError::new("a histogram needs at least one bin"));
    }
    let units = units_for(system)?;
    let r = dt_doc::lower::ranged(typed, units).map_err(|r| {
        let help = r.help.map(|h| format!(" ({h})")).unwrap_or_default();
        JsError::new(&format!("{}: {}{help}", r.code, r.message))
    })?;
    let d = &r.dist;
    let mut rng = Rng::new(Seed::new(u64::from(seed)));
    let mut counts = vec![0_u32; bins as usize];
    let mut sum = 0.0;
    for _ in 0..draws {
        let v = d.draw(&mut rng);
        sum += v;
        let t = if d.is_point() {
            0.0
        } else {
            (v - d.lo) / (d.hi - d.lo)
        };
        // the top edge belongs to the last bin
        let k = (t * f64::from(bins)).floor().max(0.0);
        // 0 <= k <= bins, so the cast is exact
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let k = (k as usize).min(bins as usize - 1);
        counts[k] += 1;
    }
    let to_mm = units.square_mm / SQUARE_MM;
    json(&Explored {
        function: r.function,
        args: r.args,
        written: d.written(),
        lo_dmm: d.lo,
        hi_dmm: d.hi,
        lo_mm: d.lo * to_mm,
        hi_mm: d.hi * to_mm,
        density: d.weighting.density(bins as usize),
        masses: d.weighting.masses(),
        counts,
        draws,
        mean_dmm: (draws > 0).then(|| sum / f64::from(draws)),
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn lib() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../library");
        let mut m = BTreeMap::new();
        for group in ["parts", "printers", "tiles"] {
            let mut dirs = vec![root.join(group)];
            while let Some(d) = dirs.pop() {
                for e in std::fs::read_dir(d).unwrap() {
                    let p = e.unwrap().path();
                    if p.is_dir() {
                        dirs.push(p);
                    } else if p.extension().is_some_and(|x| x == "dtile") {
                        let rel = p
                            .strip_prefix(&root)
                            .unwrap()
                            .to_string_lossy()
                            .into_owned();
                        m.insert(format!("lib:{rel}"), std::fs::read_to_string(&p).unwrap());
                    }
                }
            }
        }
        LIBRARY.with(|l| *l.borrow_mut() = m);
    }

    #[test]
    fn a_range_is_drawn_with_the_programs_odds() {
        let v: Explored =
            serde_json::from_str(&explore("low(1dmm .. 6dmm, 4)", "db", 10, 1000, 1).unwrap())
                .unwrap();
        assert_eq!(v.function, "low");
        assert_eq!(v.counts.len(), 10);
        // low odds: the first slice is the most drawn
        assert!(v.counts[0] > v.counts[9]);
    }

    #[test]
    fn a_length_prints_less_on_openlock() {
        lib();
        let v: Option<PrintedBoth> = serde_json::from_str(&printed("7dmm").unwrap()).unwrap();
        assert_eq!(v.unwrap().ol[0], "5.08");
    }

    #[test]
    fn a_mistake_is_a_diagnostic_with_a_span() {
        lib();
        let v: Analysis =
            serde_json::from_str(&analyze("tile size=\"1x1\" {}\n", "db").unwrap()).unwrap();
        assert!(!v.ok);
        assert!(v.diagnostics[0].code.starts_with("dt::"));
    }

    /// Where the site's engine types are checked in.
    fn contract_file() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/assets/live/engine-wire.d.ts")
    }

    /// The TypeScript of every answer the engine gives the page, and the engine's functions with the type each
    /// answers as JSON.
    fn engine_typescript() -> String {
        let mut all = dt_doc::contract::Contract::default();
        all.add::<Analysis>();
        all.add::<Checked>();
        all.add::<BuildMeta>();
        all.add::<Explored>();
        all.add::<PrintedBoth>();
        all.add::<StageTick>();
        all.add::<FixJson>();
        let mut out = String::from(
            "// Generated from crates/dt-web/src/lib.rs (the engine's answers and the types they name) by\n\
             // `DT_CONTRACT=write cargo test -p dt-web --lib the_engine_contract_is_current`.\n\
             // Do not edit: a test fails when this file is not what the Rust types say.  The site's scripts\n\
             // (docs/assets/live/*.js) are checked against it by `npm test` in web/preview.\n\n",
        );
        out.push_str(&all.declarations());
        out.push_str(
            "/** Each engine function that answers JSON, and what the JSON is. */\n\
             export interface Answers {\n  analyze: Analysis;\n  check_tile: Checked;\n  explore: Explored;\n  \
             printed: PrintedBoth | null;\n  \"build_tile().meta\": BuildMeta;\n  \"dtProgress\": StageTick;\n}\n\n\
             /** A built tile (`Tile`): its getters, as wasm-bindgen exports them. */\n\
             export interface EngineTile {\n  readonly meta: string;\n  readonly stl: Uint8Array;\n  \
             readonly positions: Float32Array;\n  readonly colours: Float32Array;\n  free(): void;\n}\n\n\
             /** The compiled module (docs/engine/dt_web.js, `cargo xtask wasm`, not in the repo): its exports, written here \
             beside the Rust functions they are, so a script is checked the same whether a build is on disk or not. */\n\
             export interface Engine {\n  default(init: { module_or_path: WebAssembly.Module | string }): Promise<unknown>;\n  \
             version(): string;\n  set_library(json: string): void;\n  analyze(text: string, system: string): string;\n  \
             check_tile(text: string, system: string): string;\n  fix_tile(text: string, fix: string): string;\n  \
             printed(typed: string): string;\n  \
             build_tile(text: string, system: string, seed: number, cells: number, tier: string): EngineTile;\n  \
             explore(typed: string, system: string, bins: number, draws: number, seed: number): string;\n}\n",
        );
        out
    }

    /// `docs/assets/live/engine-wire.d.ts` is what the engine's Rust types say; `DT_CONTRACT=write` rewrites it.  The
    /// site's scripts are checked against that file by `tsc` (`npm test` in web/preview), so a field renamed here
    /// fails there.
    #[test]
    fn the_engine_contract_is_current() {
        let want = engine_typescript();
        if std::env::var("DT_CONTRACT").is_ok_and(|v| v == "write") {
            std::fs::write(contract_file(), &want).unwrap();
            return;
        }
        let have = std::fs::read_to_string(contract_file()).unwrap_or_default();
        assert!(
            have == want,
            "docs/assets/live/engine-wire.d.ts is not what the engine's types say; run\n  DT_CONTRACT=write cargo test -p dt-web --lib the_engine_contract_is_current\nand then `npm test` in web/preview, which checks the site against it"
        );
    }

    /// Prints the parity hash of a build, for comparing with the browser's:
    /// `PARITY_TILE=lib:tiles/ground/1x1-grass.dtile PARITY_CELLS=48 cargo test --release -p dt-web parity -- --ignored --nocapture`.
    #[test]
    #[ignore = "a hash for comparing with the browser's: run by hand"]
    fn parity() {
        lib();
        let var = |k: &str, d: &str| std::env::var(k).unwrap_or_else(|_| d.to_owned());
        let tile = var("PARITY_TILE", "lib:tiles/ground/1x1-grass.dtile");
        let system = var("PARITY_SYSTEM", "db");
        let seed: i64 = var("PARITY_SEED", "-1").parse().unwrap();
        let cells: u32 = var("PARITY_CELLS", "0").parse().unwrap();
        let lib = LIBRARY.with(|l| l.borrow().clone());
        // PARITY_FILE: a document on disk (the page's own presets) instead of a library tile
        let text = match std::env::var("PARITY_FILE") {
            Ok(f) => std::fs::read_to_string(f).unwrap(),
            Err(_) => lib.get(&tile).unwrap().clone(),
        };
        let t0 = std::time::Instant::now();
        let source = Overlay {
            library: &lib,
            text: &text,
        };
        let (built, opts) = build_with(
            &source,
            &system,
            seed,
            cells,
            Tier::Full,
            &dt_eval::Progress::none(),
        )
        .unwrap();
        let verdict = built.verdict(&opts.printer);
        let passed = verdict.passed();
        let (hash, bytes) = passed
            .as_ref()
            .map_or_else(|| (String::new(), 0), stl_sha256);
        println!(
            "PARITY {tile} {system} seed={seed} cells={cells}: sha256={hash} bytes={bytes} triangles={} failures={:?} secs={:.2}",
            built.audit.triangles,
            verdict.reasons(),
            t0.elapsed().as_secs_f64()
        );
    }
}
