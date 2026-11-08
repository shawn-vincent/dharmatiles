//! The map of a tile for the editor's overlay: its lines as drawn, its
//! regions as the build fills them (on a coarse lattice), and where each
//! line point and fill point is written, so the page can drag them.

use serde_json::{Value, json};

/// Lattice nodes per square for the regions' areas.
const CELLS: u32 = 16;

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

/// A node's single-valued controls, both as `name=value` and as a
/// `name value` line inside it (not `skip`): each name, its value as
/// written and the value's byte span.
fn controls_of(text: &str, node: &kdl::KdlNode, skip: &[&str]) -> Vec<Value> {
    let value = |e: &kdl::KdlEntry| {
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
    };
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
    let Some(c) = node
        .children()
        .and_then(|d| d.nodes().iter().find(|n| n.name().value() == child))
    else {
        return Value::Null;
    };
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
pub(super) fn json(c: &dt_eval::Checked, text: &str) -> Value {
    let Some(t) = &c.tile else {
        return Value::Null;
    };
    let doc = kdl::KdlDocument::parse(text).ok();
    let node = doc
        .as_ref()
        .and_then(|d| d.nodes().iter().find(|n| n.name().value() == "tile"));
    let masks = c.region_masks(CELLS);
    let lines: Vec<Value> = t
        .lines
        .iter()
        .map(|(name, pts)| {
            let w = node.and_then(|n| written(n, "line", name, "through"));
            let control = t.wobbles.get(name).map(|(p, _)| p).or(Some(pts));
            json!({
                "name": name,
                "pts": pts,
                "wobble": t.wobbles.get(name).map(|(_, w)| w),
                "span": w.as_ref().map(|w| w.0),
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
        .map(|(r, m)| {
            let w = node.and_then(|n| written(n, "region", &r.name, "fill"));
            let bits: String = m.bits.iter().map(|&b| if b { '1' } else { '0' }).collect();
            json!({
                "name": r.name,
                "bounded_by": r.bounded_by.iter().map(|b| b.value.clone()).collect::<Vec<_>>(),
                "nx": m.nx, "ny": m.ny, "bits": bits,
                "span": w.as_ref().map(|w| w.0),
                "fills": w.map(|(_, w)| points(&w, Some(&r.fills))).unwrap_or_default(),
                "sheet": node.and_then(|n| mark_node(n, "region", &r.name)).map(|g| json!({
                    "at": g.name().span().offset(),
                    "bounded_by": args_of(text, g, "bounded-by"),
                    "fill": args_of(text, g, "fill"),
                })),
            })
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
    json!({ "cols": t.cols, "rows": t.rows, "cells": CELLS, "lines": lines, "regions": regions, "tile": own })
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
