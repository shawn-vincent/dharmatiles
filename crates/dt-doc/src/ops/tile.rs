//! The edit operations on a tile (`.dtile`): each names a node by its id ([`crate::nodeid`]) and changes only
//! what it names.  Comments, blank lines, alignment and every other node stay byte for byte.
//!
//! There is one technique underneath, and only one: the text is parsed (read only) for the byte ranges of the
//! nodes and entries, each operation works out the ranges it changes, and [`lay`] writes the new words over
//! exactly those ranges, last first.  Nothing is printed back from a tree, so nothing about how the author laid
//! the text out can be lost.

use kdl::{KdlDocument, KdlNode, KdlValue};
use miette::SourceSpan;

use super::{Done, EditError, Mended, Op, Place, Slot};
use crate::error::Fix;
use crate::nodeid::{self, entry_range, value_range};
use crate::seams::Snapped;

const INDENT: &str = "    ";

/// New words written over a byte range of the text (an insertion has `at == end`).
#[derive(Debug)]
struct Splice {
    at: usize,
    end: usize,
    with: String,
}

/// `text` with every splice written over its range; the ranges do not overlap.
fn lay(text: &str, mut splices: Vec<Splice>) -> String {
    splices.sort_by_key(|s| std::cmp::Reverse((s.at, s.end)));
    let mut out = text.to_owned();
    for s in splices {
        out.replace_range(s.at..s.end, &s.with);
    }
    out
}

/// A text with one node left to select: its id and where it starts.
fn finish(text: String, at: usize) -> Result<Done, EditError> {
    let id = nodeid::id_at(&text, at)?;
    Ok(Done {
        id: Some(id),
        at: Some(at),
        ..Done::text(text)
    })
}

/// Make `op` on `text`, read as a tile.
pub(super) fn apply(text: &str, op: &Op) -> Result<Done, EditError> {
    let doc = nodeid::parse(text)?;
    match op {
        Op::SetControl { id, name, value } => {
            let node = nodeid::locate(&doc, id)?;
            let splices = match value {
                Some(v) => set_control(text, node, name, v)?,
                None => remove_control(text, node, name)?,
            };
            // the node starts where it did: every change is inside it
            finish(lay(text, splices), node.span().offset())
        }
        Op::SetArg { id, index, value } => {
            let node = nodeid::locate(&doc, id)?;
            let splices = vec![set_arg(text, node, *index, value)?];
            finish(lay(text, splices), node.span().offset())
        }
        Op::SetArgs { id, values } => {
            let node = nodeid::locate(&doc, id)?;
            let args: Vec<_> = node
                .entries()
                .iter()
                .filter(|e| e.name().is_none())
                .collect();
            let (Some(first), Some(last)) = (args.first(), args.last()) else {
                return Err(EditError::NoEntry("argument 0".to_owned()));
            };
            if values.is_empty() {
                return Err(EditError::Cannot("give at least one value".to_owned()));
            }
            let with = values
                .iter()
                .map(|v| word_or_string(v))
                .collect::<Vec<_>>()
                .join(" ");
            let splices = vec![Splice {
                at: value_range(text, first).0,
                end: value_range(text, last).1,
                with,
            }];
            finish(lay(text, splices), node.span().offset())
        }
        Op::SetPoint { id, slot, q, seams } => {
            let node = nodeid::locate(&doc, id)?;
            let (cols, rows) = tile_size(&doc)?;
            let written = match seams.map(|t| t.snap(*q, cols, rows)) {
                Some(Snapped::Word(w)) => w,
                Some(Snapped::Pair(p)) => pair_text(p, rows),
                None => pair_text(*q, rows),
            };
            let value = KdlValue::String(written);
            let splices = match slot {
                Slot::Arg(index) => vec![set_arg(text, node, *index, &value)?],
                Slot::Control(name) => set_control(text, node, name, &value)?,
            };
            finish(lay(text, splices), node.span().offset())
        }
        Op::Insert { place, node } => insert(text, &doc, *place, node),
        Op::Move { id, before } => move_node(text, &doc, id, *before),
        Op::Take { id } => {
            let (at, end) = line_range(text, nodeid::locate(&doc, id)?.span());
            Ok(Done::text(lay(
                text,
                vec![Splice {
                    at,
                    end,
                    with: String::new(),
                }],
            )))
        }
        Op::TakeChildren { id, names } => {
            let parent = nodeid::locate(&doc, id)?;
            let splices = parent
                .children()
                .map(KdlDocument::nodes)
                .unwrap_or_default()
                .iter()
                .filter(|n| names.contains(&n.name().value()))
                .map(|n| line_splice(text, n))
                .collect();
            Ok(Done::text(lay(text, splices)))
        }
        Op::Hide { id } => {
            let at = nodeid::locate(&doc, id)?.span().offset();
            Ok(Done::text(lay(
                text,
                vec![Splice {
                    at,
                    end: at,
                    with: "/- ".to_owned(),
                }],
            )))
        }
        Op::Rename { id, new } => rename(text, &doc, id, new),
        Op::Fix(fixes) => fix(text, fixes),
        _ => Err(EditError::Cannot(
            "that edit is made to a print list or an inventory, not to a tile".to_owned(),
        )),
    }
}

// ---- arguments and points --------------------------------------------------------------------------

/// The write that replaces the positional argument `index` of `node`.
fn set_arg(
    text: &str,
    node: &KdlNode,
    index: usize,
    value: &KdlValue,
) -> Result<Splice, EditError> {
    let e = node
        .entries()
        .iter()
        .filter(|e| e.name().is_none())
        .nth(index)
        .ok_or_else(|| EditError::NoEntry(format!("argument {index}")))?;
    let (at, end) = value_range(text, e);
    Ok(Splice {
        at,
        end,
        with: value.to_string(),
    })
}

/// A word as it is written: bare when it is one KDL reads as itself (a line's name), else a string.
fn word_or_string(s: &str) -> String {
    let plain = s
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
        && !matches!(s, "true" | "false" | "null" | "inf" | "nan");
    if plain {
        s.to_owned()
    } else {
        KdlValue::String(s.to_owned()).to_string()
    }
}

/// The tile's size in squares, from its `size="2x1"`.
fn tile_size(doc: &KdlDocument) -> Result<(u32, u32), EditError> {
    doc.nodes()
        .iter()
        .find(|n| n.name().value() == "tile")
        .and_then(|n| n.entry("size"))
        .and_then(|e| e.value().as_string())
        .and_then(|s| s.split_once('x'))
        .and_then(|(c, r)| Some((c.trim().parse().ok()?, r.trim().parse().ok()?)))
        .ok_or_else(|| {
            EditError::Cannot("the tile has no `size=\"2x1\"` to place a point by".to_owned())
        })
}

/// A free point as a document writes it: `(0.5sq, 0.25sq)`, counted from the north-west corner.
fn pair_text(p: [f64; 2], rows: u32) -> String {
    let round = |v: f64| (v * 100.0).round() / 100.0 + 0.0;
    format!("({}sq, {}sq)", round(p[0]), round(f64::from(rows) - p[1]))
}

// ---- controls ------------------------------------------------------------------------------------

/// The child of `node` called `name`.
fn child<'n>(node: &'n KdlNode, name: &str) -> Option<&'n KdlNode> {
    node.children()?
        .nodes()
        .iter()
        .find(|c| c.name().value() == name)
}

/// The writes that set a control of `node`: its property, else the argument of its own child line, else a new
/// property at the end of the line.  A sub-control (`stones/height`) is a property of its group's line, which
/// is written as a child line when there is none yet.
fn set_control(
    text: &str,
    node: &KdlNode,
    name: &str,
    value: &KdlValue,
) -> Result<Vec<Splice>, EditError> {
    let repr = value.to_string();
    if let Some((group, sub)) = name.split_once('/') {
        return match child(node, group) {
            Some(g) => set_control(text, g, sub, value),
            None => Ok(vec![
                add_child(text, node, None, &format!("{group} {sub}={repr}")).0,
            ]),
        };
    }
    if let Some(e) = node.entry(name) {
        let (at, end) = value_range(text, e);
        return Ok(vec![Splice {
            at,
            end,
            with: repr,
        }]);
    }
    if let Some(c) = child(node, name) {
        let e = c
            .entries()
            .iter()
            .find(|e| e.name().is_none())
            .ok_or_else(|| EditError::NoEntry(format!("{name}'s value")))?;
        let (at, end) = value_range(text, e);
        return Ok(vec![Splice {
            at,
            end,
            with: repr,
        }]);
    }
    let at = node.entries().last().map_or_else(
        || {
            let n = node.name().span();
            n.offset() + n.len()
        },
        |e| entry_range(text, e).1,
    );
    Ok(vec![Splice {
        at,
        end: at,
        with: format!(" {name}={repr}"),
    }])
}

/// The writes that take a control out, in whichever form it is written.  A child line goes with its own line.
/// A sub-control comes off its group's line, and the line goes with the last of its properties.
fn remove_control(text: &str, node: &KdlNode, name: &str) -> Result<Vec<Splice>, EditError> {
    if let Some((group, sub)) = name.split_once('/') {
        let g = child(node, group).ok_or_else(|| EditError::NoEntry(name.to_owned()))?;
        let only = g.children().is_none()
            && matches!(g.entries(), [e] if e.name().is_some_and(|n| n.value() == sub));
        return if only {
            Ok(vec![line_splice(text, g)])
        } else {
            remove_control(text, g, sub)
        };
    }
    if let Some(e) = node
        .entries()
        .iter()
        .find(|e| e.name().is_some_and(|n| n.value() == name))
    {
        let (start, end) = entry_range(text, e);
        // the space before it goes with it
        let at = start - (text[..start].len() - text[..start].trim_end_matches([' ', '\t']).len());
        return Ok(vec![Splice {
            at,
            end,
            with: String::new(),
        }]);
    }
    let c = child(node, name).ok_or_else(|| EditError::NoEntry(name.to_owned()))?;
    Ok(vec![line_splice(text, c)])
}

// ---- nodes ---------------------------------------------------------------------------------------

/// The bytes a node takes out with it: its whole line, with any comment on it, when it has the line to itself;
/// else the node and the space after it.
fn line_range(text: &str, sp: SourceSpan) -> (usize, usize) {
    let (mut s, mut e) = (sp.offset(), sp.offset() + sp.len());
    if text[e..].starts_with(';') {
        e += 1;
    }
    let ls = text[..s].rfind('\n').map_or(0, |n| n + 1);
    let le = text[e..].find('\n').map_or(text.len(), |n| e + n);
    let rest = text[e..le].trim();
    if text[ls..s].trim().is_empty() && (rest.is_empty() || rest.starts_with("//")) {
        s = ls;
        e = (le + 1).min(text.len());
    } else {
        e += text[e..le].len() - text[e..le].trim_start().len();
    }
    (s, e)
}

fn line_splice(text: &str, node: &KdlNode) -> Splice {
    let (at, end) = line_range(text, node.span());
    Splice {
        at,
        end,
        with: String::new(),
    }
}

/// The whitespace a line starts with, for the line holding byte `at`.
fn indent_of(text: &str, at: usize) -> &str {
    let ls = text[..at].rfind('\n').map_or(0, |n| n + 1);
    let line = &text[ls..];
    &line[..line.len() - line.trim_start_matches([' ', '\t']).len()]
}

/// `node` with every line after its first indented by `ind`.
fn reindent(node: &str, ind: &str) -> String {
    node.lines().collect::<Vec<_>>().join(&format!("\n{ind}"))
}

/// The write that puts `node` as the last child of `parent` (or before its child `before`), on a line of its own
/// at its siblings' indent; and the byte the new node's name starts at in the new text.
fn add_child(
    text: &str,
    parent: &KdlNode,
    before: Option<&KdlNode>,
    node: &str,
) -> (Splice, usize) {
    let put = |at: usize, end: usize, with: String| Splice { at, end, with };
    let pspan = parent.span();
    let pind = indent_of(text, pspan.offset());
    let inner = format!("{pind}{INDENT}");
    if let Some(b) = before {
        return insert_above(text, b.span().offset(), node);
    }
    let end = pspan.offset() + pspan.len();
    match parent.children() {
        Some(kids) if !kids.nodes().is_empty() => {
            let last = kids.nodes().last().expect("not empty").span();
            let le = last.offset() + last.len();
            let close = text[..end].rfind('}').unwrap_or(end);
            if let Some(nl) = text[le..close].find('\n') {
                let ins = le + nl + 1;
                let ind = indent_of(text, last.offset()).to_owned();
                (
                    put(ins, ins, format!("{ind}{}\n", reindent(node, &ind))),
                    ins + ind.len(),
                )
            } else {
                // a block on one line: `stack { rock }`
                let s = format!("\n{inner}{}\n{pind}", reindent(node, &inner));
                (put(close, close, s), close + 1 + inner.len())
            }
        }
        Some(_) => {
            let close = text[..end].rfind('}').unwrap_or(end);
            let open = text[..close].rfind('{').unwrap_or(close);
            let s = format!("\n{inner}{}\n{pind}", reindent(node, &inner));
            (put(open + 1, close, s), open + 2 + inner.len())
        }
        None => {
            let s = format!(" {{\n{inner}{}\n{pind}}}", reindent(node, &inner));
            (put(end, end, s), end + 3 + inner.len())
        }
    }
}

/// The write that puts `node` on a line of its own above the node starting at byte `at` (above the `//` comment
/// lines that sit on top of it, which belong to it), at its indent; or, when it shares its line, in front of it as
/// `node; `.  And the byte the new node starts at in the new text.
fn insert_above(text: &str, at: usize, node: &str) -> (Splice, usize) {
    let ls = text[..at].rfind('\n').map_or(0, |n| n + 1);
    if text[ls..at].trim().is_empty() {
        let ind = &text[ls..at];
        let top = lead_start(text, at);
        return (
            Splice {
                at: top,
                end: top,
                with: format!("{ind}{}\n", reindent(node, ind)),
            },
            top + ind.len(),
        );
    }
    (
        Splice {
            at,
            end: at,
            with: format!("{node}; "),
        },
        at,
    )
}

/// The start of the line holding byte `at`, moved up over the `//` comment lines directly above it: the comments
/// that belong to the node there.
fn lead_start(text: &str, at: usize) -> usize {
    let mut ls = text[..at].rfind('\n').map_or(0, |n| n + 1);
    while ls > 0 {
        let prev = text[..ls - 1].rfind('\n').map_or(0, |n| n + 1);
        if text[prev..ls - 1].trim_start().starts_with("//") {
            ls = prev;
        } else {
            break;
        }
    }
    ls
}

/// Write a new node (one or more lines of KDL, unindented) in `place`.
fn insert(text: &str, doc: &KdlDocument, place: Place, node: &str) -> Result<Done, EditError> {
    let node = node.trim();
    nodeid::parse(node)?;
    let tile = || {
        doc.nodes()
            .iter()
            .find(|n| n.name().value() == "tile")
            .ok_or_else(|| EditError::NoNode("tile".into()))
    };
    let (splice, at) = match place {
        Place::In(id) => add_child(text, nodeid::locate(doc, id)?, None, node),
        Place::Before(id) => insert_above(text, nodeid::locate(doc, id)?.span().offset(), node),
        Place::Tile => {
            let tile = tile()?;
            let stack = tile
                .children()
                .and_then(|c| c.nodes().iter().find(|n| n.name().value() == "stack"));
            add_child(text, tile, stack, node)
        }
        Place::Stack => {
            let tile = tile()?;
            match tile
                .children()
                .and_then(|c| c.nodes().iter().find(|n| n.name().value() == "stack"))
            {
                Some(s) => add_child(text, s, None, node),
                None => {
                    let (sp, at) =
                        add_child(text, tile, None, &format!("stack {{\n{INDENT}{node}\n}}"));
                    // the entry is inside the new stack: past its line and its indent
                    let t = lay(text, vec![sp]);
                    let ind = indent_of(&t, at).len();
                    return finish(t, at + "stack {\n".len() + ind + INDENT.len());
                }
            }
        }
    };
    finish(lay(text, vec![splice]), at)
}

/// A node moved whole to stand before its sibling `before`, or last.
fn move_node(
    text: &str,
    doc: &KdlDocument,
    id: &str,
    before: Option<&str>,
) -> Result<Done, EditError> {
    let (sibs, i) = nodeid::siblings(doc, id)?;
    let from = sibs[i].span().offset();
    let j = match before {
        Some(b) => {
            let (others, j) = nodeid::siblings(doc, b)?;
            if others.as_ptr() != sibs.as_ptr() {
                return Err(EditError::Cannot(
                    "a node moves among its own siblings: the target is not one".into(),
                ));
            }
            j
        }
        None => sibs.len(),
    };
    // already there: it stays where it is, byte for byte
    if j == i || j == i + 1 {
        return finish(text.to_owned(), from);
    }
    let (bs, be) = block_of(text, &sibs[i])?;
    // where it goes: above the target's own comments, else after the last sibling
    let p = match sibs.get(j) {
        Some(t) => {
            block_of(text, t)?;
            lead_start(text, t.span().offset())
        }
        None => block_of(text, &sibs[sibs.len() - 1])?.1,
    };
    // the gap it leaves does not keep a blank line on both sides
    let blank_below = be < text.len()
        && text[be..]
            .split('\n')
            .next()
            .is_some_and(|l| l.trim().is_empty());
    let blank_above = bs > 0 && {
        let prev = text[..bs - 1].rfind('\n').map_or(0, |n| n + 1);
        text[prev..bs - 1].trim().is_empty()
    };
    let rm_end = if blank_above && blank_below {
        text[be..].find('\n').map_or(text.len(), |n| be + n + 1)
    } else {
        be
    };
    let mut block = text[bs..be].to_owned();
    if !block.ends_with('\n') {
        block.push('\n');
    }
    let inside = from - bs;
    // the block comes out where it was and goes in at `p`: two writes
    let (cut, put, at) = if p < bs {
        (
            Splice {
                at: bs,
                end: rm_end,
                with: String::new(),
            },
            Splice {
                at: p,
                end: p,
                with: block,
            },
            p + inside,
        )
    } else {
        let p = p.max(rm_end);
        (
            Splice {
                at: bs,
                end: rm_end,
                with: String::new(),
            },
            Splice {
                at: p,
                end: p,
                with: block,
            },
            p - (rm_end - bs) + inside,
        )
    };
    finish(lay(text, vec![cut, put]), at)
}

/// The lines a node takes up, as a byte range: from the comment lines above it to the end of its own last line.
///
/// # Errors
/// [`EditError::Cannot`] when it shares a line with another node.
fn block_of(text: &str, node: &KdlNode) -> Result<(usize, usize), EditError> {
    let sp = node.span();
    let (s, mut e) = (sp.offset(), sp.offset() + sp.len());
    if text[e..].starts_with(';') {
        e += 1;
    }
    let ls = text[..s].rfind('\n').map_or(0, |n| n + 1);
    let le = text[e..].find('\n').map_or(text.len(), |n| e + n);
    let rest = text[e..le].trim();
    if !text[ls..s].trim().is_empty() || !(rest.is_empty() || rest.starts_with("//")) {
        return Err(EditError::Cannot(
            "a node that shares its line with another cannot be moved: write each on a line of its own"
                .into(),
        ));
    }
    Ok((lead_start(text, s), (le + 1).min(text.len())))
}

// ---- names ---------------------------------------------------------------------------------------

/// A line or region renamed, and every place the tile names it: a line in `boundedBy` and `at=`, a region in
/// `at=`.  Only those names change, each written as it was (quoted or bare).
fn rename(text: &str, doc: &KdlDocument, id: &str, new: &str) -> Result<Done, EditError> {
    let def = nodeid::locate(doc, id)?;
    let kind = def.name().value();
    if kind != "line" && kind != "region" {
        return Err(EditError::Cannot(
            "only a line or a region is renamed".into(),
        ));
    }
    let tile = doc
        .nodes()
        .iter()
        .find(|n| n.name().value() == "tile")
        .ok_or_else(|| EditError::NoNode("tile".into()))?;
    let body = tile.children().map(KdlDocument::nodes).unwrap_or_default();
    if !body.iter().any(|n| std::ptr::eq(n, def)) {
        return Err(EditError::Cannot(
            "a line or a region of the tile is renamed".into(),
        ));
    }
    let old = def
        .entries()
        .first()
        .and_then(|e| e.value().as_string())
        .ok_or_else(|| EditError::NoEntry(format!("{kind}'s name")))?;
    let plain = new.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        && new
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if !plain {
        return Err(EditError::BadName(format!(
            "a {kind} is named with a letter, then letters, digits, - and _"
        )));
    }
    if crate::place::Edge::parse(new).is_some() || crate::place::Corner::parse(new).is_some() {
        return Err(EditError::BadName(format!(
            "`{new}` is a tile edge or corner"
        )));
    }
    let named = |n: &KdlNode| {
        n.name().value() == kind
            && n.entries()
                .first()
                .is_some_and(|e| e.name().is_none() && e.value().as_string() == Some(new))
    };
    if new != old && body.iter().any(named) {
        return Err(EditError::BadName(format!(
            "there is already a {kind} called `{new}`"
        )));
    }
    fn walk<'a>(nodes: &'a [KdlNode], out: &mut Vec<&'a KdlNode>) {
        for n in nodes {
            out.push(n);
            if let Some(c) = n.children() {
                walk(c.nodes(), out);
            }
        }
    }
    let mut ranges = vec![value_range(text, &def.entries()[0])];
    let mut all = Vec::new();
    walk(body, &mut all);
    for n in all {
        for (i, e) in n.entries().iter().enumerate() {
            if e.value().as_string() != Some(old) {
                continue;
            }
            let refers = match (kind, e.name().map(|k| k.value())) {
                ("line", None) => n.name().value() == "boundedBy",
                (_, Some("at")) => true,
                _ => false,
            };
            // the definition's own name is already in
            if refers && !(i == 0 && std::ptr::eq(n, def)) {
                ranges.push(value_range(text, e));
            }
        }
    }
    ranges.sort_unstable();
    ranges.dedup();
    let splices = ranges
        .into_iter()
        .map(|(at, end)| Splice {
            at,
            end,
            with: if text[at..end].starts_with('"') {
                format!("\"{new}\"")
            } else {
                new.to_owned()
            },
        })
        .collect();
    Ok(Done::text(lay(text, splices)))
}

// ---- fixes ---------------------------------------------------------------------------------------

/// The program's fixes laid into `text`, the text they were made for: each writes its replacement over its bytes,
/// and only where the text still reads what the fix saw (`was`).  A fix that overlaps one already made waits
/// (`deferred`), and the same fix twice is made once.
fn fix(text: &str, fixes: &[Fix]) -> Result<Done, EditError> {
    let mut order: Vec<&Fix> = fixes.iter().collect();
    order.sort_by_key(|f| (f.offset, f.len));
    order.dedup_by(|a, b| a.offset == b.offset && a.len == b.len && a.replacement == b.replacement);
    let (mut applied, mut deferred) = (Vec::new(), 0);
    let mut end = 0;
    for f in order {
        let there = text.get(f.offset..f.offset + f.len);
        if there != Some(f.was.as_str()) {
            return Err(EditError::Cannot(format!(
                "the text at byte {} no longer reads `{}`: the fix `{}` was made for another text",
                f.offset, f.was, f.label
            )));
        }
        if f.offset < end || (f.len == 0 && f.offset == end && !applied.is_empty()) {
            deferred += 1;
            continue;
        }
        end = f.offset + f.len;
        applied.push(f.clone());
    }
    let splices = applied
        .iter()
        .map(|f| Splice {
            at: f.offset,
            end: f.offset + f.len,
            with: f.replacement.clone(),
        })
        .collect();
    Ok(Done {
        mended: Some(Mended { applied, deferred }),
        ..Done::text(lay(text, splices))
    })
}

#[cfg(test)]
mod tests;
