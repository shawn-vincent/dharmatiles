//! Reading a document and its imports into a [`Tile`].

use std::collections::BTreeMap;
use std::sync::Arc;

use dt_doc::Known;
use dt_doc::consts::ConstTable;
use dt_doc::expr::Expr;
use dt_doc::model::{self, Definition, PlaceRef, Spanned, Value};
use dt_doc::place::{Along, Edge, Point};
use miette::{NamedSource, SourceSpan};

use crate::diag::{Diag, Src};
use crate::lower::Lets;
use crate::source::Source;

use super::model::*;

mod controls;
mod expand;

/// A document as read: the file, and its tile if it has one.
#[derive(Debug)]
pub struct Read {
    /// The file.
    pub file: Arc<File>,
    /// Its tile, if it has one (a parts or printer file has none).
    pub tile: Option<Tile>,
}

/// FNV-1a of a document's text (the build record's document hash).
#[must_use]
pub fn text_hash(text: &str) -> u64 {
    crate::hash::bytes(crate::hash::OFFSET, text.as_bytes())
}

pub use dt_doc::paths::use_path;

/// Read the document at `path` and its imports.  A tile is flattened
/// into a [`Tile`]; any other file (parts, printers, heights, edges) is
/// only decoded.
///
/// # Errors
/// Every problem found, each at its text.
pub fn read(sources: &dyn Source, path: &str) -> Result<Read, Vec<Diag>> {
    read_with(sources, path, dt_doc::units::Units::default())
}

/// [`read`] for a build target: `units` says what `mm`, `in` and `layer`
/// are worth in a document (the system's square, the printer's layer).
///
/// # Errors
/// Every problem found, each at its text.
pub fn read_with(
    sources: &dyn Source,
    path: &str,
    units: dt_doc::units::Units,
) -> Result<Read, Vec<Diag>> {
    let mut r = Reader {
        units,
        counts: BTreeMap::new(),
        sources,
        files: BTreeMap::new(),
        loading: Vec::new(),
        errors: Vec::new(),
        entries: 0,
        notes: Vec::new(),
        top: None,
        dims: (1.0, 1.0),
        points: BTreeMap::new(),
        widths: BTreeMap::new(),
    };
    let Some(file) = r.load(path, None) else {
        return Err(r.errors);
    };
    let tile = r.tile(&file);
    if r.errors.is_empty() {
        Ok(Read { file, tile })
    } else {
        Err(r.errors)
    }
}

/// The printer profile at `path`, as written.
///
/// # Errors
/// Every problem found; a file with no `printer` is one.
pub fn printer_doc(sources: &dyn Source, path: &str) -> Result<dt_doc::model::Printer, Vec<Diag>> {
    let r = read(sources, path)?;
    let Some(p) = r.file.doc.printer.clone() else {
        return Err(vec![
            Diag::plain(dt_lang::codes::PRINTER, format!("`{path}` has no `printer`"))
                .help("a printer file holds `printer name { line …; layer …; overhang …; steepest …; gap … }`, or a machine: `bed \"256x256mm\"` and the rest"),
        ]);
    };
    Ok(p)
}

/// The printer profile at `path`, for the audit gate.
///
/// # Errors
/// Every problem found; a file with no `printer` is one.
pub fn printer(sources: &dyn Source, path: &str) -> Result<dt_kit::Printer, Vec<Diag>> {
    let p = printer_doc(sources, path)?;
    Ok(
        dt_kit::Printer::new(p.line.value, p.layer.value, p.overhang.value, p.gap.value)
            .steepest(p.steepest.value),
    )
}

/// A place word an element does not take, where it is written.
pub(super) fn misplaced(src: &Src, span: SourceSpan, word: &str, on: &str) -> Diag {
    Diag::at(
        src,
        span,
        dt_lang::codes::PLACE,
        format!("`{word}` does not place `{on}`"),
        "not a place word for this",
    )
    .help("a scattered thing is placed by its scatter: put `at=` on the `scatter` line")
}

/// "`what` not available yet", at `span`: the language has it, the
/// elements do not build it yet.
pub(super) fn not_yet(src: &Src, span: SourceSpan, what: &str) -> Diag {
    Diag::at(
        src,
        span,
        dt_lang::codes::NOT_YET,
        format!("{what} not available yet"),
        "not built yet",
    )
    .help("`check` accepts this; `build` does not, yet")
}

/// A read that cannot be satisfied, at its text.
pub(super) fn read_flaw(src: &Src, span: SourceSpan, flaw: &dt_doc::consts::ReadFlaw) -> Diag {
    use dt_doc::consts::ReadFlaw;
    match flaw {
        ReadFlaw::Unknown { name, near } => {
            let d = Diag::at(
                src,
                span,
                dt_lang::codes::UNKNOWN_NAME,
                format!("nothing installed `{name}` where this is read"),
                "not in the context",
            );
            match near {
                Some(n) => d
                    .suggest(dt_doc::error::locate(src.inner(), span, name), n)
                    .also(Some("a `$` name is read from the context of whoever calls, and the library installs its heights at the root")),
                None => d.help(format!("a `$` name is read from the context of whoever calls: the library installs its heights at the root, and `let {name} …` in an enclosing block installs one of your own")),
            }
        }
        ReadFlaw::PerPoint { name } => Diag::at(
            src,
            span,
            dt_lang::codes::TYPE,
            format!("`{name}` is worked out at each point, and this is drawn once"),
            "a count or size is not per point",
        )
        .help("a position or a height under goes where a height is worked out at each point: a top, a pin or a level"),
        ReadFlaw::NotSet {
            name,
            element,
            sets,
        } => Diag::at(
            src,
            span,
            dt_lang::codes::UNKNOWN_NAME,
            format!("`{element}` does not set `{name}`"),
            "not set here",
        )
        .help(format!(
            "`{element}` sets {}; a body (dirt, water, a floor, a cliff) works its top out at each node and sets the position and the depth and angle round its region, a tree's top is worked out once over the ground at its foot",
            sets.join(", ")
        )),
        ReadFlaw::Loop => Diag::at(
            src,
            span,
            dt_lang::codes::VALUE,
            "names read each other in a loop",
            "a loop",
        )
        .help("a `let` that reads itself needs `override`, which reads the value it replaces"),
    }
}

/// Where a formula is read: drawn once (a count, a size), or worked out at
/// each point by an element, which sets only the names
/// [`dt_doc::words::sets`] says.
#[derive(Clone, Copy)]
enum Reads<'a> {
    Once,
    At(&'a str),
}

/// A parameter's value and the text it was written in.
#[derive(Clone)]
struct Bound {
    value: Spanned<Value>,
    src: Src,
}

/// What an inlined definition's entries see.
#[derive(Clone)]
struct Scope {
    file: Arc<File>,
    params: BTreeMap<String, Bound>,
    /// The tile's `let`s (library heights included): numbers in mm, and the
    /// ranges they hold.
    lets: Arc<Lets>,
    area: Option<Spanned<String>>,
    /// Place words a definition call passes to the elements inside it.
    at: Option<Spanned<Point>>,
    along: Option<Spanned<Along>>,
    /// A definition call's `top=`, for the tree inside it, and where it
    /// was written.  The outermost call's wins.
    top: Option<(Spanned<Expr>, Src)>,
    seed: Option<u64>,
    id: Option<String>,
    depth: usize,
}

struct Reader<'s> {
    sources: &'s dyn Source,
    files: BTreeMap<String, Arc<File>>,
    loading: Vec<String>,
    errors: Vec<Diag>,
    /// Stack entries met so far while inlining, definitions included.
    entries: usize,
    /// Notes (nothing wrong, but look): a weighting on a range of no width.
    notes: Vec<Diag>,
    /// The tile's own entry being inlined (where a blow-up is reported).
    top: Option<(Src, SourceSpan)>,
    /// The tile's squares across and up, for edge paths and points.
    dims: (f64, f64),
    /// Every point of the tile by name (built in and declared), squares.
    points: BTreeMap<String, [f64; 2]>,
    /// The width of each line that has one, mm at the reference square.
    widths: BTreeMap<String, f64>,
    /// What `mm`, `in` and `layer` are worth (format 1).
    units: dt_doc::units::Units,
    /// How many entries of each element are in the stack so far: an entry
    /// without an `id=` is named by its count.
    counts: BTreeMap<String, usize>,
}

/// The most stack entries a tile may expand to, definitions inlined.  A
/// definition calling another eight times, six deep, is 262 144 entries
/// from seven short lines; this stops it with one error.
pub const MAX_ENTRIES: usize = 10_000;

impl Reader<'_> {
    /// Decode `path` after the files it uses; each file once.
    fn load(&mut self, path: &str, from: Option<(&Src, SourceSpan)>) -> Option<Arc<File>> {
        if let Some(f) = self.files.get(path) {
            return Some(Arc::clone(f));
        }
        let at = |code, msg: String, label: &str, help: Option<String>| match from {
            Some((src, span)) => Diag::at(src, span, code, msg, label).maybe_help(help),
            None => Diag::plain(code, msg).maybe_help(help),
        };
        if self.loading.iter().any(|p| p == path) {
            self.errors.push(at(
                dt_lang::codes::USE,
                format!("`{path}` uses itself, through {}", self.loading.join(" → ")),
                "a loop of files",
                Some("files may use each other, but never round to themselves".into()),
            ));
            return None;
        }
        let text = match self.sources.read(path) {
            Ok(t) => t,
            Err(e) => {
                // the document asked for itself, or a file a `use` names
                let code = if from.is_some() {
                    dt_lang::codes::USE
                } else {
                    dt_lang::codes::SOURCE
                };
                self.errors.push(at(
                    code,
                    e,
                    "used here",
                    from.map(|_| {
                        "a bare path or `lib:` is the bundled library; `./` and `../` are your project, relative to this file".into()
                    }),
                ));
                return None;
            }
        };
        let src: Src = Arc::new(NamedSource::new(path, text.clone()));
        let uses = match dt_doc::uses(path, &text) {
            Ok(u) => u,
            Err(es) => {
                self.errors.extend(es.iter().map(Diag::from));
                return None;
            }
        };
        let mut known = Known::new().with_units(self.units);
        if path != HEIGHTS {
            // every file sees the library's heights
            let heights = self.load(HEIGHTS, None)?;
            known.add_consts(&heights.doc);
        }
        let mut ok = true;
        self.loading.push(path.to_owned());
        for u in &uses {
            match self.load(&use_path(path, u), Some((&src, u.path.span))) {
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
                let f = Arc::new(File {
                    path: path.to_owned(),
                    src,
                    doc,
                });
                self.files.insert(path.to_owned(), Arc::clone(&f));
                Some(f)
            }
            Err(es) => {
                self.errors.extend(es.iter().map(Diag::from));
                None
            }
        }
    }

    /// What went wrong entering a scope, each at its text in `src`.
    fn scope_errors(&mut self, src: &Src, errors: Vec<dt_doc::consts::ScopeError>) {
        for e in errors {
            let d = Diag::at(src, e.span, e.code, e.message, e.label);
            self.errors.push(match e.near {
                // the suggestion first, then what else the error has to say
                Some(w) => d.suggest(e.span, &w).also(e.help.as_deref()),
                None => d.maybe_help(e.help),
            });
        }
    }

    /// The scope a block makes: `scope` with the block's `let`s and
    /// `override`s.
    fn scoped(&mut self, scope: &Scope, consts: &[model::Const]) -> Scope {
        let Some(table) = scope.lets.table.as_ref() else {
            return scope.clone();
        };
        if consts.is_empty() {
            return scope.clone();
        }
        let (table, errors) = table.enter(consts);
        self.scope_errors(&scope.file.src, errors);
        Scope {
            lets: Arc::new(Lets::from_consts(&table)),
            ..scope.clone()
        }
    }

    /// Whether `expr`, written at `span` in `src`, can be read in the
    /// context `table`: every `$` name is installed, and a
    /// position or height under is only read where a height is worked out
    /// at each point.  A mistake is reported at its text.
    fn readable(
        &mut self,
        table: &ConstTable,
        expr: &Expr,
        span: SourceSpan,
        src: &Src,
        reads: Reads<'_>,
    ) -> bool {
        let checked = match reads {
            Reads::Once => table.check_reads(expr, false),
            Reads::At(element) => table.check_reads_for(expr, element),
        };
        match checked {
            Ok(()) => true,
            Err(flaw) => {
                self.errors.push(read_flaw(src, span, &flaw));
                false
            }
        }
    }

    fn tile(&mut self, file: &Arc<File>) -> Option<Tile> {
        let def = file.doc.tile()?;
        let header = def.header()?;
        let src = &file.src;
        let library = self.load(HEIGHTS, None)?;
        let library_consts: Vec<dt_doc::model::Const> = library
            .doc
            .consts
            .iter()
            .filter(|c| c.kind == dt_doc::model::ConstKind::Let)
            .cloned()
            .collect();
        // the library's heights are `$` names at the root, and the tile's
        // `let`s and `override`s are its first scope
        let (consts, errors) = ConstTable::root(&library_consts).enter(&def.consts);
        self.scope_errors(src, errors);
        let lets = Arc::new(Lets::from_consts(&consts));
        let seed = match header.seed {
            None => 1,
            Some(s) => u64::try_from(s).unwrap_or_else(|_| {
                self.errors.push(Diag::at(
                    src,
                    def.name.span,
                    dt_lang::codes::SEED,
                    format!("a tile seed is a whole number from 0, not {s}"),
                    "this tile",
                ));
                1
            }),
        };
        let mut t = Tile {
            cols: header.cols,
            rows: header.rows,
            seed,
            lines: BTreeMap::new(),
            widths: BTreeMap::new(),
            drawn_lines: BTreeMap::new(),
            wobbles: BTreeMap::new(),
            regions: Vec::new(),
            heights: Heights::default(),
            has_base: def.base.is_some(),
            base_span: def.base.as_ref().map(|b| b.span),
            stack: Vec::new(),
            ids: BTreeMap::new(),
            pending: Vec::new(),
            notes: Vec::new(),
            src: Arc::clone(src),
            span: def.name.span,
        };
        let (cols, rows) = (f64::from(t.cols), f64::from(t.rows));
        self.dims = (cols, rows);
        self.points = BTreeMap::new();
        self.widths = BTreeMap::new();
        for n in dt_doc::place::BUILTIN_POINTS {
            if let Some((x, y)) = dt_doc::place::builtin_point(n, cols, rows) {
                self.points.insert(n.to_owned(), [x, y]);
            }
        }
        for p in &def.points {
            self.points.insert(p.name.value.clone(), [p.x, p.y]);
        }
        for l in &def.lines {
            if let Some(w) = l.width.as_ref()
                && self.readable(&consts, &w.value.expr, w.span, src, Reads::Once)
            {
                match wobble_mm(&w.value, &lets) {
                    Ok(v) if v.is_finite() && v > 0.0 && v <= MAX_LINE_WIDTH_MM => {
                        self.widths.insert(l.name.value.clone(), v);
                        t.widths.insert(l.name.value.clone(), v);
                        t.regions.push(Region {
                            name: strip_name(&l.name.value),
                            bounded_by: Vec::new(),
                            fills: Vec::new(),
                            fill_draws: Vec::new(),
                            within: vec![Within {
                                mm: v / 2.0,
                                range: None,
                                of: PlaceRef::Name(l.name.value.clone()),
                                span: w.span,
                            }],
                            except: Vec::new(),
                            span: l.name.span,
                            strip: true,
                        });
                    }
                    got => {
                        let why = got.err().map_or_else(
                            || format!("a line's width is above 0 and up to {MAX_LINE_WIDTH_MM} mm"),
                            |e| format!("a line's width is a length above 0, up to {MAX_LINE_WIDTH_MM} mm: {e}"),
                        );
                        self.errors
                            .push(Diag::at(src, w.span, dt_lang::codes::VALUE, why, "here"));
                    }
                }
            }
            let wobble_read = l
                .wobble
                .as_ref()
                .is_none_or(|w| self.readable(&consts, &w.value.expr, w.span, src, Reads::Once));
            let wobble = match l
                .wobble
                .as_ref()
                .filter(|_| wobble_read)
                .map(|w| (w, wobble_mm(&w.value, &lets)))
            {
                None => 0.0,
                Some((_, Ok(v))) if v.is_finite() && (0.0..=35.0).contains(&v) => v,
                Some((w, got)) => {
                    // a value out of range, or one that is not a length
                    // (an unknown level): say which
                    let why = got.err().map_or_else(
                        || "a line's wobble is from 0 to 35 mm".to_owned(),
                        |e| format!("a line's wobble is a length from 0 to 35 mm: {e}"),
                    );
                    self.errors
                        .push(Diag::at(src, w.span, dt_lang::codes::VALUE, why, "here"));
                    0.0
                }
            };
            let mut spots = Vec::new();
            for p in &l.through {
                match spot(&p.value, cols, rows, &self.points) {
                    Ok(sp) => spots.push(sp),
                    Err(r) => {
                        let mut d = Diag::at(src, p.span, r.code, r.message, "here");
                        d.help = r.help;
                        self.errors.push(d);
                    }
                }
            }
            let line = DrawnLine {
                spots,
                closed: l.closed,
                wobble,
            };
            let (base, pts) = line.lay(t.seed, &l.name.value);
            if wobble > 0.0 {
                t.wobbles.insert(l.name.value.clone(), (base, wobble));
            }
            if line.spots.iter().any(|s| matches!(s, Spot::Drawn(_))) {
                t.drawn_lines.insert(l.name.value.clone(), line);
            }
            t.lines.insert(l.name.value.clone(), pts);
        }
        for r in &def.regions {
            let mut fills = Vec::new();
            let mut fill_draws = Vec::new();
            for p in &r.fills {
                match spot(&p.value, cols, rows, &self.points) {
                    Ok(Spot::Fixed(xy)) => fills.push(xy),
                    Ok(Spot::Drawn(d)) => {
                        fill_draws.push((fills.len(), d.clone()));
                        fills.push(draw_spot(&d, fill_key(t.seed, &r.name.value, fills.len())));
                    }
                    Err(e) => {
                        let mut d = Diag::at(src, p.span, e.code, e.message, "here");
                        d.help = e.help;
                        self.errors.push(d);
                    }
                }
            }
            let mut within = Vec::new();
            for w in &r.within {
                if !self.readable(&consts, &w.mm.value, w.mm.span, src, Reads::Once) {
                    continue;
                }
                // a distance is a band: the odds of a weighting have nothing to act on
                if let Some(text) = consts.weighted_in(&w.mm.value) {
                    let mut d = Diag::at(
                        src,
                        w.mm.span,
                        dt_lang::codes::WEIGHTING,
                        format!("a weighted range does not go on a band: `{text}` in a distance"),
                        "within is a band, not a draw",
                    );
                    d.help = Some("write the range alone".to_owned());
                    self.errors.push(d);
                    continue;
                }
                let range = if w.mm.value.is_range() {
                    match (
                        consts.scalar(&w.mm.value, 0.0, &|_| None),
                        consts.scalar(&w.mm.value, 1.0, &|_| None),
                    ) {
                        (Ok(a), Ok(b)) if a.is_finite() && b.is_finite() && a >= 0.0 => {
                            Some((a.min(b), a.max(b)))
                        }
                        _ => None,
                    }
                } else {
                    None
                };
                match consts.scalar(&w.mm.value, 0.5, &|_| None) {
                    Ok(mm) if mm >= 0.0 && mm.is_finite() => within.push(Within {
                        mm,
                        range,
                        of: w.of.value.clone(),
                        span: w.of.span,
                    }),
                    Ok(_) => self.errors.push(Diag::at(
                        src,
                        w.mm.span,
                        dt_lang::codes::VALUE,
                        "a distance is not negative",
                        "here",
                    )),
                    Err(msg) => self.errors.push(Diag::at(
                        src,
                        w.mm.span,
                        dt_lang::codes::VALUE,
                        format!("can't work out this distance: {msg}"),
                        "here",
                    )),
                }
            }
            t.regions.push(Region {
                name: r.name.value.clone(),
                bounded_by: r.bounded_by.clone(),
                fills,
                fill_draws,
                within,
                except: r.except.iter().map(|e| e.value.clone()).collect(),
                span: r.name.span,
                strip: false,
            });
        }
        let scope = Scope {
            file: Arc::clone(file),
            params: BTreeMap::new(),
            lets,
            area: None,
            at: None,
            along: None,
            top: None,
            seed: None,
            id: None,
            depth: 0,
        };
        let mut stack = Vec::new();
        let mut pending = Vec::new();
        let stack_scope = self.scoped(&scope, &def.stack_consts);
        self.expand(&def.stack, &stack_scope, &mut stack, &mut pending);
        t.stack = stack;
        t.pending.extend(pending);
        t.notes = std::mem::take(&mut self.notes);
        t.heights = self.heights(def, consts);
        t.ids = t
            .stack
            .iter()
            .enumerate()
            .map(|(i, e)| (e.id.clone(), i))
            .collect();
        Some(t)
    }

    /// What the heights language wrote in the tile besides its entries: its
    /// constants, points and the library's defaults.
    fn heights(&mut self, def: &Definition, consts: ConstTable) -> Heights {
        let mut defaults = Vec::new();
        if let Some(f) = self.load(EDGES, None) {
            let dfrom = Provenance {
                path: f.path.clone(),
                src: Arc::clone(&f.src),
                default: true,
            };
            for d in &f.doc.defaults {
                defaults.push(DefaultBody {
                    element: d.what.value.clone(),
                    top: d.top.as_ref().map(|t| (t.value.clone(), t.span)),
                    pins: d.pins.iter().map(|p| pin_decl(p, &dfrom)).collect(),
                    from: dfrom.clone(),
                });
            }
        }
        Heights {
            consts,
            points: self.points.clone(),
            closed: def
                .lines
                .iter()
                .filter(|l| l.closed)
                .map(|l| l.name.value.clone())
                .collect(),
            defaults,
        }
    }

    /// A point written in a part (which has no tile) now that the tile it is
    /// called in is known: in squares from the south-west corner.  Its frame
    /// is `$tile`, or `$region` where the call acts on the whole tile
    /// (`in_region` false); a named region's box is known only once `check`
    /// has the region tree, and a thing is placed before that.
    fn in_tile_frame(
        &self,
        f: &dt_doc::framed::Framed,
        in_region: bool,
    ) -> Result<(Expr, Expr), crate::lower::Refused> {
        use dt_doc::expr::Expr as E;
        let (cols, rows) = self.dims;
        let tile = match &f.frame {
            E::Name(n) if n == "$tile" => true,
            E::Name(n) if n == "$region" => !in_region,
            _ => false,
        };
        if !tile {
            return Err(crate::lower::Refused::mistake(format!(
                "`{}` is in the frame of a region, and this part is called where the thing is placed before the region's box is known",
                f.frame
            ))
            .with_help(
                "write the point against the tile: `point($tile, 50%, 50%)`, or call the part on the whole tile",
            ));
        }
        match f.xy_in([0.0, 0.0, cols, rows], self.units) {
            Ok(Some(xy)) => Ok(xy),
            Ok(None) => {
                let dt_doc::framed::FramedAt::Polar { d, a_deg } = &f.at else {
                    return Err(crate::lower::Refused::mistake("this point has no place"));
                };
                let r = d * 0.5 * cols.min(rows);
                let (cx, cy) = (0.5 * cols, 0.5 * rows);
                Ok((
                    E::num(cx + r * dt_doc::expr::cos_deg(*a_deg)),
                    E::num(cy - r * dt_doc::expr::sin_deg(*a_deg)),
                ))
            }
            Err(ke) => {
                let r = crate::lower::Refused::mistake(ke.message);
                Err(match ke.help {
                    Some(h) => r.with_help(h),
                    None => r,
                })
            }
        }
    }
}

/// A pin as the evaluator keeps it.
pub(super) fn pin_decl(p: &model::Pin, from: &Provenance) -> PinDecl {
    PinDecl {
        name: p.name.as_ref().map(|n| n.value.clone()),
        top: p.top.value.clone(),
        top_span: p.top.span,
        at: p.at.value.clone(),
        at_span: p.at.span,
        span: p.span,
        from: from.clone(),
    }
}

/// A point in squares, if it is not drawn from a range.
pub(super) fn fixed_point(
    p: &dt_doc::place::Point,
    cols: f64,
    rows: f64,
    named: &BTreeMap<String, [f64; 2]>,
) -> Option<[f64; 2]> {
    if let dt_doc::place::Point::Named(n) = p {
        return named.get(n).copied();
    }
    if p.is_range() {
        return None;
    }
    let ((x, _), (y, _)) = p.bounds(cols, rows).ok()?;
    Some([x, y])
}

/// A path of tile edges (each meeting the next at a corner) as one
/// polyline in squares, walked with the tile inside on the left: the
/// path a run's outer face follows (`north west` on a 1×1 is
/// `(1,1) → (0,1) → (0,0)`, the spine of a north-west corner
/// wall). A path written clockwise is walked the other way round.
#[must_use]
pub fn edge_path(edges: &[Edge], (cols, rows): (f64, f64)) -> Vec<[f64; 2]> {
    // each edge anticlockwise round the tile: the inside on the left
    let walk = |e: Edge| match e {
        Edge::South => ([0.0, 0.0], [cols, 0.0]),
        Edge::East => ([cols, 0.0], [cols, rows]),
        Edge::North => ([cols, rows], [0.0, rows]),
        Edge::West => ([0.0, rows], [0.0, 0.0]),
    };
    let mut order: Vec<Edge> = edges.to_vec();
    #[allow(clippy::float_cmp)]
    let follows = |a: Edge, b: Edge| walk(a).1 == walk(b).0;
    if order.len() > 1 && !follows(order[0], order[1]) {
        order.reverse();
    }
    let mut out = Vec::with_capacity(order.len() + 1);
    if let Some(&first) = order.first() {
        out.push(walk(first).0);
    }
    out.extend(order.iter().map(|&e| walk(e).1));
    out
}

/// A line's wobble: one number of mm, its names the tile's `let`s.
pub(super) fn wobble_mm(n: &model::Num, lets: &Lets) -> Result<f64, String> {
    if let Some(x) = n
        .expr
        .names()
        .into_iter()
        .find(|x| lets.draws.iter().any(|(k, _)| k == x))
    {
        return Err(format!("`{x}` is a range; one number is needed here"));
    }
    n.mm(&lets.nums)
}

/// A formula with every name it reads that the context holds replaced by
/// what it holds, so what a tree's top is can be read, compared and keyed
/// without the context.  The names a place sets (`$x`, `$d`, `$bottom`, …)
/// and the regions stay.
pub(super) fn expand_names(e: &Expr, table: &ConstTable, depth: usize) -> Expr {
    e.map_names(&mut |n| {
        let stays = matches!(n, "x" | "y" | "bottom" | "$_y")
            || dt_doc::words::POINT_NAMES.contains(&n)
            || dt_doc::words::REGION_NAMES.contains(&n);
        if stays || depth > 64 {
            return None;
        }
        table
            .get(n)
            .map(|c| expand_names(&c.expr, table, depth + 1))
    })
}
