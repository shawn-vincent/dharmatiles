//! The document walk of the 0 → 1 migration: every value of a format 0
//! document, found by what its place wants of it, written as format 1 by
//! text edits so everything else (comments, layout, order) stays as it was.

use std::collections::{HashMap, HashSet};

use kdl::{KdlDocument, KdlEntry, KdlNode, KdlValue};
use miette::SourceSpan;

use super::adorn::{DESIGN, Flaw, Names, SQUARES, Want, adorn, flip_y, kind_of, show};
use crate::decode::{arg_str, value_span};
use crate::error::{DocError, Src};
use crate::expr::{self, Expr};
use crate::known::DefInfo;
use crate::place::{self, BUILTIN_LINES, BUILTIN_POINTS, BUILTIN_REGION, Corner, Edge, Point};
use crate::schema::{self, Axis, Control, Element};
use crate::units::Kind;

/// One change to the text.
pub(super) struct Edit {
    pub start: usize,
    pub end: usize,
    pub with: String,
}

/// A plain name a document declares (a `let` or a `param`), where, and what
/// it holds.
struct Decl {
    name: String,
    span: SourceSpan,
    value: Option<Expr>,
    what: &'static str,
}

/// What a `tile` or a `define` is read in.
#[derive(Default)]
struct Scope {
    rows: Option<f64>,
    /// Region and line names, whose frame a pin's point is in.
    framed: HashSet<String>,
    /// Every place the document names: a point there is a name, not a pair.
    places: HashSet<String>,
}

pub(super) struct Walk {
    pub src: Src,
    pub edits: Vec<Edit>,
    pub problems: Vec<DocError>,
    pub names: Names,
    pub defs: HashMap<String, DefInfo>,
    scope: Scope,
    decls: Vec<Decl>,
    /// A plain name that collides with a word of the language, and what it
    /// becomes.
    renames: HashMap<String, String>,
    /// Kinds that disagree: a name used as a length and as a count.
    conflicts: HashMap<String, Vec<Kind>>,
}

fn quote(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn ident_safe(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '$' | '_' | '-' | '.'))
        && !s.starts_with(|c: char| c.is_ascii_digit())
}

fn size_of(node: &KdlNode) -> Option<(f64, f64)> {
    let s = node.entry("size")?.value().as_string()?;
    let (c, r) = s.split_once('x')?;
    Some((c.trim().parse().ok()?, r.trim().parse().ok()?))
}

fn num_of(v: &KdlValue) -> Option<f64> {
    match v {
        KdlValue::Integer(i) => i.to_string().parse().ok(),
        KdlValue::Float(f) => Some(*f),
        _ => None,
    }
}

impl Walk {
    pub fn new(src: Src, defs: HashMap<String, DefInfo>) -> Self {
        Self {
            src,
            edits: Vec::new(),
            problems: Vec::new(),
            names: Names::default(),
            defs,
            scope: Scope::default(),
            decls: Vec::new(),
            renames: HashMap::new(),
            conflicts: HashMap::new(),
        }
    }

    fn problem(
        &mut self,
        code: &'static str,
        span: SourceSpan,
        message: impl Into<String>,
        label: &str,
        help: Option<String>,
    ) {
        let mut e = DocError::new(&self.src, code, span, message, label);
        e.help = help;
        self.problems.push(e);
    }

    fn flaw_at(&mut self, span: SourceSpan, f: Flaw) {
        self.problem(
            "dt::migrate-by-hand",
            span,
            f.message,
            "no format 1 form",
            f.help,
        );
    }

    fn put(&mut self, span: SourceSpan, with: String) {
        self.edits.push(Edit {
            start: span.offset(),
            end: span.offset() + span.len(),
            with,
        });
    }

    fn put_value(&mut self, e: &KdlEntry, text: &str) {
        self.put(value_span(e), quote(text));
    }

    fn put_name(&mut self, e: &KdlEntry, text: &str) {
        let quoted = e
            .format()
            .is_some_and(|f| f.value_repr.starts_with(['"', '#']));
        let with = if quoted || !ident_safe(text) {
            quote(text)
        } else {
            text.to_owned()
        };
        self.put(value_span(e), with);
    }

    // ---- values ---------------------------------------------------------

    /// The value an entry holds, as an expression: a number, or a quoted
    /// one.  `None` (and a problem) when it is not readable.
    fn parse_entry(&mut self, e: &KdlEntry, v2: bool) -> Option<Expr> {
        let span = value_span(e);
        match e.value() {
            v @ (KdlValue::Integer(_) | KdlValue::Float(_)) => Some(Expr::num(num_of(v)?)),
            KdlValue::String(s) => {
                let parsed = if v2 {
                    expr::parse_v2(s)
                } else {
                    expr::parse(s)
                };
                match parsed {
                    Ok(x) => Some(x),
                    Err(err) => {
                        self.problem(
                            "dt::migrate-by-hand",
                            span,
                            format!("can't read this value to migrate it: {}", err.message),
                            "here",
                            None,
                        );
                        None
                    }
                }
            }
            _ => None,
        }
    }

    /// An entry's value written for `want`: lengths and angles take their
    /// unit, names their `$`.  `v2` is the heights language.
    fn value(&mut self, e: &KdlEntry, want: Want, v2: bool) {
        let Some(old) = self.parse_entry(e, v2) else {
            return;
        };
        let span = value_span(e);
        let mut old = old;
        if let Some(t) = e.ty() {
            match t.value() {
                "mm" => {}
                "h" => match scale_h(&old) {
                    Some(x) => old = x,
                    None => {
                        self.problem(
                            "dt::migrate-by-hand",
                            span,
                            "`(h)` on a formula has no format 1 form",
                            "wall tops",
                            Some("write the length in `dmm` (1 wall top is 33.1)".to_owned()),
                        );
                        return;
                    }
                },
                _ => return,
            }
        }
        let orig = old.clone();
        let old = self.renamed(old);
        let new = match adorn(&old, want, &self.names, DESIGN) {
            Ok(n) => n,
            Err(f) => {
                self.flaw_at(span, f);
                return;
            }
        };
        if new != orig || e.ty().is_some() {
            self.put_value(e, &show(&new, v2));
        }
    }

    /// Plain names a reserved word forced to another name.
    fn renamed(&self, e: Expr) -> Expr {
        if self.renames.is_empty() {
            return e;
        }
        e.map_names(&mut |n| self.renames.get(n).map(|r| Expr::Name(r.clone())))
    }

    /// A control's value, by what its axis wants.
    fn control_value(&mut self, e: &KdlEntry, k: &'static Control) {
        let Some(axis) = k.value else { return };
        let word = e
            .value()
            .as_string()
            .is_some_and(|s| k.words.iter().any(|w| w.word == s));
        if word {
            return;
        }
        self.axis_value(e, axis);
    }

    fn axis_value(&mut self, e: &KdlEntry, axis: Axis) {
        match axis {
            Axis::Mm => self.value(e, Want::Length, false),
            Axis::Deg => self.value(e, Want::Angle, false),
            Axis::Share | Axis::Count | Axis::PerSquare | Axis::Number => {
                // plain: the units stay as they are; the names that are
                // there are read as plain numbers (a count the parameter
                // fills), which fixes what the parameter is
                if let Some(old) = self.parse_entry(e, false)
                    && !old.names().is_empty()
                {
                    let renamed = self.renamed(old.clone());
                    let new = adorn(&renamed, Want::Plain, &self.names, DESIGN)
                        .unwrap_or_else(|_| renamed.clone());
                    if new != old {
                        self.put_value(e, &show(&new, false));
                    }
                }
            }
            Axis::Word | Axis::Bool | Axis::Tag => {
                if let Some(s) = e.value().as_string() {
                    self.names.note(s, Kind::Any);
                }
            }
        }
    }

    // ---- points ---------------------------------------------------------

    /// A place written as a point, in the north-west frame; `None` when it
    /// is a name, a word, an edge path or `foot(…)`.
    fn point_text(&mut self, s: &str, span: SourceSpan, framed: bool) -> Option<String> {
        let t = s.trim();
        if t.starts_with("foot(")
            || self.scope.places.contains(t)
            || Corner::parse(t).is_some()
            || Edge::parse(t).is_some()
            || t.split_whitespace().all(|w| Edge::parse(w).is_some())
        {
            return None;
        }
        let Ok(Point::Pair { x, y }) = place::parse_point(s) else {
            return None;
        };
        let Some(rows) = self.scope.rows else {
            self.problem(
                "dt::migrate-by-hand",
                span,
                "a point in a part has no size to count down from the north against",
                "no tile here",
                Some(
                    "write it in the north-west frame by hand: `(x, rows - y)`, or a `point($tile, …)`"
                        .to_owned(),
                ),
            );
            return None;
        };
        let xs = match adorn(&x, Want::Length, &self.names, SQUARES) {
            Ok(x) => x,
            Err(f) => {
                self.flaw_at(span, f);
                return None;
            }
        };
        let ys = match flip_y(&y, rows, &self.names) {
            Ok(y) => y,
            Err(f) => {
                self.flaw_at(span, f);
                return None;
            }
        };
        Some(if framed {
            format!("point($tile, {}, {})", show(&xs, true), show(&ys, true))
        } else {
            format!("({}, {})", show(&xs, false), show(&ys, false))
        })
    }

    fn point_entry(&mut self, e: &KdlEntry, framed: bool) {
        let Some(s) = e.value().as_string() else {
            return;
        };
        let span = value_span(e);
        if let Some(t) = self.point_text(s, span, framed) {
            self.put_value(e, &t);
        }
    }

    // ---- entries --------------------------------------------------------

    fn entry_node(&mut self, n: &KdlNode) {
        let name = n.name().value();
        let el: Option<&'static Element> = schema::element(name);
        let def = if el.is_none() {
            self.defs.get(name).cloned()
        } else {
            None
        };
        // the place a pin's point is in: the region (or the line's strip)
        // the entry acts in
        let framed = n
            .entry("at")
            .and_then(|e| e.value().as_string())
            .is_some_and(|s| self.scope.framed.contains(s.trim()));
        for e in n.entries() {
            let Some(key) = e.name().map(|k| k.value().to_owned()) else {
                continue;
            };
            self.property(el, def.as_ref(), &key, e);
        }
        for c in n.iter_children() {
            let cname = c.name().value();
            if cname == "pin" {
                self.pin_node(c, framed);
                continue;
            }
            let control = el.and_then(|l| l.control(cname));
            let param = def
                .as_ref()
                .and_then(|d| d.params.iter().find(|p| p.name == cname));
            if control.is_some() || param.is_some() {
                self.control_node(c, control, param.and_then(|p| p.control));
                continue;
            }
            match cname {
                "slope" => {
                    if let [e] = c.entries() {
                        self.value(e, Want::Angle, false);
                    }
                }
                "at" => {
                    if let [e] = c.entries() {
                        self.point_entry(e, false);
                    }
                }
                "side" | "seed" | "id" | "weight" => {}
                _ => self.entry_node(c),
            }
        }
    }

    fn property(
        &mut self,
        el: Option<&'static Element>,
        def: Option<&DefInfo>,
        key: &str,
        e: &KdlEntry,
    ) {
        match key {
            "top" => {
                let word = e.value().as_string().is_some_and(|s| {
                    el.and_then(|l| l.control("top"))
                        .is_some_and(|k| k.words.iter().any(|w| w.word == s))
                });
                if !word {
                    self.value(e, Want::Length, true);
                }
            }
            "slope" => self.value(e, Want::Angle, false),
            "at" => {
                let framed = false;
                self.point_entry(e, framed);
            }
            "side" | "seed" | "id" | "weight" => {}
            _ => {
                if let Some(k) = el.and_then(|l| l.control(key)) {
                    self.control_value(e, k);
                } else if let Some(d) = def
                    && let Some(p) = d.params.iter().find(|p| p.name == key)
                    && let Some(k) = p.control
                {
                    self.control_value(e, k);
                }
            }
        }
    }

    /// A control written on its own line: its value, then sub-controls as
    /// properties.
    fn control_node(
        &mut self,
        c: &KdlNode,
        control: Option<&'static Control>,
        param: Option<&'static Control>,
    ) {
        let k = control.or(param);
        let Some(k) = k else { return };
        for e in c.entries() {
            match e.name() {
                None => {
                    if k.value.is_some() {
                        self.control_value(e, k);
                    }
                }
                Some(sub) => {
                    if let Some(s) = k.sub(sub.value()) {
                        self.control_value(e, s);
                    }
                }
            }
        }
    }

    /// `pin NAME top=… at=…`.  A point in a pin is in the frame of the
    /// region the body acts in, so a literal point there is written with the
    /// tile as its frame: the meaning stays what it was.
    fn pin_node(&mut self, c: &KdlNode, framed: bool) {
        for e in c.entries() {
            match e.name().map(|k| k.value()) {
                Some("top") => self.value(e, Want::Length, true),
                Some("at") => self.point_entry(e, framed),
                _ => {}
            }
        }
    }

    fn stack_node(&mut self, stack: &KdlNode) {
        for c in stack.iter_children() {
            if c.name().value() == "base" {
                continue;
            }
            self.entry_node(c);
        }
    }

    // ---- declarations ---------------------------------------------------

    fn declare(&mut self, name: &str, span: SourceSpan, value: Option<Expr>, what: &'static str) {
        self.names.plain.insert(name.to_owned());
        self.decls.push(Decl {
            name: name.to_owned(),
            span,
            value,
            what,
        });
    }

    fn rename_for_reserved(&mut self, name: &str) -> String {
        if !crate::words::is_reserved(name) {
            return name.to_owned();
        }
        if let Some(done) = self.renames.get(name) {
            return done.clone();
        }
        let mut chars = name.chars();
        let first = chars.next().map(|c| c.to_ascii_uppercase()).unwrap_or('X');
        let mut new = format!("my{first}{}", chars.as_str());
        while self.names.plain.contains(&new) || self.names.library.contains(&new) {
            new.push('2');
        }
        self.renames.insert(name.to_owned(), new.clone());
        new
    }

    /// A `let` or an `override` of the tile.
    fn const_node(&mut self, n: &KdlNode) {
        let [name_e, value_e] = n.entries() else {
            return;
        };
        let (Some(name), true) = (arg_str(name_e), name_e.name().is_none()) else {
            return;
        };
        let name = name.to_owned();
        if n.name().value() == "override" {
            if self.names.library.contains(&name) {
                self.put_name(name_e, &format!("${name}"));
                self.value(value_e, Want::Length, true);
            } else {
                self.problem(
                    "dt::migrate-by-hand",
                    value_span(name_e),
                    format!("`override {name}` replaces a name of the tile, which a plain name cannot be in format 1"),
                    "a plain name",
                    Some(
                        "an override replaces a `$` name a caller installed: make the `let` a `$` name, or edit its value where it is"
                            .to_owned(),
                    ),
                );
            }
            return;
        }
        let old = self.parse_entry(value_e, true);
        let new_name = self
            .renames
            .get(&name)
            .cloned()
            .unwrap_or_else(|| name.clone());
        if new_name != name {
            self.put_name(name_e, &new_name);
        }
        self.declare(&new_name, value_span(name_e), old, "let");
        // the name it had stands for the one it has now
        let want = self
            .names
            .kinds
            .get(&new_name)
            .map_or(Want::Any, |k| Want::of_kind(*k));
        self.value(value_e, want, true);
    }

    /// A file-level `let`: a library height, a `$` name of the context.
    pub fn library_let(&mut self, n: &KdlNode) {
        let [name_e, value_e] = n.entries() else {
            return;
        };
        let Some(name) = arg_str(name_e) else { return };
        self.put_name(name_e, &format!("${name}"));
        self.value(value_e, Want::Length, true);
    }

    fn point_node(&mut self, n: &KdlNode) {
        if let [_, e] = n.entries() {
            self.point_entry(e, false);
        }
    }

    fn line_node(&mut self, n: &KdlNode) {
        for e in n.entries() {
            if let Some(k) = e.name()
                && matches!(k.value(), "width" | "wobble")
            {
                self.value(e, Want::Length, false);
            }
        }
        for c in n.iter_children() {
            match c.name().value() {
                "through" => {
                    for e in c.entries() {
                        self.point_entry(e, false);
                    }
                }
                "width" | "wobble" => {
                    if let [e] = c.entries() {
                        self.value(e, Want::Length, false);
                    }
                }
                _ => {}
            }
        }
    }

    fn region_node(&mut self, n: &KdlNode) {
        for c in n.iter_children() {
            match c.name().value() {
                "fill" => {
                    for e in c.entries() {
                        self.point_entry(e, false);
                    }
                }
                "within" => {
                    for e in c.entries() {
                        match e.name().map(|k| k.value()) {
                            None => self.value(e, Want::Length, true),
                            Some("of") => self.point_entry(e, false),
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
    }

    // ---- units ----------------------------------------------------------

    fn collect_places(&mut self, body: &KdlDocument, tile: bool) {
        self.scope.places = BUILTIN_POINTS
            .iter()
            .chain(BUILTIN_LINES.iter())
            .map(|s| (*s).to_owned())
            .collect();
        self.scope.places.insert(BUILTIN_REGION.to_owned());
        self.scope.framed.clear();
        for n in body.nodes() {
            let name = n.name().value();
            if !matches!(name, "point" | "line" | "region") {
                continue;
            }
            if let Some(p) = n.entries().first().and_then(arg_str) {
                self.scope.places.insert(p.to_owned());
                if tile && matches!(name, "line" | "region") {
                    self.scope.framed.insert(p.to_owned());
                }
            }
        }
    }

    /// A `tile` or a `define`: its declarations, then its stack.
    pub fn unit(&mut self, node: &KdlNode) {
        let tile = node.name().value() == "tile";
        self.scope = Scope {
            rows: if tile {
                size_of(node).map(|s| s.1)
            } else {
                None
            },
            ..Scope::default()
        };
        self.names.rows = self.scope.rows;
        self.names.plain.clear();
        self.decls.clear();
        self.renames.clear();
        let Some(body) = node.children() else { return };
        self.collect_places(body, tile);
        // a `let` named for a word of the language is renamed before anything
        // reads it
        for n in body.nodes() {
            if n.name().value() == "let"
                && let Some(name) = n.entries().first().and_then(arg_str)
            {
                self.rename_for_reserved(name);
            }
        }
        // the parameters first: names are read before they are declared
        for n in body.nodes() {
            if n.name().value() == "param"
                && let [name_e, ..] = n.entries()
                && let Some(name) = arg_str(name_e)
            {
                let default = n.entries().get(1).and_then(|e| self.parse_entry(e, false));
                if crate::words::is_reserved(name) {
                    self.problem(
                        "dt::migrate-by-hand",
                        value_span(name_e),
                        format!("`{name}` is a word of the language in format 1, and a parameter's name is how callers pass it"),
                        "a reserved word",
                        Some("rename the parameter (and every call that names it) by hand".to_owned()),
                    );
                }
                self.declare(name, value_span(name_e), default, "param");
            }
        }
        for n in body.nodes() {
            match n.name().value() {
                "let" | "override" => self.const_node(n),
                _ => {}
            }
        }
        for n in body.nodes() {
            match n.name().value() {
                "param" => self.param_node(n),
                "point" => self.point_node(n),
                "line" => self.line_node(n),
                "region" => self.region_node(n),
                "stack" => self.stack_node(n),
                _ => {}
            }
        }
    }

    fn param_node(&mut self, n: &KdlNode) {
        let (Some(name_e), Some(default)) = (n.entries().first(), n.entries().get(1)) else {
            return;
        };
        let Some(name) = arg_str(name_e) else { return };
        let Some(kind) = self.names.kinds.get(name).copied() else {
            return;
        };
        match kind {
            Kind::Length => self.value(default, Want::Length, false),
            Kind::Angle => self.value(default, Want::Angle, false),
            _ => {}
        }
    }

    /// The nodes of `defaults`: each an entry with its tops and pins.
    pub fn defaults(&mut self, node: &KdlNode) {
        self.scope = Scope::default();
        self.names.rows = None;
        self.names.plain.clear();
        for c in node.iter_children() {
            self.entry_node(c);
        }
    }

    /// A printer: its numbers in printed units.
    pub fn printer(&mut self, node: &KdlNode) {
        for c in node.iter_children() {
            let unit = match c.name().value() {
                "line" | "layer" | "gap" => "mm",
                "overhang" | "steepest" => "deg",
                _ => continue,
            };
            if let [e] = c.entries()
                && let Some(v) = num_of(e.value())
            {
                self.put_value(e, &format!("{v}{unit}"));
            }
        }
    }

    /// What every plain name measures, from its value and its uses.
    ///
    /// Returns true when a name's kind was settled this round.
    pub fn settle(&mut self) -> bool {
        let mut changed = false;
        let uses = std::mem::take(&mut *self.names.uses.borrow_mut());
        let mut by_name: HashMap<String, Vec<Kind>> = HashMap::new();
        for (n, k) in uses {
            let v = by_name.entry(n).or_default();
            if !v.contains(&k) {
                v.push(k);
            }
        }
        for d in &self.decls {
            if self.names.kinds.contains_key(&d.name) {
                continue;
            }
            let from_value = d.value.as_ref().and_then(|v| kind_of(v, &self.names));
            let used = by_name.get(&d.name);
            let kind = match (from_value, used) {
                (Some(k), _) => Some(k),
                (None, Some(ks)) if ks.len() == 1 => Some(ks[0]),
                (None, Some(ks)) => {
                    self.conflicts.insert(d.name.clone(), ks.clone());
                    None
                }
                (None, None) => None,
            };
            if let Some(k) = kind {
                self.names.kinds.insert(d.name.clone(), k);
                changed = true;
            }
        }
        changed
    }

    /// What is still unsettled: a name used both ways or never.
    pub fn unsettled(&mut self) {
        let decls = std::mem::take(&mut self.decls);
        for d in &decls {
            if self.names.kinds.contains_key(&d.name) {
                continue;
            }
            if let Some(ks) = self.conflicts.get(&d.name) {
                let list: Vec<&str> = ks.iter().map(|k| kind_word(*k)).collect();
                self.problem(
                    "dt::migrate-unit",
                    d.span,
                    format!("`{}` is used as {}", d.name, list.join(" and as ")),
                    "no one unit fits",
                    Some(
                        "split it in two (one a length, one a count) and run migrate again"
                            .to_owned(),
                    ),
                );
            } else {
                self.problem(
                    "dt::migrate-unit",
                    d.span,
                    format!("`{}` is never used where a unit can be told", d.name),
                    "its kind is not known",
                    Some(format!(
                        "use it, or give it its unit by hand: a {} is a length (`4dmm`) or a plain number",
                        d.what
                    )),
                );
            }
        }
        self.decls = decls;
    }

    pub fn reset_round(&mut self) {
        self.edits.clear();
        self.problems.clear();
        self.decls.clear();
        self.names.uses.borrow_mut().clear();
    }
}

fn kind_word(k: Kind) -> &'static str {
    match k {
        Kind::Length => "a length",
        Kind::Angle => "an angle",
        Kind::Plain => "a plain number",
        _ => "a word",
    }
}

/// `(h)` on literals: wall tops to design millimetres, the way the decoder
/// multiplies them.
fn scale_h(e: &Expr) -> Option<Expr> {
    Some(match e {
        Expr::Num {
            value,
            percent: false,
        } => Expr::num(value * schema::H_MM),
        Expr::Neg(a) => Expr::Neg(Box::new(scale_h(a)?)),
        Expr::Range { lo, hi } => Expr::Range {
            lo: Box::new(scale_h(lo)?),
            hi: Box::new(scale_h(hi)?),
        },
        _ => return None,
    })
}
