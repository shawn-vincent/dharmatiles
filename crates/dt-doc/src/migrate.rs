//! The migration to the heights language, written as code so a tile of the
//! old forms (`ground`, `level=`, `bumps=`, a region painted at another
//! level) is carried over by one call instead of by hand.
//!
//! What each old form becomes:
//!
//! - `dirt level=L bumps=B` is `dirt top="[L .. L + B]"` (`top=L` when it
//!   has no bumps; no `top` at all when it has neither: the library's top);
//! - `ground in=R level="water - 2.2"` (the bed under water) is a pin on
//!   the land's dirt, `pin top=waterBedHeight at=R`, written last so it
//!   wins, with a wide pin `top="[waterBedHeight .. HIGH]"` before it over
//!   `shoreBand`, the strip a bank's width (7 mm) round every line, which
//!   is the space the bank rule used to draw; the dirt's `in=` goes, since
//!   the dirt now covers the whole tile;
//! - `ground` with no `in=` under water becomes `dirt top=waterBedHeight`
//!   pinned at the edges; any other `ground` is a quiet `dirt` (`clods=none
//!   stones=none`: no texture at all);
//! - the stack gains `base` as its first line.
//!
//! - `floor` is `flagstones` (the slabs; planks are new): its `level=L`
//!   (or `level L` as a child) goes, and a level other than the library's
//!   `floorHeight` becomes `top=L` with `pin top=L at=edges`, so the floor
//!   still meets the tile's edge at its own height.
//!
//! Level names become the library's heights: `edge` is `dirtEdgeHeight`,
//! `ground` is `groundHeight`, `water` is `waterSurfaceHeight`.

use kdl::{KdlDocument, KdlNode, KdlValue};

use crate::edit::EditError;

/// How wide the strip round a line is, mm at the reference square: the
/// width the old bank rule gave a bank.
const SHORE_MM: f64 = 7.0;

fn heights_word(w: &str) -> String {
    match w {
        "edge" => "dirtEdgeHeight".to_owned(),
        "ground" => "groundHeight".to_owned(),
        "water" => "waterSurfaceHeight".to_owned(),
        "foot" => "footHeight".to_owned(),
        "floor" => "floorHeight".to_owned(),
        other => other.to_owned(),
    }
}

/// A level expression (`edge + 0.3`) as a height expression.
fn height(expr: &str) -> String {
    let mut out = String::new();
    let mut word = String::new();
    for c in expr.chars().chain(std::iter::once(' ')) {
        if c.is_alphanumeric() || c == '_' || c == '.' && !word.is_empty() {
            word.push(c);
        } else {
            if !word.is_empty() {
                let w = if word.chars().next().is_some_and(char::is_alphabetic) {
                    heights_word(&word)
                } else {
                    word.clone()
                };
                out.push_str(&w);
                word.clear();
            }
            out.push(c);
        }
    }
    out.trim_end().to_owned()
}

fn num_text(v: &KdlValue) -> Option<String> {
    match v {
        KdlValue::String(s) => Some(s.clone()),
        KdlValue::Integer(i) => Some(i.to_string()),
        KdlValue::Float(f) => Some(format!("{f}")),
        _ => None,
    }
}

fn prop<'a>(n: &'a KdlNode, name: &str) -> Option<&'a KdlValue> {
    n.entry(name).map(kdl::KdlEntry::value)
}

fn line_start(text: &str, at: usize) -> usize {
    text[..at].rfind('\n').map_or(0, |n| n + 1)
}

fn line_end(text: &str, at: usize) -> usize {
    text[at..].find('\n').map_or(text.len(), |n| at + n + 1)
}

fn indent(text: &str, at: usize) -> String {
    let ls = line_start(text, at);
    text[ls..at]
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect()
}

/// The high end of a dirt's top, as an expression, and the top itself.
fn top_of(n: &KdlNode) -> (Option<String>, String) {
    let level = prop(n, "level").and_then(num_text).map(|l| height(&l));
    let bumps = prop(n, "bumps").and_then(num_text);
    let hi_bump = bumps.map(|b| b.rsplit("..").next().unwrap_or("0").to_owned());
    match (level, hi_bump) {
        (None, None) => (None, "groundHeight + 1".to_owned()),
        (Some(l), None) => (Some(l.clone()), l),
        (l, Some(b)) => {
            let l = l.unwrap_or_else(|| "groundHeight".to_owned());
            let hi = format!("{l} + {b}");
            (Some(format!("[{l} .. {hi}]")), hi)
        }
    }
}

struct Edit {
    start: usize,
    end: usize,
    with: String,
}

/// Carry `text`, a tile of the old forms, to the heights language.
/// `Ok(None)` when it has none of them and already writes its `base`.
///
/// # Errors
/// If the text is not KDL, or a stack is written on one line.
pub fn heights(text: &str) -> Result<Option<String>, EditError> {
    let floors = floors(text)?;
    let now = floors.as_deref().unwrap_or(text);
    let cliffs = cliffs(now)?;
    let now = cliffs.as_deref().unwrap_or(now);
    Ok(heights_only(now)?.or(cliffs).or(floors))
}

/// The rings of a free-standing pile, each a disc round a point: its radius
/// and the share of the height it stands at (the middle of the form's own
/// range), at the 70 mm reference the form's masses were drawn for.
type Rings = &'static [(f64, f64)];

const PLATEAU: Rings = &[(26.0, 1.0)];
const MASSIF: Rings = &[(26.5, 0.5), (15.0, 1.0)];
const MOUND: Rings = &[
    (27.0, 0.17),
    (20.5, 0.385),
    (14.5, 0.59),
    (10.0, 0.8),
    (8.0, 1.0),
];
const SPIRE: Rings = &[(26.0, 0.11), (18.5, 0.325), (13.0, 0.59), (9.5, 1.0)];
const NEEDLE: Rings = &[(24.0, 0.14), (16.0, 0.42), (10.0, 1.0)];

/// How a form's cliff is written now: its regions, its top and its pins.
struct Recipe {
    /// Region lines, written before the stack.
    regions: Vec<String>,
    /// The cliff's top.
    top: String,
    /// The cliff's pins.
    pins: Vec<String>,
    /// The region the cliff stands in, and the one a film of dirt lies in.
    crag: String,
    film: String,
}

fn round1(v: f64) -> String {
    let r = (v * 2.0).round() / 2.0;
    if (r - r.round()).abs() < 1e-9 {
        format!("{}", r.round())
    } else {
        format!("{r}")
    }
}

/// `rings` as nested discs round `centre` (a place), `scale` the form's
/// size against its reference, `height` the cliff's: the outer ring is the
/// cliff's own band, the inner ones pins, each a step of three millimetres.
fn rings(
    name: &str,
    centre: &str,
    rings: Rings,
    scale: f64,
    height: f64,
    out: &mut Recipe,
    first: bool,
) {
    for (k, &(r, h)) in rings.iter().enumerate() {
        let region = if k == 0 && first {
            name.to_owned()
        } else {
            format!("{name}{}", k + 1)
        };
        out.regions.push(format!(
            "region {region} {{ within {} of={centre} }}",
            round1(r * scale)
        ));
        let hi = height * h;
        let band = format!(
            "dirtEdgeHeight + [{} .. {}]",
            round1((hi - 3.0).max(1.0)),
            round1(hi)
        );
        if k == 0 && first {
            out.top = format!("\"{band}\"");
            out.crag = region.clone();
        } else {
            out.pins.push(format!("pin top=\"{band}\" at={region}"));
        }
        out.film = region;
    }
}

/// The recipe for one old cliff form.
#[allow(clippy::too_many_lines)]
fn recipe(
    n: usize,
    form: &str,
    along: &[&str],
    height: Option<f64>,
    (cols, rows): (f64, f64),
) -> Recipe {
    let suffix = if n == 0 {
        String::new()
    } else {
        (n + 1).to_string()
    };
    let name = format!("crag{suffix}");
    let h = height.unwrap_or(match form {
        "plateau" => 14.0,
        "massif" | "run" | "corner" | "canyon" => 35.0,
        "mound" => 34.0,
        "spire" => 58.0,
        "needles" => 55.0,
        _ => 35.0,
    });
    let mut out = Recipe {
        regions: Vec::new(),
        top: format!("\"dirtEdgeHeight + {}\"", round1(h)),
        pins: Vec::new(),
        crag: name.clone(),
        film: name.clone(),
    };
    let scale = (cols.min(rows) * 35.0) / 70.0;
    let edge = |w: &str| format!("{w}Edge");
    match form {
        "plateau" | "massif" | "mound" | "spire" => {
            let rs = match form {
                "plateau" => PLATEAU,
                "massif" => MASSIF,
                "mound" => MOUND,
                _ => SPIRE,
            };
            rings(&name, "center", rs, scale, h, &mut out, true);
        }
        "needles" => {
            // three needles on a shared footing, as the form drew them
            let (cx, cy) = (cols / 2.0, rows / 2.0);
            let step = 0.3_f64.min(cols.min(rows) * 0.3);
            let needles = [
                (cx, cy, 1.0),
                (cx - step, cy + step * 0.35, 0.8),
                (cx + step * 0.7, cy - step * 0.3, 0.75),
            ];
            let foot: Vec<String> = needles
                .iter()
                .map(|&(x, y, z)| {
                    format!(
                        "within {} of=\"({}, {})\"",
                        round1(NEEDLE[0].0 * 1.3 * scale * z),
                        round1_sq(x),
                        round1_sq(y)
                    )
                })
                .collect();
            out.regions
                .push(format!("region {name} {{ {} }}", foot.join("; ")));
            out.top = format!("\"dirtEdgeHeight + [3 .. {}]\"", round1(h * 0.18));
            let mut film = name.clone();
            // the rings of every needle outer first, so no needle's foot
            // is drawn over another's summit
            for (k, &(r, hf)) in NEEDLE.iter().enumerate() {
                for (i, &(x, y, z)) in needles.iter().enumerate() {
                    let c = format!("\"({}, {})\"", round1_sq(x), round1_sq(y));
                    let region = format!("{name}n{}r{}", i + 1, k + 1);
                    out.regions.push(format!(
                        "region {region} {{ within {} of={c} }}",
                        round1(r * scale * z)
                    ));
                    let hi = h * z * hf;
                    out.pins.push(format!(
                        "pin top=\"dirtEdgeHeight + [{} .. {}]\" at={region}",
                        round1((hi - 3.0).max(1.0)),
                        round1(hi)
                    ));
                    if i == 0 && k == NEEDLE.len() - 1 {
                        film = region;
                    }
                }
            }
            out.film = film;
        }
        _ => {
            // a band against edges: how deep, where, how high
            let (walls, depth): (Vec<&str>, &str) = match form {
                "canyon" => (vec!["west", "east"], "[8 .. 14]"),
                _ => (
                    if along.is_empty() {
                        vec!["north"]
                    } else {
                        along.to_vec()
                    },
                    "[10 .. 20]",
                ),
            };
            let withins: Vec<String> = walls
                .iter()
                .map(|w| format!("within \"{depth}\" of={}", edge(w)))
                .collect();
            out.regions
                .push(format!("region {name} {{ {} }}", withins.join("; ")));
            if form == "escarpment" {
                // a band whose height ramps along the edge, 10 to 40 mm
                // (x and y are squares: a height from them is a sine)
                let (v, n) = if cols >= rows {
                    ("x", cols)
                } else {
                    ("y", rows)
                };
                out.top = format!(
                    "\"dirtEdgeHeight + 10 + 30 * sin(90 * {v} / {}) + [-4 .. 0]\"",
                    round1(n)
                );
            }
        }
    }
    out
}

fn round1_sq(v: f64) -> String {
    let r = (v * 100.0).round() / 100.0;
    format!("{r}")
}

/// A number's text in a property: `35` or `"1.5..3.5"`.
fn range_of(v: Option<&KdlValue>, default: (f64, f64)) -> (f64, f64) {
    let Some(t) = v.and_then(num_text) else {
        return default;
    };
    let mut it = t.split("..").map(|p| p.trim().parse::<f64>().ok());
    match (it.next().flatten(), it.next()) {
        (Some(a), None) => (a, a),
        (Some(a), Some(Some(b))) => (a, b),
        _ => default,
    }
}

/// A cliff of the old forms (`form=`, `along=`, `height=`, `top dirt`, a
/// dirt's `bank against=cliff`) written as a region, a band, pins and a film
/// of dirt. `Ok(None)` when the tile has none of them.
#[allow(clippy::too_many_lines)]
fn cliffs(text: &str) -> Result<Option<String>, EditError> {
    let doc = KdlDocument::parse(text).map_err(|e| EditError::Parse(e.to_string()))?;
    let Some(tile) = doc.nodes().iter().find(|n| n.name().value() == "tile") else {
        return Ok(None);
    };
    let Some(body) = tile.children() else {
        return Ok(None);
    };
    let Some(stack) = body.nodes().iter().find(|n| n.name().value() == "stack") else {
        return Ok(None);
    };
    let Some(kids) = stack.children() else {
        return Ok(None);
    };
    let size = prop(tile, "size")
        .and_then(|v| v.as_string())
        .and_then(|s| {
            let (a, b) = s.split_once('x')?;
            Some((a.parse::<f64>().ok()?, b.parse::<f64>().ok()?))
        })
        .unwrap_or((1.0, 1.0));
    let old = |n: &KdlNode| {
        n.name().value() == "cliff"
            && (n.entry("form").is_some()
                || n.entry("along").is_some()
                || n.entry("height").is_some()
                || n.children().is_some_and(|c| {
                    c.nodes().iter().any(|k| {
                        [
                            "top",
                            "bank-height",
                            "bank-reach",
                            "scree",
                            "boulders",
                            "grit",
                        ]
                        .contains(&k.name().value())
                    })
                }))
    };
    let cliffs: Vec<&KdlNode> = kids.nodes().iter().filter(|n| old(n)).collect();
    if cliffs.is_empty() {
        return Ok(None);
    }
    let mut edits: Vec<Edit> = Vec::new();
    let mut region_lines: Vec<String> = Vec::new();
    let mut apron: Option<(String, f64, (f64, f64))> = None;
    let sind = indent(text, stack.span().offset());
    for (n, c) in cliffs.iter().enumerate() {
        let (off, len) = (c.span().offset(), c.span().len());
        let ind = indent(text, off);
        let form = prop(c, "form")
            .and_then(|v| v.as_string())
            .unwrap_or("run")
            .to_owned();
        let along: Vec<String> = prop(c, "along")
            .and_then(|v| v.as_string())
            .map(|s| s.split_whitespace().map(str::to_owned).collect())
            .unwrap_or_default();
        let along_refs: Vec<&str> = along.iter().map(String::as_str).collect();
        let height = prop(c, "height")
            .and_then(num_text)
            .and_then(|t| t.parse().ok());
        let form = if form == "run" && along.len() > 1 {
            "corner".to_owned()
        } else {
            form
        };
        let r = recipe(n, &form, &along_refs, height, size);
        region_lines.extend(r.regions.iter().cloned());
        // the children that stay: the stone's own, not the old form's
        let mut kept: Vec<String> = Vec::new();
        let mut film = false;
        if let Some(b) = c.children() {
            for k in b.nodes() {
                let name = k.name().value();
                if name == "top" {
                    film = k
                        .entries()
                        .first()
                        .and_then(|e| e.value().as_string())
                        .is_some_and(|w| w == "dirt");
                    continue;
                }
                if ["bank-height", "bank-reach", "scree", "boulders", "grit"].contains(&name) {
                    continue;
                }
                let (ko, kl) = (k.span().offset(), k.span().len());
                kept.push(text[ko..ko + kl].trim().to_owned());
            }
        }
        // the cliff's own entries on its line, less the old form's
        let mut head = String::from("cliff");
        head.push_str(&format!(" top={}", r.top));
        for e in c.entries() {
            let Some(name) = e.name().map(|n| n.value().to_owned()) else {
                continue;
            };
            if ["form", "along", "height", "depth"].contains(&name.as_str()) {
                continue;
            }
            let t = &text[e.span().offset()..e.span().offset() + e.span().len()];
            head.push(' ');
            head.push_str(t.trim());
        }
        head.push_str(&format!(" at={}", r.crag));
        let mut out = head;
        if kept.is_empty() && r.pins.is_empty() {
            // no block
        } else {
            out.push_str(" {\n");
            for p in &r.pins {
                out.push_str(&format!("{ind}    {p}\n"));
            }
            for k in &kept {
                out.push_str(&format!("{ind}    {k}\n"));
            }
            out.push_str(&format!("{ind}}}"));
        }
        if film {
            out.push_str(&format!(
                "\n{ind}dirt top=\"bottom + [0.6 .. 1.2]\" at={}",
                r.film
            ));
        }
        edits.push(Edit {
            start: off,
            end: off + len,
            with: out,
        });
        if apron.is_none() {
            apron = Some((r.crag.clone(), 0.0, (0.0, 0.0)));
        }
    }
    // the bank on the dirt: drawn as pins
    for d in kids.nodes().iter().filter(|n| n.name().value() == "dirt") {
        let Some(b) = d.children() else { continue };
        let Some(bank) = b.nodes().iter().find(|k| {
            k.name().value() == "bank"
                && prop(k, "against").and_then(|v| v.as_string()) == Some("cliff")
        }) else {
            continue;
        };
        let (h0, h1) = range_of(prop(bank, "height"), (1.5, 3.5));
        let (r0, r1) = range_of(prop(bank, "reach"), (3.0, 6.5));
        let _ = r0;
        let crag = apron.as_ref().map_or("crag", |a| a.0.as_str()).to_owned();
        region_lines.push(format!(
            "region apron {{ within {} of={crag} }}",
            round1(r1)
        ));
        let (bo, bl) = (bank.span().offset(), bank.span().len());
        let (s, e) = (line_start(text, bo), line_end(text, bo + bl));
        let nind = indent(text, d.span().offset());
        edits.push(Edit {
            start: s,
            end: e,
            with: format!(
                "{nind}    pin top=\"[dirtEdgeHeight .. dirtEdgeHeight + {}]\" at=apron\n{nind}    pin top=\"dirtEdgeHeight + [{} .. {}]\" at={crag}\n",
                round1(h1),
                round1(h0),
                round1(h1)
            ),
        });
    }
    // the regions, before the stack
    let at = line_start(text, stack.span().offset());
    let mut block = String::new();
    for l in &region_lines {
        block.push_str(&format!("{sind}{l}\n"));
    }
    block.push('\n');
    edits.push(Edit {
        start: at,
        end: at,
        with: block,
    });
    edits.sort_by_key(|e| std::cmp::Reverse((e.start, e.end)));
    let mut out = text.to_owned();
    for e in edits {
        out.replace_range(e.start..e.end, &e.with);
    }
    Ok(Some(out))
}

/// `floor` as `flagstones`; `Ok(None)` when the tile has no `floor`.
fn floors(text: &str) -> Result<Option<String>, EditError> {
    let doc = KdlDocument::parse(text).map_err(|e| EditError::Parse(e.to_string()))?;
    let stack = doc
        .nodes()
        .iter()
        .find(|n| n.name().value() == "tile")
        .and_then(|n| n.children())
        .and_then(|b| b.nodes().iter().find(|n| n.name().value() == "stack"))
        .and_then(|n| n.children());
    let Some(kids) = stack else {
        return Ok(None);
    };
    let mut edits: Vec<Edit> = Vec::new();
    for n in kids.nodes().iter().filter(|n| n.name().value() == "floor") {
        let (off, len) = (n.span().offset(), n.span().len());
        edits.push(Edit {
            start: off,
            end: off + 5,
            with: "flagstones".to_owned(),
        });
        // the level, as a property or a child
        let mut level: Option<String> = None;
        if let Some(e) = n.entry("level") {
            level = num_text(e.value()).map(|l| height(&l));
            let s = e.span().offset();
            let start = if text[..s].ends_with(' ') { s - 1 } else { s };
            edits.push(Edit {
                start,
                end: s + e.span().len(),
                with: String::new(),
            });
        }
        let mut brace = None;
        if let Some(body) = n.children() {
            brace = text[off..off + len].find('{').map(|b| off + b);
            for c in body.nodes().iter().filter(|c| c.name().value() == "level") {
                level = c
                    .entries()
                    .first()
                    .and_then(|e| num_text(e.value()))
                    .map(|l| height(&l));
                let (co, cl) = (c.span().offset(), c.span().len());
                edits.push(Edit {
                    start: line_start(text, co),
                    end: line_end(text, co + cl),
                    with: String::new(),
                });
            }
        }
        let Some(level) = level.filter(|l| l != "floorHeight" && l != "7.4") else {
            continue;
        };
        let top = if level.contains(' ') {
            format!("\"{level}\"")
        } else {
            level
        };
        let ind = indent(text, off);
        let pin = format!("pin top={top} at=edges");
        // `top=` goes on the head, the pin into the block
        let head_end = brace.unwrap_or_else(|| {
            let node = text[off..off + len].trim_end();
            off + node.len()
        });
        let head_end = if brace.is_some() {
            text[..head_end].trim_end().len()
        } else {
            head_end
        };
        let own = if brace.is_some() {
            format!(" top={top}")
        } else {
            format!(" top={top} {{ {pin} }}")
        };
        edits.push(Edit {
            start: head_end,
            end: head_end,
            with: own,
        });
        if let Some(b) = brace {
            edits.push(Edit {
                start: b + 1,
                end: b + 1,
                with: format!("\n{ind}    {pin}"),
            });
        }
    }
    if edits.is_empty() {
        return Ok(None);
    }
    edits.sort_by_key(|e| std::cmp::Reverse((e.start, e.end)));
    let mut out = text.to_owned();
    for e in edits {
        out.replace_range(e.start..e.end, &e.with);
    }
    Ok(Some(out))
}

#[allow(clippy::too_many_lines)]
fn heights_only(text: &str) -> Result<Option<String>, EditError> {
    let doc = KdlDocument::parse(text).map_err(|e| EditError::Parse(e.to_string()))?;
    let Some(tile) = doc.nodes().iter().find(|n| n.name().value() == "tile") else {
        return Ok(None);
    };
    let Some(body) = tile.children() else {
        return Ok(None);
    };
    let Some(stack) = body.nodes().iter().find(|n| n.name().value() == "stack") else {
        return Ok(None);
    };
    let Some(kids) = stack.children() else {
        return Ok(None);
    };
    let lines: Vec<String> = body
        .nodes()
        .iter()
        .filter(|n| n.name().value() == "line")
        .filter_map(|n| n.entries().first()?.value().as_string().map(str::to_owned))
        .collect();
    let first = kids.nodes().first();
    let has_base = first.is_some_and(|n| n.name().value() == "base");
    // the beds under water: `ground` with a water level
    let beds: Vec<&KdlNode> = kids
        .nodes()
        .iter()
        .filter(|n| {
            n.name().value() == "ground"
                && n.entry("in").is_some()
                && prop(n, "level")
                    .and_then(num_text)
                    .is_some_and(|l| l.contains("water"))
        })
        .collect();
    let old_forms = kids.nodes().iter().any(|n| {
        n.name().value() == "ground"
            || (n.name().value() == "dirt"
                && ["level", "bumps"].iter().any(|k| n.entry(*k).is_some()))
    });
    if has_base && !old_forms {
        return Ok(None);
    }
    let region_of = |n: &KdlNode| -> String {
        prop(n, "in")
            .and_then(|v| v.as_string())
            .map_or_else(|| "tile".to_owned(), str::to_owned)
    };
    let mut edits: Vec<Edit> = Vec::new();
    let ind = first.map_or_else(
        || "        ".to_owned(),
        |n| indent(text, n.span().offset()),
    );
    // base first
    if !has_base {
        let at = first.map_or(0, |n| line_start(text, n.span().offset()));
        if first.is_none()
            || text[..at].contains("stack {") && line_start(text, stack.span().offset()) == at
        {
            return Err(EditError::Parse("a stack written on one line".to_owned()));
        }
        edits.push(Edit {
            start: at,
            end: at,
            with: format!("{ind}base\n"),
        });
    }
    // the strip round the shore lines, before the stack
    let wide =
        !beds.is_empty() && !lines.is_empty() && beds.iter().any(|b| b.entry("in").is_some());
    if wide {
        let sind = indent(text, stack.span().offset());
        let at = line_start(text, stack.span().offset());
        let withins: Vec<String> = lines
            .iter()
            .map(|l| format!("within {SHORE_MM} of={l}"))
            .collect();
        edits.push(Edit {
            start: at,
            end: at,
            with: format!("{sind}region shoreBand {{ {} }}\n", withins.join("; ")),
        });
    }
    let mut pinned_dirt = false;
    for n in kids.nodes() {
        let (off, len) = (n.span().offset(), n.span().len());
        let name = n.name().value();
        match name {
            "ground" if beds.iter().any(|b| std::ptr::eq(*b, n)) => {
                // dropped; its pin goes on the land's dirt
                let (s, e) = (line_start(text, off), line_end(text, off + len));
                edits.push(Edit {
                    start: s,
                    end: e,
                    with: String::new(),
                });
            }
            "ground" | "dirt" => {
                let is_ground = name == "ground";
                let bed_whole = is_ground && n.entry("in").is_none();
                // the dirt that takes the pins: the first land dirt of a tile with beds
                let takes_pins = !pinned_dirt && !is_ground && !beds.is_empty();
                let whole_bed = bed_whole
                    && prop(n, "level")
                        .and_then(num_text)
                        .is_some_and(|l| l.contains("water"));
                let (top, hi) = if whole_bed {
                    (
                        Some("waterBedHeight".to_owned()),
                        "waterBedHeight".to_owned(),
                    )
                } else {
                    top_of(n)
                };
                // the node's text, its kept entries and its block
                let node_text = text[off..off + len].trim_end();
                let tail = &text[off + node_text.len()..off + len];
                let brace = node_text.find('{');
                let head = &node_text[..brace.unwrap_or(node_text.len())];
                let mut props: Vec<String> = Vec::new();
                for e in n.entries() {
                    let key = e.name().map(|k| k.value());
                    let drop =
                        matches!(key, Some("level" | "bumps")) || (key == Some("in") && takes_pins);
                    if drop {
                        continue;
                    }
                    let (s, f) = (
                        e.span().offset() - off,
                        e.span().offset() - off + e.span().len(),
                    );
                    props.push(head[s..f].trim().to_owned());
                }
                if let Some(t) = &top {
                    props.push(if t.starts_with('[') || t.contains(' ') {
                        format!("top=\"{t}\"")
                    } else {
                        format!("top={t}")
                    });
                }
                if is_ground {
                    props.push("clods=none".to_owned());
                    props.push("stones=none".to_owned());
                }
                let inner = brace.map(|b| {
                    let close = node_text.rfind('}').unwrap_or(node_text.len());
                    node_text[b + 1..close].to_owned()
                });
                let nind = indent(text, off);
                let mut pins = String::new();
                if takes_pins {
                    pinned_dirt = true;
                    if wide {
                        pins.push_str(&format!(
                            "{nind}    pin top=\"[waterBedHeight .. {hi}]\" at=shoreBand   // anywhere from the bed to the land\n"
                        ));
                    }
                    for b in &beds {
                        pins.push_str(&format!(
                            "{nind}    pin top=waterBedHeight at={}   // the bed, written last so it wins\n",
                            match region_of(b).as_str() {
                                "tile" => "tile".to_owned(),
                                r => r.to_owned(),
                            }
                        ));
                    }
                }
                if whole_bed {
                    pins.push_str(&format!(
                        "{nind}    pin top=waterBedHeight at=edges   // the bed meets the rim\n"
                    ));
                }
                let mut out = format!(
                    "dirt{}{}",
                    if props.is_empty() { "" } else { " " },
                    props.join(" ")
                );
                if pins.is_empty() && inner.is_none() {
                    // no block
                } else {
                    out.push_str(" {\n");
                    out.push_str(&pins);
                    match &inner {
                        Some(i) => {
                            let i = i.strip_prefix('\n').unwrap_or(i);
                            out.push_str(i);
                            if !i.ends_with(&nind) && !i.ends_with('\n') {
                                out.push('\n');
                                out.push_str(&nind);
                            }
                        }
                        None => out.push_str(&nind),
                    }
                    out.push('}');
                }
                out.push_str(tail);
                edits.push(Edit {
                    start: off,
                    end: off + len,
                    with: out,
                });
            }
            _ => {}
        }
    }
    edits.sort_by_key(|e| std::cmp::Reverse((e.start, e.end)));
    let mut out = text.to_owned();
    for e in edits {
        out.replace_range(e.start..e.end, &e.with);
    }
    Ok(Some(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pond_gets_its_bed_pins_and_a_base() {
        let old = "tile size=\"1x1\" seed=1 {\n    line shore wobble=5 { through \"0,0.4\" \"1,0.4\" }\n    region pool { bounded-by shore; fill \"0.5,0.2\" }\n    region bank { bounded-by shore; fill \"0.5,0.8\" }\n    stack {\n        dirt in=bank\n        grass in=bank\n        ground in=pool level=\"water - 2.2\"\n        rock in=pool\n        water\n    }\n}\n";
        let new = heights(old).unwrap().unwrap();
        assert!(new.contains("        base\n        dirt {"), "{new}");
        assert!(
            new.contains("region shoreBand { within 7 of=shore }"),
            "{new}"
        );
        assert!(new.contains("pin top=waterBedHeight at=pool"), "{new}");
        assert!(!new.contains("ground in="), "{new}");
        assert!(heights(&new).unwrap().is_none());
    }

    #[test]
    fn levels_and_bumps_become_a_top() {
        let old = "tile size=\"2x2\" seed=1 {\n    stack {\n        dirt level=\"edge + 0.3\" bumps=\"2.2..3.2\" bump-size=14 {\n            clods some\n        }\n    }\n}\n";
        let new = heights(old).unwrap().unwrap();
        assert!(
            new.contains(
                "dirt bump-size=14 top=\"[dirtEdgeHeight + 0.3 .. dirtEdgeHeight + 0.3 + 3.2]\" {"
            ),
            "{new}"
        );
        assert!(new.contains("clods some"), "{new}");
    }

    #[test]
    fn a_floor_is_flagstones_and_its_level_a_top() {
        let old = "tile size=\"1x1\" seed=1 {\n    stack {\n        base\n        dirt\n        floor texture=dressed seed=21 {\n            slabs 1\n            level 7.4   // the official floor\n        }\n        floor level=4 style=fieldstone\n    }\n}\n";
        let new = heights(old).unwrap().unwrap();
        assert!(
            new.contains("flagstones texture=dressed seed=21 {"),
            "{new}"
        );
        assert!(!new.contains("level"), "{new}");
        assert!(new.contains("slabs 1"), "{new}");
        assert!(
            new.contains("flagstones style=fieldstone top=4 { pin top=4 at=edges }"),
            "{new}"
        );
        assert!(heights(&new).unwrap().is_none());
    }

    #[test]
    fn a_plain_ground_is_a_quiet_dirt() {
        let old = "tile size=\"1x1\" seed=1 {\n    stack {\n        ground level=edge\n    }\n}\n";
        let new = heights(old).unwrap().unwrap();
        assert!(
            new.contains("dirt top=dirtEdgeHeight clods=none stones=none"),
            "{new}"
        );
    }

    #[test]
    fn a_cliff_run_becomes_a_region_a_band_pins_and_a_film() {
        let old = "tile size=\"2x1\" seed=1 {\n    stack {\n        base\n        dirt top=dirtEdgeHeight {\n            bank against=cliff height=\"1.5..3.5\" reach=\"3..6.5\"\n        }\n        cliff along=north height=35 {\n            rock mixed\n            top dirt\n        }\n    }\n}\n";
        let new = heights(old).unwrap().unwrap();
        assert!(
            new.contains("region crag { within \"[10 .. 20]\" of=northEdge }"),
            "{new}"
        );
        assert!(new.contains("region apron { within 6.5 of=crag }"), "{new}");
        assert!(
            new.contains("cliff top=\"dirtEdgeHeight + 35\" at=crag {"),
            "{new}"
        );
        assert!(
            new.contains("dirt top=\"bottom + [0.6 .. 1.2]\" at=crag"),
            "{new}"
        );
        assert!(new.contains("pin top=\"dirtEdgeHeight + [1.5 .. 3.5]\" at=crag"));
        assert!(!new.contains("bank"), "{new}");
        assert!(heights(&new).unwrap().is_none());
    }

    #[test]
    fn a_free_standing_form_is_nested_discs() {
        let old = "tile size=\"1x1\" seed=1 {\n    stack {\n        base\n        dirt\n        cliff form=mound height=34 {\n            rock mixed\n        }\n    }\n}\n";
        let new = heights(old).unwrap().unwrap();
        assert!(
            new.contains("region crag { within 13.5 of=center }"),
            "{new}"
        );
        assert!(new.contains("pin top="), "{new}");
        assert!(!new.contains("form="), "{new}");
    }
}
