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
use dt_eval::{Diag, Options, Source};
use dt_value::{Rng, Seed};
use serde::Serialize;
use wasm_bindgen::prelude::*;

/// The path the page's document is read as.
const MAIN: &str = "tile.dtile";

/// The printer's layer on both systems (the default printer's).
const LAYER_MM: f64 = 0.2;

thread_local! {
    static LIBRARY: RefCell<BTreeMap<String, String>> = const { RefCell::new(BTreeMap::new()) };
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = performance, js_name = now)]
    fn performance_now() -> f64;
}

#[cfg(target_arch = "wasm32")]
fn clock_secs() -> f64 {
    performance_now() / 1000.0
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console, js_name = error)]
    fn console_error(message: &str);
}

/// Give the module the library: a JSON object of `lib:` path to text.  In the
/// browser it also gives the engine the page's clock (`performance.now()`),
/// which a build times its stages with.
///
/// # Errors
/// The text is not a JSON object of strings.
#[wasm_bindgen]
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

#[derive(Serialize)]
struct Diagnostic {
    code: &'static str,
    message: String,
    label: String,
    help: Option<String>,
    /// The file it is in (`tile.dtile` is the page's document).
    file: String,
    /// Byte offset and length in that file.
    span: [usize; 2],
    /// UTF-16 offset and length, for a text area.
    span16: [usize; 2],
    line: usize,
    col: usize,
    /// An edit that mends it, when the program has one: write `replacement` over the `len` bytes at `offset`.
    fix: Option<FixOut>,
}

/// A [`dt_eval::Fix`] on the wire, as the program's messages carry it.
#[derive(Serialize)]
struct FixOut {
    offset: usize,
    len: usize,
    was: String,
    replacement: String,
    label: String,
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
        code: e.code,
        message: e.message.clone(),
        label: e.label.clone(),
        help: e.help.clone(),
        file,
        span: [o, l],
        span16: [s16, e16 - s16],
        line,
        col,
        fix: e.fix.as_deref().map(|f| FixOut {
            offset: f.offset,
            len: f.len,
            was: f.was.clone(),
            replacement: f.replacement.clone(),
            label: f.label.clone(),
        }),
    }
}

/// What a length prints on each target, as the editor shows it.
#[derive(Serialize)]
struct PrintedBoth {
    /// DungeonBlocks: the printed mm of each end, empty when it prints as written.
    db: Vec<String>,
    /// OpenLOCK.
    ol: Vec<String>,
}

#[derive(Serialize)]
struct Control {
    name: String,
    /// The value as written, without its quotes.
    value: String,
    span16: [usize; 2],
    /// What the schema says it measures (`Dmm`), when it is a known control.
    axis: Option<String>,
    about: Option<&'static str>,
    printed: Option<PrintedBoth>,
}

#[derive(Serialize)]
struct StackItem {
    what: String,
    /// `element` or `definition`.
    callee: &'static str,
    line: usize,
    span16: [usize; 2],
    controls: Vec<Control>,
    children: Vec<StackItem>,
}

#[derive(Serialize)]
struct TileInfo {
    name: String,
    cols: u32,
    rows: u32,
    seed: Option<i64>,
    about: Option<String>,
}

#[derive(Serialize)]
struct Analysis {
    /// False when the document has a mistake: `diagnostics` says where.
    ok: bool,
    diagnostics: Vec<Diagnostic>,
    tile: Option<TileInfo>,
    stack: Vec<StackItem>,
}

struct Ctx<'a> {
    text: &'a str,
    consts: &'a ConstTable,
    db: Units,
    ol: Units,
}

fn control(c: &Setting, cx: &Ctx<'_>) -> Control {
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
    Control {
        name: c.name.value.clone(),
        value,
        span16: [s16, e16 - s16],
        axis: axis.map(|a| format!("{a:?}")),
        about: c.control.map(|k| k.about),
        printed,
    }
}

fn item(e: &Entry, cx: &Ctx<'_>) -> StackItem {
    let (o, l) = (e.span.offset(), e.span.len());
    let (s16, e16) = (utf16_at(cx.text, o), utf16_at(cx.text, o + l));
    StackItem {
        what: e.what.value.clone(),
        callee: if e.element().is_some() {
            "element"
        } else {
            "definition"
        },
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
#[wasm_bindgen]
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
/// [`dt_eval::map::json`], the one producer the program's server uses too) and
/// what one square is on the target.
#[derive(Serialize)]
struct Checked {
    ok: bool,
    errors: Vec<Diagnostic>,
    notes: Vec<Diagnostic>,
    /// The tile's map, or null when the document has no tile or did not read.
    map: serde_json::Value,
    /// One square's side on the target, mm.
    square_mm: f64,
}

/// Check `text` for `system` and describe what the overlay draws: JSON
/// ([`Checked`]).  A document with mistakes is not an error here: its
/// mistakes are in `errors`.
///
/// # Errors
/// An unknown target, or the library was never set.
#[wasm_bindgen]
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
                    map: serde_json::Value::Null,
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
                map: serde_json::Value::Null,
                square_mm: units.square_mm,
            }),
        }
    })
}

/// What a length prints on both targets, read against the library's heights:
/// `7dmm`, `3sq`, `[1dmm .. 6dmm]`, `$groundHeight + 2dmm`.  JSON: the
/// printed mm of each end for DungeonBlocks and OpenLOCK (empty where it
/// prints as written), or null where the value is not a length.
///
/// # Errors
/// The library was never set, or its heights do not read.
#[wasm_bindgen]
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
#[wasm_bindgen]
pub struct Tile {
    meta: String,
    stl: Vec<u8>,
    positions: Vec<f32>,
    colours: Vec<f32>,
}

#[wasm_bindgen]
impl Tile {
    /// What the build found, as JSON.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn meta(&self) -> String {
        self.meta.clone()
    }

    /// The binary STL a printer is given: made for a full-detail build
    /// (`cells` 0) and empty for a draft, which is never exported.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn stl(&self) -> Vec<u8> {
        self.stl.clone()
    }

    /// Corner positions, mm, three floats each, three corners a triangle.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn positions(&self) -> Vec<f32> {
        self.positions.clone()
    }

    /// Corner colours, linear 0 to 1, three floats each: the editor's paint
    /// (overhang past the band red, the 55 to 65 degree band orange).
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn colours(&self) -> Vec<f32> {
        self.colours.clone()
    }
}

#[derive(Serialize)]
struct BuildMeta {
    /// False when the document did not build: `diagnostics` says why.
    built: bool,
    diagnostics: Vec<Diagnostic>,
    triangles: usize,
    vertices: usize,
    /// Cells per square the build used; 0 is the build's own finest.
    cells: u32,
    /// The gate's verdict: it passes when `failures` is empty and the build
    /// was checked.
    passes: bool,
    checked: bool,
    failures: Vec<String>,
    watertight: bool,
    overhang_mm2: f64,
    overhang_budget_mm2: f64,
    band_mm2: f64,
    band_budget_mm2: f64,
    holes: usize,
    bodies: usize,
    volume_mm3: f64,
    footprint: [f64; 2],
    /// SHA-256 of the STL's triangles (the file after its 80-byte header,
    /// which names the engine and document), as hex.
    stl_sha256: String,
    stl_bytes: usize,
    /// Each stage's hash, in build order.
    stages: Vec<(String, String)>,
    /// How long each stage took in the page, seconds.
    stage_secs: Vec<(String, f64)>,
}

/// How finely a build is made, the program's own tiers ([`Options::draft`], [`Options::low`]): `full` is what the printer is
/// given, `draft` the editor's view while a value is dragged (coarse cells, no union, no gate), `low` the browsing tier (grass
/// as a carpet, rocks as hulls).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tier {
    Full,
    Draft,
    Low,
}

impl Tier {
    /// The tier a page names.
    ///
    /// # Errors
    /// A word that is no tier.
    pub fn named(word: &str) -> Result<Self, JsError> {
        match word {
            "full" => Ok(Self::Full),
            "draft" => Ok(Self::Draft),
            "low" => Ok(Self::Low),
            other => Err(JsError::new(&format!(
                "unknown detail `{other}` (full, draft or low)"
            ))),
        }
    }
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
    opts.draft = tier == Tier::Draft;
    opts.low = tier == Tier::Low;
    system.clone_into(&mut opts.system);
    opts.seed = u64::try_from(seed).ok();
    opts.cells = (cells > 0).then_some(cells);
    let built = dt_eval::build_with(source, MAIN, &opts, progress)?;
    Ok((built, opts))
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
extern "C" {
    /// The page's worker hears each stage: `globalThis.dtProgress(fraction, label, done, total)`.
    #[wasm_bindgen(js_namespace = globalThis, js_name = dtProgress)]
    fn js_progress(fraction: f64, label: &str, done: u32, total: u32);
}

/// Nobody listens off the browser.
#[cfg(not(target_arch = "wasm32"))]
fn js_progress(_fraction: f64, _label: &str, _done: u32, _total: u32) {}

/// How many stages a build of `source` will announce: its stack, the union and the audit.  A document that
/// does not check builds nothing, and the build reports why, so the count only has to be a number.
fn stage_total(source: &dyn Source, system: &str) -> usize {
    let stack = options(source, system)
        .ok()
        .and_then(|o| dt_eval::check(source, MAIN, &o).ok())
        .and_then(|c| c.tile.as_ref().map(|t| t.stack.len()));
    stack.unwrap_or(1) + 2
}

/// Reports a build's stages to the page: the browser's third copy of the program's progress events.
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
            js_progress(f, label, d, t);
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
pub fn stl_sha256(built: &dt_eval::Built) -> (String, usize) {
    stl_hash(&dt_print::stl(&built.export.mesh, &built.record))
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
#[wasm_bindgen]
pub fn build_tile(
    text: &str,
    system: &str,
    seed: i32,
    cells: u32,
    tier: &str,
) -> Result<Tile, JsError> {
    units_for(system)?;
    let tier = Tier::named(tier)?;
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
                    triangles: 0,
                    vertices: 0,
                    cells,
                    passes: false,
                    checked: false,
                    failures: Vec::new(),
                    watertight: false,
                    overhang_mm2: 0.0,
                    overhang_budget_mm2: 0.0,
                    band_mm2: 0.0,
                    band_budget_mm2: 0.0,
                    holes: 0,
                    bodies: 0,
                    volume_mm3: 0.0,
                    footprint: [0.0; 2],
                    stl_sha256: String::new(),
                    stl_bytes: 0,
                    stages: Vec::new(),
                    stage_secs: Vec::new(),
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
        let failures = built.audit.failures(&opts.printer);
        let stl_file = dt_print::stl(&built.export.mesh, &built.record);
        let (stl_sha256, stl_bytes) = stl_hash(&stl_file);
        let a = &built.audit;
        let meta = BuildMeta {
            built: true,
            diagnostics: Vec::new(),
            triangles: a.triangles,
            vertices: a.vertices,
            cells,
            passes: built.checked && failures.is_empty(),
            checked: built.checked,
            failures,
            watertight: a.watertight,
            overhang_mm2: a.overhang_mm2,
            overhang_budget_mm2: a.overhang_budget_mm2.max(opts.printer.overhang_budget_mm2),
            band_mm2: a.band_mm2,
            band_budget_mm2: a.band_budget_mm2,
            holes: a.holes,
            bodies: a.bodies,
            volume_mm3: a.volume_mm3,
            footprint: built.footprint,
            stl_sha256,
            stl_bytes,
            stages: built
                .report
                .stages
                .iter()
                .map(|s| (s.name.clone(), format!("{:016x}", s.hash)))
                .collect(),
            stage_secs: built.report.stage_secs.clone(),
        };
        Ok(Tile {
            meta: json(&meta)?,
            stl: if tier == Tier::Full && cells == 0 {
                stl_file
            } else {
                Vec::new()
            },
            positions,
            colours,
        })
    })
}

/// A weighted range, drawn.
#[derive(Serialize)]
struct Explored {
    /// The odds' function: `flat`, `low`, `list`, ...
    function: String,
    args: Vec<f64>,
    /// As the program writes the range (`low(1..6, 4)`), in dmm.
    written: String,
    /// The ends, dmm at the 35 mm reference.
    lo_dmm: f64,
    hi_dmm: f64,
    /// The ends printed on the target, mm.
    lo_mm: f64,
    hi_mm: f64,
    /// The program's own curve: each of `bins` slices' share, as a multiple
    /// of a flat range's.  Null for a list.
    density: Option<Vec<f64>>,
    /// A list's values: position along the range and share of the draws.
    masses: Option<Vec<(f64, f64)>>,
    /// How many of `draws` landed in each of `bins` equal slices.
    counts: Vec<u32>,
    draws: u32,
    mean_dmm: f64,
}

/// Read `typed` as one weighted range (`low(1dmm .. 6dmm, 4)`, `1..6`,
/// `[1:4, 2:5, 3]`) and draw `draws` values from it with `seed`, as a build
/// draws them.  JSON.
///
/// # Errors
/// The value is not one range with odds (the program's own refusal, in
/// words), or `bins` is 0.
#[wasm_bindgen]
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
    let r = dt_doc::lower::ranged_in(typed, units).map_err(|r| {
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
        mean_dmm: if draws == 0 {
            f64::NAN
        } else {
            sum / f64::from(draws)
        },
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
        let v: serde_json::Value =
            serde_json::from_str(&explore("low(1dmm .. 6dmm, 4)", "db", 10, 1000, 1).unwrap())
                .unwrap();
        assert_eq!(v["function"], "low");
        assert_eq!(v["counts"].as_array().unwrap().len(), 10);
        // low odds: the first slice is the most drawn
        assert!(v["counts"][0].as_u64() > v["counts"][9].as_u64());
    }

    #[test]
    fn a_length_prints_less_on_openlock() {
        lib();
        let v: serde_json::Value = serde_json::from_str(&printed("7dmm").unwrap()).unwrap();
        assert_eq!(v["ol"][0], "5.08");
    }

    #[test]
    fn a_mistake_is_a_diagnostic_with_a_span() {
        lib();
        let v: serde_json::Value =
            serde_json::from_str(&analyze("tile size=\"1x1\" {}\n", "db").unwrap()).unwrap();
        assert_eq!(v["ok"], false);
        assert!(
            v["diagnostics"][0]["code"]
                .as_str()
                .unwrap()
                .starts_with("dt::")
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
        let (hash, bytes) = stl_sha256(&built);
        println!(
            "PARITY {tile} {system} seed={seed} cells={cells}: sha256={hash} bytes={bytes} triangles={} failures={:?} secs={:.2}",
            built.audit.triangles,
            built.audit.failures(&opts.printer),
            t0.elapsed().as_secs_f64()
        );
    }
}
