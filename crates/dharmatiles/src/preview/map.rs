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
            })
        })
        .collect();
    json!({ "cols": t.cols, "rows": t.rows, "cells": CELLS, "lines": lines, "regions": regions })
}
