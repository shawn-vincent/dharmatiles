//! The map of a tile for the editor's overlay: its lines as drawn, its
//! regions as the build fills them (on the draft build's lattice), and where each
//! line point and fill point is written, so the page can drag them.

use serde_json::{Value, json};

/// Lattice nodes per square for the regions' areas: the draft build's
/// lattice, so a region's edge on the map steps under a millimetre (0.7 mm
/// on DungeonBlocks, 0.5 on OpenLOCK) and meets its lines.
const CELLS: u32 = crate::DRAFT_CELLS;

/// How the map reads a written point: as the document's own `"(0.5sq, 25%)"`
/// forms, which only the program's reader ([`dt_doc::decode::point_text`])
/// knows how to read.
#[derive(Clone, Copy)]
struct Reading {
    units: dt_doc::units::Units,
}

impl Reading {
    /// The point a text writes, if it writes one.
    fn point(self, t: &crate::Tile, text: &str) -> Option<dt_doc::place::Point> {
        dt_doc::decode::point_text(text.trim(), (t.cols, t.rows), self.units).ok()
    }
}

/// A written value: its text and its byte span.
type Written = (String, [usize; 2]);

/// The written entries of each `name` child of the node `parent NAME`, in
/// order: (their text, their byte span), with the node's own span.
fn written(
    tile: &kdl::KdlNode,
    parent: &str,
    name: &str,
    child: &str,
) -> Option<([usize; 2], Vec<Written>)> {
    let node = tile.children()?.nodes().iter().find(|n| {
        n.name().value() == parent
            && n.entries()
                .first()
                .is_some_and(|e| e.name().is_none() && e.value().as_string() == Some(name))
    })?;
    let mut out = Vec::new();
    for c in node.children().map(|c| c.nodes()).unwrap_or_default() {
        if c.name().value() != child {
            continue;
        }
        for e in c.entries().iter().filter(|e| e.name().is_none()) {
            let v = e.value();
            let text = v.as_string().map_or_else(|| v.to_string(), str::to_owned);
            out.push((text, [e.span().offset(), e.span().len()]));
        }
    }
    Some(([node.span().offset(), node.span().len()], out))
}

/// An entry's value as written (not its `name=`), with the value's byte span.
fn entry_value(text: &str, e: &kdl::KdlEntry) -> (String, [usize; 2]) {
    let sp = e.span();
    let whole = &text[sp.offset()..sp.offset() + sp.len()];
    let from = if e.name().is_some() {
        whole.find('=').map_or(0, |i| i + 1)
    } else {
        0
    };
    let rest = &whole[from..];
    let lead = rest.len() - rest.trim_start().len();
    (
        rest.trim().to_owned(),
        [sp.offset() + from + lead, rest.trim().len()],
    )
}

/// A node's single-valued controls, both as `name=value` and as a
/// `name value` line inside it (not `skip`): each name, its value as
/// written and the value's byte span.
fn controls_of(text: &str, node: &kdl::KdlNode, skip: &[&str]) -> Vec<Value> {
    let value = |e: &kdl::KdlEntry| entry_value(text, e);
    let mut out = Vec::new();
    for e in node.entries().iter().filter(|e| e.name().is_some()) {
        let (t, span) = value(e);
        out.push(json!({ "name": e.name().map(|k| k.value()), "text": t, "span": span }));
    }
    for c in node
        .children()
        .map(kdl::KdlDocument::nodes)
        .unwrap_or_default()
    {
        let name = c.name().value();
        if skip.contains(&name) || c.children().is_some() {
            continue;
        }
        if let [e] = c.entries()
            && e.name().is_none()
        {
            let (t, span) = value(e);
            out.push(json!({ "name": name, "text": t, "span": span }));
        }
    }
    out
}

/// The arguments of `node`'s first `child` line, as one run of text: the
/// text and its span (from the first argument to the end of the last).
fn args_of(text: &str, node: &kdl::KdlNode, child: &str) -> Value {
    node.children()
        .and_then(|d| d.nodes().iter().find(|n| n.name().value() == child))
        .map_or(Value::Null, |c| run_of(text, c))
}

/// The positional arguments of `c`, as one run of text with its span.
fn run_of(text: &str, c: &kdl::KdlNode) -> Value {
    let args: Vec<_> = c.entries().iter().filter(|e| e.name().is_none()).collect();
    let (Some(a), Some(b)) = (args.first(), args.last()) else {
        return json!({ "at": c.name().span().offset(), "text": "", "span": null });
    };
    let lead = |e: &kdl::KdlEntry| {
        let sp = e.span();
        let w = &text[sp.offset()..sp.offset() + sp.len()];
        (
            sp.offset() + w.len() - w.trim_start().len(),
            sp.offset() + w.trim_end().len(),
        )
    };
    let (start, _) = lead(a);
    let (_, end) = lead(b);
    json!({ "at": c.name().span().offset(), "text": &text[start..end], "span": [start, end - start] })
}

/// A written value with its quotes taken off.
fn unquoted(t: &str) -> String {
    t.trim_matches('"').to_owned()
}

/// What a region's body writes, clause by clause in the order written: its
/// `within` clauses (the distance and the place, each with its span) and
/// its `except` clauses (the regions, as one run).  Each `within` carries
/// what its distance prints on this target (`printed`, from
/// [`dt_doc::printed::printed_size`], the one place that works it out; null
/// where it prints as written): the page writes no unit word of its own.
fn clauses(
    text: &str,
    node: &kdl::KdlNode,
    units: dt_doc::units::Units,
    consts: &dt_doc::consts::ConstTable,
) -> Vec<Value> {
    let mut out = Vec::new();
    for c in node
        .children()
        .map(kdl::KdlDocument::nodes)
        .unwrap_or_default()
    {
        let at = c.name().span().offset();
        match c.name().value() {
            "within" => {
                let dist = c
                    .entries()
                    .iter()
                    .find(|e| e.name().is_none())
                    .map(|e| entry_value(text, e));
                let of = c
                    .entries()
                    .iter()
                    .find(|e| e.name().is_some_and(|n| n.value() == "of"))
                    .map(|e| entry_value(text, e));
                let printed = dist.as_ref().and_then(|(t, _)| {
                    dt_doc::printed::printed_size(
                        dt_doc::schema::Axis::Dmm,
                        &unquoted(t),
                        units,
                        Some(consts),
                    )
                });
                out.push(json!({
                    "kind": "within",
                    "at": at,
                    "distance": dist.map(|(t, s)| json!({ "text": t, "span": s })),
                    "of": of.map(|(t, s)| json!({ "text": unquoted(&t), "written": t, "span": s })),
                    "printed": printed.map(|p| p.ends),
                }));
            }
            "except" => out.push(json!({ "kind": "except", "at": at, "regions": run_of(text, c) })),
            _ => {}
        }
    }
    out
}

/// Where a place a region is made from lies: its name, what kind of place
/// it is, and its geometry in squares (a line's points, a point; a region
/// is found by name among the map's regions).
fn place_ref(t: &crate::Tile, name: &str, rd: Reading) -> Value {
    use dt_doc::place::{
        BUILTIN_LINES, BUILTIN_POINTS, BUILTIN_REGION, builtin_line, builtin_point,
    };
    let (cols, rows) = (f64::from(t.cols), f64::from(t.rows));
    if (name.starts_with('(') || name.starts_with("point(") || name.starts_with("polar("))
        && let Some(p) = rd.point(t, name)
        && let Ok(((x0, x1), (y0, y1))) = p.bounds(cols, rows)
    {
        let at = [f64::midpoint(x0, x1), f64::midpoint(y0, y1)];
        return json!({ "name": name, "kind": "point", "at": at });
    }
    if name == BUILTIN_REGION {
        let ring = builtin_line("edges", cols, rows);
        return json!({ "name": name, "kind": "tile", "pts": ring });
    }
    if let Some(xy) = t.heights.points.get(name) {
        return json!({ "name": name, "kind": "point", "at": xy });
    }
    if BUILTIN_POINTS.contains(&name)
        && let Some((x, y)) = builtin_point(name, cols, rows)
    {
        return json!({ "name": name, "kind": "point", "at": [x, y] });
    }
    if BUILTIN_LINES.contains(&name)
        && let Some(pts) = builtin_line(name, cols, rows)
    {
        return json!({ "name": name, "kind": "edge", "pts": pts });
    }
    if let Some(pts) = t.lines.get(name) {
        return json!({ "name": name, "kind": "line", "pts": pts });
    }
    if t.regions.iter().any(|r| r.name == name) {
        return json!({ "name": name, "kind": "region" });
    }
    json!({ "name": name, "kind": "unknown" })
}

/// Where a written point lies, in squares, and whether it is a `literal`
/// pair or a `name` (a corner or edge word, a declared or built-in point).
/// A coordinate drawn from a range or written as an expression is placed
/// where its range middles; a name nothing declares has no place.  The
/// reading is the program's own ([`dt_doc::place::parse_point`]), not the page's.
fn locate(t: &crate::Tile, text: &str, rd: Reading) -> (&'static str, Option<[f64; 2]>) {
    use dt_doc::place::{BUILTIN_POINTS, Point, builtin_point};
    let (cols, rows) = (f64::from(t.cols), f64::from(t.rows));
    if let Some(p) = rd.point(t, text) {
        let kind = if matches!(p, Point::Pair { .. }) {
            "literal"
        } else {
            "name"
        };
        let at = p
            .bounds(cols, rows)
            .ok()
            .map(|((x0, x1), (y0, y1))| [f64::midpoint(x0, x1), f64::midpoint(y0, y1)]);
        return (kind, at);
    }
    let name = text.trim();
    let at = t.heights.points.get(name).copied().or_else(|| {
        BUILTIN_POINTS
            .contains(&name)
            .then(|| builtin_point(name, cols, rows))
            .flatten()
            .map(|(x, y)| [x, y])
    });
    ("name", at)
}

/// A control point a line or region refers to: its label (the source as
/// written: the coordinates, or the point's name), where it is, and the
/// written value's byte span.  `ranged` is true when it lies somewhere in a
/// range, so `at` is only the middle of where it may land.
fn marker(t: &crate::Tile, text: &str, span: Option<[usize; 2]>, rd: Reading) -> Value {
    let (kind, at) = locate(t, text, rd);
    let ranged = rd.point(t, text).is_some_and(|p| p.is_range());
    json!({ "label": text.trim(), "kind": kind, "at": at, "span": span, "ranged": ranged })
}

/// The `of` points of a region's `within` clauses: the ones written as a
/// literal pair or as the name of a point (a line or region is drawn as
/// itself, not as a point).
fn within_markers(t: &crate::Tile, node: &kdl::KdlNode, text: &str, rd: Reading) -> Vec<Value> {
    use dt_doc::place::{BUILTIN_POINTS, Point};
    node.children()
        .map(kdl::KdlDocument::nodes)
        .unwrap_or_default()
        .iter()
        .filter(|c| c.name().value() == "within")
        .filter_map(|c| {
            let e = c
                .entries()
                .iter()
                .find(|e| e.name().is_some_and(|n| n.value() == "of"))?;
            let (w, span) = entry_value(text, e);
            let w = unquoted(&w);
            let name = w.trim();
            let is_point = t.heights.points.contains_key(name)
                || BUILTIN_POINTS.contains(&name)
                || matches!(rd.point(t, &w), Some(Point::Pair { .. }));
            is_point.then(|| marker(t, &w, Some(span), rd))
        })
        .collect()
}

/// Every control point a region refers to: each `fill` point, then each
/// `within … of=` centre that is a point.
fn markers_of(
    t: &crate::Tile,
    tile: Option<&kdl::KdlNode>,
    r: &crate::Region,
    fills: Option<&[Written]>,
    text: &str,
    rd: Reading,
) -> Vec<Value> {
    let mut out: Vec<Value> = fills
        .unwrap_or_default()
        .iter()
        .map(|(x, sp)| marker(t, x, Some(*sp), rd))
        .collect();
    if let Some(node) = tile.and_then(|n| mark_node(n, "region", &r.name)) {
        out.extend(within_markers(t, node, text, rd));
    }
    out
}

/// The places a region is made from, each once: the lines that bound it,
/// the places its `within`s are round, the regions it excepts.
fn region_refs(t: &crate::Tile, r: &crate::Region, rd: Reading) -> Vec<Value> {
    let mut names: Vec<String> = r.bounded_by.iter().map(|b| b.value.clone()).collect();
    names.extend(r.within.iter().map(|w| w.of.to_string()));
    names.extend(r.except.iter().cloned());
    let mut seen = std::collections::BTreeSet::new();
    names.retain(|n| *n != r.name && seen.insert(n.clone()));
    names.iter().map(|n| place_ref(t, n, rd)).collect()
}

/// Every place a tile has, for a choice among them: the built-in edges and
/// points, the tile's own points, lines and regions.
fn places(t: &crate::Tile) -> Vec<Value> {
    use dt_doc::place::{BUILTIN_LINES, BUILTIN_POINTS, BUILTIN_REGION};
    let mut out = Vec::new();
    let mut add = |name: &str, kind: &str| out.push(json!({ "name": name, "kind": kind }));
    for n in BUILTIN_LINES {
        add(n, "edge");
    }
    for l in t.lines.keys() {
        add(l, "line");
    }
    for r in &t.regions {
        add(&r.name, "region");
    }
    add(BUILTIN_REGION, "tile");
    for n in BUILTIN_POINTS {
        add(n, "point");
    }
    for p in t.heights.points.keys() {
        if !BUILTIN_POINTS.contains(&p.as_str()) {
            add(p, "point");
        }
    }
    out
}

/// The `line` or `region` node of the tile called `name`.
fn mark_node<'a>(tile: &'a kdl::KdlNode, kind: &str, name: &str) -> Option<&'a kdl::KdlNode> {
    tile.children()?.nodes().iter().find(|n| {
        n.name().value() == kind
            && n.entries()
                .first()
                .is_some_and(|e| e.name().is_none() && e.value().as_string() == Some(name))
    })
}

fn points(w: &[(String, [usize; 2])], at: Option<&Vec<[f64; 2]>>) -> Vec<Value> {
    w.iter()
        .enumerate()
        .map(|(k, (text, span))| {
            let at = at.filter(|a| a.len() == w.len()).map(|a| a[k]);
            json!({ "text": text, "span": span, "at": at })
        })
        .collect()
}

/// The map of checked tile `c`, its spans from `text` (the tile's own
/// document as checked).
pub fn json(
    c: &crate::Checked,
    text: &str,
    units: dt_doc::units::Units,
) -> Result<Value, Vec<crate::Diag>> {
    let Some(t) = &c.tile else {
        return Ok(Value::Null);
    };
    let rd = Reading { units };
    let doc = kdl::KdlDocument::parse(text).ok();
    let node = doc
        .as_ref()
        .and_then(|d| d.nodes().iter().find(|n| n.name().value() == "tile"));
    let masks = c.region_masks(CELLS)?;
    let lines: Vec<Value> = t
        .lines
        .iter()
        .map(|(name, pts)| {
            let w = node.and_then(|n| written(n, "line", name, "through"));
            let control = t.controls.get(name).or(Some(pts));
            json!({
                "name": name,
                "pts": pts,
                // how far the line may wander, printed mm on this target (the
                // page draws it as it is and does no unit arithmetic)
                "wobbleMm": t.wobbles.get(name).map(|(_, w)| w * units.square_mm / dt_doc::schema::SQUARE_MM),
                "span": w.as_ref().map(|w| w.0),
                "markers": w.as_ref().map(|(_, w)| w.iter().map(|(x, sp)| marker(t, x, Some(*sp), rd)).collect::<Vec<_>>()).unwrap_or_default(),
                "points": w.map(|(_, w)| points(&w, control)).unwrap_or_default(),
                "sheet": node.and_then(|n| mark_node(n, "line", name)).map(|l| json!({
                    "at": l.name().span().offset(),
                    "controls": controls_of(text, l, &["through"]),
                    "through": args_of(text, l, "through"),
                })),
            })
        })
        .collect();
    let regions: Vec<Value> = t
        .regions
        .iter()
        .zip(&masks)
        // a line's width makes a strip no document names: the map has the line
        .filter(|(r, _)| !r.strip)
        .map(|(r, m)| {
            let w = node.and_then(|n| written(n, "region", &r.name, "fill"));
            let bits: String = m.bits.iter().map(|&b| if b { '1' } else { '0' }).collect();
            json!({
                "name": r.name,
                "bounded_by": r.bounded_by.iter().map(|b| b.value.clone()).collect::<Vec<_>>(),
                "nx": m.nx, "ny": m.ny, "bits": bits,
                "span": w.as_ref().map(|w| w.0),
                "markers": markers_of(t, node, r, w.as_ref().map(|(_, w)| w.as_slice()), text, rd),
                "fills": w.map(|(_, w)| points(&w, Some(&r.fills))).unwrap_or_default(),
                "sheet": node.and_then(|n| mark_node(n, "region", &r.name)).map(|g| json!({
                    "at": g.name().span().offset(),
                    "bounded_by": args_of(text, g, "boundedBy"),
                    "fill": args_of(text, g, "fill"),
                    "clauses": clauses(text, g, units, &t.heights.consts),
                })),
                "refs": region_refs(t, r, rd),
            })
        })
        .collect();
    // the tile's own `point name "(x, y)"` declarations: where each is and where it is written
    let named: Vec<Value> = node
        .and_then(|n| n.children())
        .map(|c| c.nodes())
        .unwrap_or_default()
        .iter()
        .filter(|n| n.name().value() == "point")
        .filter_map(|n| {
            let name = n.entries().first()?.value().as_string()?;
            let at = t.heights.points.get(name)?;
            Some(json!({ "name": name, "at": at, "span": [n.span().offset(), n.span().len()] }))
        })
        .collect();
    // the tile node itself, for the property sheet when nothing is selected
    let own = node.map(|n| {
        let at = n.name().span().offset();
        let controls: Vec<Value> = dt_doc::edit::props_at(text, at)
            .unwrap_or_default()
            .into_iter()
            .map(|(name, text, span)| json!({ "name": name, "text": text, "span": span }))
            .collect();
        let child = |name: &str| {
            n.children()
                .and_then(|c| c.nodes().iter().find(|k| k.name().value() == name))
        };
        // `about "…"`: its string, and where the string is written
        let about = child("about").map(|a| {
            let value = a.entries().first().filter(|e| e.name().is_none()).map(|e| {
                let sp = e.span();
                let whole = &text[sp.offset()..sp.offset() + sp.len()];
                let lead = whole.len() - whole.trim_start().len();
                let v = whole.trim();
                json!({ "text": v, "span": [sp.offset() + lead, v.len()] })
            });
            json!({ "at": a.name().span().offset(), "value": value })
        });
        json!({ "at": at, "controls": controls, "about": about, "heights": heights(&t.heights.consts) })
    });
    Ok(
        json!({ "language": language(t), "cols": t.cols, "rows": t.rows, "cells": CELLS, "lines": lines, "regions": regions, "points": named, "places": places(t), "tile": own }),
    )
}

/// The words a value is written from, for completion: the `$` names (what
/// an element sets, the regions, the tile's heights), the units and the
/// functions.
fn language(t: &crate::Tile) -> Value {
    let mut names: Vec<String> = dt_lang::POINT_NAMES
        .iter()
        .chain(dt_lang::REGION_NAMES)
        .map(|n| (*n).to_owned())
        .collect();
    names.extend(
        t.heights
            .consts
            .entries()
            .iter()
            .filter(|e| e.name.starts_with('$'))
            .map(|e| e.name.clone()),
    );
    json!({ "names": names, "units": dt_lang::UNITS, "functions": dt_lang::function_words() })
}

/// The heights a value may name in this tile: its constants (the library's
/// first, then its own `let`s), each with its value in mm at the reference
/// square where it is one number, and where it comes from.
fn heights(table: &dt_doc::consts::ConstTable) -> Vec<Value> {
    use dt_doc::consts::Origin;
    table
        .entries()
        .iter()
        .map(|e| {
            let value = table.scalar(&e.expr, 0.5, &|_| None).ok();
            let from = match e.origin {
                Origin::Library => "library",
                Origin::Tile => "tile",
                Origin::Override => "override",
            };
            json!({ "name": e.name, "value": value, "text": e.text, "from": from })
        })
        .collect()
}
