//! dt-web: the language crates in the browser.
//!
//! What `dharmatiles check`, the preview's outline and the editor's printed
//! sizes already do, behind `wasm-bindgen`, with nothing from the geometry
//! crates: [`dt_doc`] reads and checks a document and says what a length
//! prints, [`dt_value`] draws from a weighted range.  Every call returns JSON
//! (the page parses it) or throws: nothing is swallowed on the way.
//!
//! The library (`parts/heights.dtile` and the files a document `use`s) is
//! passed in from the page with [`set_library`], as one JSON object of
//! `lib:` path to text; nothing of it is compiled in.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::Arc;

use dt_doc::DocError;
use dt_doc::Known;
use dt_doc::consts::ConstTable;
use dt_doc::error::Src;
use dt_doc::model::{ConstKind, Document, Entry, Setting};
use dt_doc::paths::{HEIGHTS, use_path};
use dt_doc::schema::{OPENLOCK_SQUARE_MM, SQUARE_MM};
use dt_doc::units::Units;
use dt_value::{Rng, Seed};
use miette::{NamedSource, SourceSpan};
use serde::Serialize;
use wasm_bindgen::prelude::*;

/// The path the page's document is read as.
const MAIN: &str = "tile.dtile";

/// The printer's layer on both systems (the default printer's).
const LAYER_MM: f64 = 0.2;

thread_local! {
    static LIBRARY: RefCell<BTreeMap<String, String>> = const { RefCell::new(BTreeMap::new()) };
}

/// Give the module the library: a JSON object of `lib:` path to text.
///
/// # Errors
/// The text is not a JSON object of strings.
#[wasm_bindgen]
pub fn set_library(json: &str) -> Result<(), JsError> {
    let map: BTreeMap<String, String> = serde_json::from_str(json)
        .map_err(|e| JsError::new(&format!("the library is not a JSON object of text: {e}")))?;
    LIBRARY.with(|l| *l.borrow_mut() = map);
    Ok(())
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

/// A decoded file with its text.
struct Loaded {
    src: Src,
    doc: Document,
}

/// Reads a document and the library files it uses, each once, as dt-eval's
/// reader does for the program (`Reader::load`), over the page's library map
/// instead of the disk.
struct Loader<'a> {
    library: &'a BTreeMap<String, String>,
    units: Units,
    files: BTreeMap<String, Arc<Loaded>>,
    loading: Vec<String>,
    errors: Vec<DocError>,
}

impl Loader<'_> {
    fn load(
        &mut self,
        path: &str,
        text: Option<&str>,
        from: Option<(&Src, SourceSpan)>,
    ) -> Option<Arc<Loaded>> {
        if let Some(f) = self.files.get(path) {
            return Some(Arc::clone(f));
        }
        if self.loading.iter().any(|p| p == path) {
            if let Some((src, span)) = from {
                self.errors.push(
                    DocError::new(
                        src,
                        "dt::use",
                        span,
                        format!("`{path}` uses itself, through {}", self.loading.join(" → ")),
                        "a loop of files",
                    )
                    .maybe_help(Some(
                        "files may use each other, but never round to themselves".into(),
                    )),
                );
            }
            return None;
        }
        let text = match text {
            Some(t) => t.to_owned(),
            None => match self.library.get(path) {
                Some(t) => t.clone(),
                None => {
                    if let Some((src, span)) = from {
                        self.errors.push(
                            DocError::new(
                                src,
                                "dt::use",
                                span,
                                format!("the library has no `{}`", path.trim_start_matches("lib:")),
                                "used here",
                            )
                            .maybe_help(Some(
                                "the page's library holds the bundled parts; `./` files of your project are not available here".into(),
                            )),
                        );
                        return None;
                    }
                    // a file the page was never given is the page's mistake, said by the caller
                    return None;
                }
            },
        };
        let src: Src = Arc::new(NamedSource::new(path, text.clone()));
        let uses = match dt_doc::uses(path, &text) {
            Ok(u) => u,
            Err(es) => {
                self.errors.extend(es);
                return None;
            }
        };
        let mut known = Known::new().with_units(self.units);
        if path != HEIGHTS {
            // every file sees the library's heights
            let heights = self.load(HEIGHTS, None, None)?;
            known.add_consts(&heights.doc);
        }
        let mut ok = true;
        self.loading.push(path.to_owned());
        for u in &uses {
            match self.load(&use_path(path, u), None, Some((&src, u.path.span))) {
                Some(f) => known.add(u.key(), &f.doc),
                None => ok = false,
            }
        }
        self.loading.pop();
        if !ok {
            return None;
        }
        match dt_doc::decode(path, &text, &known) {
            Ok(doc) => {
                let f = Arc::new(Loaded { src, doc });
                self.files.insert(path.to_owned(), Arc::clone(&f));
                Some(f)
            }
            Err(es) => {
                self.errors.extend(es);
                None
            }
        }
    }
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
}

fn diagnostic(e: &DocError) -> Diagnostic {
    let text = e.src().inner();
    let (o, l) = (e.span.offset(), e.span.len());
    let (line, col) = line_col(text, o);
    let s16 = utf16_at(text, o);
    let e16 = utf16_at(text, o + l);
    Diagnostic {
        code: e.code,
        message: e.message.clone(),
        label: e.label.clone(),
        help: e.help.clone(),
        file: e.src().name().to_owned(),
        span: [o, l],
        span16: [s16, e16 - s16],
        line,
        col,
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
/// or `ol`), and describe it: its diagnostics (code, message, help, spans),
/// its tile and its stack with each control as written and as printed on
/// both targets.  JSON.
///
/// # Errors
/// An unknown target, or the library was never set.
#[wasm_bindgen]
pub fn analyze(text: &str, system: &str) -> Result<String, JsError> {
    let units = units_for(system)?;
    let (db, ol) = (units_for("db")?, units_for("ol")?);
    LIBRARY.with(|lib| {
        let lib = lib.borrow();
        if !lib.contains_key(HEIGHTS) {
            return Err(JsError::new(
                "the library has no parts/heights.dtile: call set_library first",
            ));
        }
        let mut loader = Loader {
            library: &lib,
            units,
            files: BTreeMap::new(),
            loading: Vec::new(),
            errors: Vec::new(),
        };
        let main = loader.load(MAIN, Some(text), None);
        let diagnostics: Vec<Diagnostic> = loader.errors.iter().map(diagnostic).collect();
        let (Some(main), true) = (main, diagnostics.is_empty()) else {
            return json(&Analysis {
                ok: false,
                diagnostics,
                tile: None,
                stack: Vec::new(),
            });
        };
        let heights = loader
            .files
            .get(HEIGHTS)
            .ok_or_else(|| JsError::new("the heights file was not read"))?;
        // the library's heights are `$` names at the root, and the tile's own
        // `let`s are the first scope, as the program's reader makes it
        let library_consts: Vec<_> = heights
            .doc
            .consts
            .iter()
            .filter(|c| c.kind == ConstKind::Let)
            .cloned()
            .collect();
        let Some(def) = main.doc.tile() else {
            return json(&Analysis {
                ok: true,
                diagnostics,
                tile: None,
                stack: Vec::new(),
            });
        };
        let (consts, scope) = ConstTable::root(&library_consts).enter(&def.consts);
        let mut diagnostics = diagnostics;
        for s in &scope {
            let e = DocError::new(&main.src, s.code, s.span, s.message.clone(), s.label)
                .maybe_help(s.help.clone());
            diagnostics.push(diagnostic(&e));
        }
        let cx = Ctx {
            text,
            consts: &consts,
            db,
            ol,
        };
        let header = def.header();
        json(&Analysis {
            ok: diagnostics.is_empty(),
            diagnostics,
            tile: header.map(|h| TileInfo {
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
        let mut loader = Loader {
            library: &lib,
            units: db,
            files: BTreeMap::new(),
            loading: Vec::new(),
            errors: Vec::new(),
        };
        let Some(heights) = loader.load(HEIGHTS, None, None) else {
            let why: Vec<String> = loader.errors.iter().map(|e| e.message.clone()).collect();
            return Err(JsError::new(&format!(
                "the library's heights do not read: {}",
                why.join("; ")
            )));
        };
        let library_consts: Vec<_> = heights
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

    const HEIGHTS_TEXT: &str = "dharmatiles version=\"1.0.0\"\nlet $groundHeight \"5dmm\"\n";

    fn lib() {
        let mut m = BTreeMap::new();
        m.insert(HEIGHTS.to_owned(), HEIGHTS_TEXT.to_owned());
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
}
