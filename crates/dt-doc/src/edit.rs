//! Format-preserving edits through the kdl crate's document API.
//!
//! This is how the GUI, the MCP server and agents change a document: the
//! file is parsed, one node is changed, and the document is written back.
//! Comments, blank lines, alignment and every other node stay byte for
//! byte as they were.
//!
//! A node is addressed by a path of steps from the top of the file, as
//! [`crate::model::Entry::path`] gives it:
//!
//! - `"stack"` — the first child called `stack`;
//! - `"tile"` — the tile (a definition is `"define <name>"`, the first
//!   `define` whose first argument is `<name>`);
//! - `"scatter#2"` — the second child called `scatter`.
//!
//! [`set_control`] is the call a GUI wants: it changes a control in
//! whichever form it is written (`height=35` on the line, or `height 35`
//! on its own line), and writes a control that isn't there yet on the
//! line, as a property.  [`remove_control`] takes one out in
//! either form.  [`set_value`] and [`insert_property`] are the lower-level
//! pieces.

use kdl::{KdlDocument, KdlEntry, KdlNode, KdlValue};

/// Which entry of the target node to change.
#[derive(Debug, Clone, Copy)]
pub enum Key<'a> {
    /// A property, `name=value`.
    Prop(&'a str),
    /// A positional argument, counting from 0.
    Arg(usize),
}

/// Why an edit could not be made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditError {
    /// The text is not valid KDL v2.
    Parse(String),
    /// No node matched this step of the path.
    NoNode(String),
    /// The node has no such entry or control.
    NoEntry(String),
    /// The control is already written; set it instead of inserting.
    Exists(String),
    /// A name that can't be used here, and why.
    BadName(String),
}

impl std::fmt::Display for EditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EditError::Parse(e) => write!(f, "not KDL v2: {e}"),
            EditError::NoNode(s) => write!(f, "no node matches `{s}`"),
            EditError::NoEntry(s) => write!(f, "the node has no `{s}`"),
            EditError::Exists(s) => write!(f, "`{s}` is already written"),
            EditError::BadName(s) => f.write_str(s),
        }
    }
}

impl std::error::Error for EditError {}

/// A path step: a name, an optional first-argument match, and which of
/// the matching siblings (1-based).
fn parse_step(step: &str) -> (&str, Option<&str>, usize) {
    let (step, nth) = match step.rsplit_once('#') {
        Some((s, n)) if n.parse::<usize>().is_ok_and(|n| n > 0) => (s, n.parse().unwrap_or(1)),
        _ => (step, 1),
    };
    match step.split_once(' ') {
        Some((n, a)) => (n, Some(a), nth),
        None => (step, None, nth),
    }
}

fn matches(node: &KdlNode, name: &str, arg: Option<&str>) -> bool {
    node.name().value() == name
        && arg.is_none_or(|a| {
            node.entries()
                .first()
                .is_some_and(|e| e.name().is_none() && e.value().as_string() == Some(a))
        })
}

fn find<'d>(nodes: &'d mut [KdlNode], step: &str) -> Option<&'d mut KdlNode> {
    let (name, arg, nth) = parse_step(step);
    nodes
        .iter_mut()
        .filter(|n| matches(n, name, arg))
        .nth(nth - 1)
}

/// Parse `text` and run `f` on the node at `path`, then print it back.
fn with_node(
    text: &str,
    path: &[&str],
    f: impl FnOnce(&mut KdlNode) -> Result<(), EditError>,
) -> Result<String, EditError> {
    let mut doc = KdlDocument::parse(text).map_err(|e| EditError::Parse(e.to_string()))?;
    let mut nodes: &mut Vec<KdlNode> = doc.nodes_mut();
    let mut target: Option<&mut KdlNode> = None;
    for (i, step) in path.iter().enumerate() {
        let node = find(nodes, step).ok_or_else(|| EditError::NoNode(path[..=i].join(" > ")))?;
        if i + 1 == path.len() {
            target = Some(node);
            break;
        }
        nodes = node
            .children_mut()
            .as_mut()
            .ok_or_else(|| EditError::NoNode(path[..=i + 1].join(" > ")))?
            .nodes_mut();
    }
    let node = target.ok_or_else(|| EditError::NoNode(String::new()))?;
    f(node)?;
    Ok(doc.to_string())
}

/// Set an entry's value, keeping its surroundings exactly.
fn replace(entry: &mut KdlEntry, value: KdlValue) {
    let repr = value.to_string();
    entry.set_value(value);
    // The entry prints its original text unless told otherwise; only the
    // value's own text changes, never its surroundings.
    if let Some(f) = entry.format_mut() {
        f.value_repr = repr;
    }
}

/// A child node that writes one control: `height 35`.
fn child_index(node: &KdlNode, name: &str) -> Option<usize> {
    node.children()?
        .nodes()
        .iter()
        .position(|c| c.name().value() == name)
}

/// Replace one value and return the new text.  Everything outside the
/// replaced value is kept exactly.
///
/// # Errors
/// If the text doesn't parse, or the path or key matches nothing.
pub fn set_value(
    text: &str,
    path: &[&str],
    key: Key<'_>,
    value: impl Into<KdlValue>,
) -> Result<String, EditError> {
    let value = value.into();
    with_node(text, path, |node| {
        let entry = match key {
            Key::Prop(k) => node.entry_mut(k),
            Key::Arg(i) => node.entry_mut(i),
        }
        .ok_or_else(|| {
            EditError::NoEntry(match key {
                Key::Prop(k) => k.to_owned(),
                Key::Arg(i) => format!("argument {i}"),
            })
        })?;
        replace(entry, value);
        Ok(())
    })
}

/// Write a new control on the node's line, as a property `name=value`,
/// after its other entries.  Nothing else in the file changes.
///
/// # Errors
/// [`EditError::Exists`] if the control is already written in either
/// form; otherwise as [`set_value`].
pub fn insert_property(
    text: &str,
    path: &[&str],
    name: &str,
    value: impl Into<KdlValue>,
) -> Result<String, EditError> {
    let value = value.into();
    with_node(text, path, |node| {
        if node.entry(name).is_some() || child_index(node, name).is_some() {
            return Err(EditError::Exists(name.to_owned()));
        }
        node.push(KdlEntry::new_prop(name, value));
        Ok(())
    })
}

/// Set a control in whichever form it is written: the property on the
/// line, or the first argument of its own child line.  A control that
/// isn't written yet goes on the line as a property.
///
/// # Errors
/// If the text doesn't parse or the path matches nothing, or the
/// control's child line has no value to set (a group such as `leaf`:
/// address its line and set its properties instead).
pub fn set_control(
    text: &str,
    path: &[&str],
    name: &str,
    value: impl Into<KdlValue>,
) -> Result<String, EditError> {
    let value = value.into();
    with_node(text, path, |node| {
        if let Some(e) = node.entry_mut(name) {
            replace(e, value);
            return Ok(());
        }
        if let Some(i) = child_index(node, name) {
            let child = node
                .children_mut()
                .as_mut()
                .map(|c| &mut c.nodes_mut()[i])
                .ok_or_else(|| EditError::NoEntry(name.to_owned()))?;
            let e = child
                .entries_mut()
                .iter_mut()
                .find(|e| e.name().is_none())
                .ok_or_else(|| EditError::NoEntry(format!("{name}'s value")))?;
            replace(e, value);
            return Ok(());
        }
        node.push(KdlEntry::new_prop(name, value));
        Ok(())
    })
}

/// Take a control out, in whichever form it is written.  A child line is
/// removed with its own line; comments on neighbouring lines stay.
///
/// # Errors
/// [`EditError::NoEntry`] if the control isn't written; otherwise as
/// [`set_value`].
pub fn remove_control(text: &str, path: &[&str], name: &str) -> Result<String, EditError> {
    with_node(text, path, |node| {
        if let Some(i) = node
            .entries()
            .iter()
            .position(|e| e.name().is_some_and(|n| n.value() == name))
        {
            node.entries_mut().remove(i);
            return Ok(());
        }
        let i = child_index(node, name).ok_or_else(|| EditError::NoEntry(name.to_owned()))?;
        let kids = node
            .children_mut()
            .as_mut()
            .ok_or_else(|| EditError::NoEntry(name.to_owned()))?;
        remove_child(kids, i);
        Ok(())
    })
}

/// Remove child `i`, keeping whatever its leading text carried from the
/// line before (a trailing comment on the parent's line, blank lines).
fn remove_child(kids: &mut KdlDocument, i: usize) {
    let removed = kids.nodes_mut().remove(i);
    let leading = removed.format().map_or("", |f| f.leading.as_str());
    let keep = leading
        .rfind('\n')
        .map_or("", |n| &leading[..=n])
        .to_owned();
    if keep.is_empty() {
        return;
    }
    if let Some(next) = kids.nodes_mut().get_mut(i) {
        if let Some(f) = next.format_mut() {
            f.leading = format!("{keep}{}", f.leading);
        }
    } else if let Some(f) = kids.format_mut() {
        f.trailing = format!("{keep}{}", f.trailing);
    }
}

/// The path (as [`set_control`] takes it) of the node whose text holds
/// byte `offset` most deeply: give the offset a node's name starts at and
/// get that node.  Each step is `name#k`, `k` counting same-named siblings.
///
/// # Errors
/// If the text doesn't parse, or no node holds the offset.
pub fn path_at(text: &str, offset: usize) -> Result<Vec<String>, EditError> {
    let doc = KdlDocument::parse(text).map_err(|e| EditError::Parse(e.to_string()))?;
    let mut path = Vec::new();
    let mut nodes = doc.nodes();
    'down: loop {
        let mut seen: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
        for n in nodes {
            let name = n.name().value();
            let k = seen.entry(name).or_default();
            *k += 1;
            let sp = n.span();
            if offset >= sp.offset() && offset < sp.offset() + sp.len().max(1) {
                path.push(format!("{name}#{k}"));
                match n.children() {
                    Some(c) => {
                        nodes = c.nodes();
                        continue 'down;
                    }
                    None => break 'down,
                }
            }
        }
        break;
    }
    if path.is_empty() {
        return Err(EditError::NoNode(format!("byte {offset}")));
    }
    Ok(path)
}

/// [`set_control`] on the node at byte `offset` (see [`path_at`]).
///
/// # Errors
/// As [`path_at`] and [`set_control`].
pub fn set_control_at(
    text: &str,
    offset: usize,
    name: &str,
    value: impl Into<KdlValue>,
) -> Result<String, EditError> {
    let path = path_at(text, offset)?;
    let steps: Vec<&str> = path.iter().map(String::as_str).collect();
    set_control(text, &steps, name, value)
}

/// Replace the positional argument `index` of the node at byte `offset`
/// (see [`path_at`]): a `within`'s distance, say.
///
/// # Errors
/// As [`path_at`] and [`set_value`].
pub fn set_arg_at(
    text: &str,
    offset: usize,
    index: usize,
    value: impl Into<KdlValue>,
) -> Result<String, EditError> {
    let path = path_at(text, offset)?;
    let steps: Vec<&str> = path.iter().map(String::as_str).collect();
    set_value(text, &steps, Key::Arg(index), value)
}

/// [`remove_control`] on the node at byte `offset` (see [`path_at`]).
///
/// # Errors
/// As [`path_at`] and [`remove_control`].
pub fn remove_control_at(text: &str, offset: usize, name: &str) -> Result<String, EditError> {
    let path = path_at(text, offset)?;
    let steps: Vec<&str> = path.iter().map(String::as_str).collect();
    remove_control(text, &steps, name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_node_at_an_offset() {
        let t = "tile {\n    stack {\n        dirt\n        scatter count=2 { rock size=\"6..10\" }\n        scatter\n    }\n}\n";
        let at = |s: &str| t.find(s).unwrap();
        assert_eq!(
            path_at(t, at("dirt")).unwrap(),
            ["tile#1", "stack#1", "dirt#1"]
        );
        assert_eq!(
            path_at(t, at("rock")).unwrap(),
            ["tile#1", "stack#1", "scatter#1", "rock#1"]
        );
        assert_eq!(
            path_at(t, t.rfind("scatter").unwrap()).unwrap(),
            ["tile#1", "stack#1", "scatter#2"]
        );
        let out = set_control_at(t, at("rock"), "size", "8..12").unwrap();
        assert!(out.contains("rock size=\"8..12\" }"), "{out}");
        let out = set_control_at(t, at("dirt"), "wear", "worn").unwrap();
        assert!(out.contains("        dirt wear=worn\n"), "{out}");
        let out = remove_control_at(t, at("scatter"), "count").unwrap();
        assert!(out.contains("        scatter { rock"), "{out}");
    }

    const SRC: &str = "// top\nthing a=1   b=\"x\" { // note\n    kid 4.5 // keep\n    kid 6\n}\n";

    #[test]
    fn edits_a_property_in_place() {
        let out = set_value(SRC, &["thing"], Key::Prop("a"), 2).unwrap();
        assert_eq!(out, SRC.replace("a=1", "a=2"));
    }

    #[test]
    fn edits_a_child_argument_in_place() {
        let out = set_value(SRC, &["thing", "kid"], Key::Arg(0), 5.25).unwrap();
        assert_eq!(out, SRC.replace("kid 4.5", "kid 5.25"));
        let out = set_value(SRC, &["thing", "kid#2"], Key::Arg(0), 7).unwrap();
        assert_eq!(out, SRC.replace("kid 6", "kid 7"));
    }

    #[test]
    fn strings_come_out_quoted_and_words_bare() {
        let out = set_value(SRC, &["thing"], Key::Prop("b"), "9..15").unwrap();
        assert_eq!(out, SRC.replace("b=\"x\"", "b=\"9..15\""));
        let out = set_value(SRC, &["thing"], Key::Prop("b"), "round").unwrap();
        assert_eq!(out, SRC.replace("b=\"x\"", "b=round"));
    }

    #[test]
    fn insert_goes_on_the_line() {
        let out = insert_property(SRC, &["thing"], "c", 3).unwrap();
        assert_eq!(out, SRC.replace("b=\"x\" {", "b=\"x\" c=3 {"));
        assert_eq!(
            insert_property(SRC, &["thing"], "a", 3),
            Err(EditError::Exists("a".to_owned()))
        );
        assert_eq!(
            insert_property(SRC, &["thing"], "kid", 3),
            Err(EditError::Exists("kid".to_owned()))
        );
    }

    #[test]
    fn set_control_keeps_the_form() {
        let out = set_control(SRC, &["thing"], "kid", 9).unwrap();
        assert_eq!(out, SRC.replace("kid 4.5", "kid 9"));
        let out = set_control(SRC, &["thing"], "a", 9).unwrap();
        assert_eq!(out, SRC.replace("a=1", "a=9"));
        let out = set_control(SRC, &["thing"], "z", "1,1").unwrap();
        assert_eq!(out, SRC.replace("b=\"x\" {", "b=\"x\" z=\"1,1\" {"));
    }

    #[test]
    fn remove_in_either_form() {
        let out = remove_control(SRC, &["thing"], "a").unwrap();
        assert_eq!(
            out,
            SRC.replace("a=1", "").replace("thing    b", "thing   b")
        );
        let out = remove_control(SRC, &["thing"], "kid").unwrap();
        assert_eq!(out, SRC.replace("    kid 4.5 // keep\n", ""));
        // The parent line's comment, carried by the first child, stays.
        assert!(out.contains("{ // note\n    kid 6\n}"), "{out}");
        assert!(matches!(
            remove_control(SRC, &["thing"], "q"),
            Err(EditError::NoEntry(_))
        ));
    }

    #[test]
    fn missing_things_are_errors() {
        assert!(matches!(
            set_value(SRC, &["nope"], Key::Arg(0), 1),
            Err(EditError::NoNode(_))
        ));
        assert!(matches!(
            set_value(SRC, &["thing", "kid#3"], Key::Arg(0), 1),
            Err(EditError::NoNode(_))
        ));
        assert!(matches!(
            set_value(SRC, &["thing"], Key::Prop("zz"), 1),
            Err(EditError::NoEntry(_))
        ));
        assert!(matches!(
            set_value("a b=true", &["a"], Key::Prop("b"), 1),
            Err(EditError::Parse(_))
        ));
    }
}

/// Where [`insert_node`] puts a new node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// The end of the tile's `stack` (made when the tile has none).
    Stack,
    /// The tile's own body, before its `stack`: a `line` or a `region`.
    Tile,
}

const INDENT: &str = "    ";

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

/// Write `node` as the last child of `parent` (or before its child
/// `before`), on a line of its own at its siblings' indent.  Returns the
/// text and the byte the new node's name starts at.
fn add_child(
    text: &str,
    parent: &KdlNode,
    before: Option<&KdlNode>,
    node: &str,
) -> (String, usize) {
    let splice = |at: usize, s: &str| format!("{}{s}{}", &text[..at], &text[at..]);
    let pspan = parent.span();
    let pind = indent_of(text, pspan.offset());
    let inner = format!("{pind}{INDENT}");
    if let Some(b) = before {
        let at = b.span().offset();
        let ls = text[..at].rfind('\n').map_or(0, |n| n + 1);
        if text[ls..at].trim().is_empty() {
            let ind = &text[ls..at];
            return (
                splice(ls, &format!("{ind}{}\n", reindent(node, ind))),
                ls + ind.len(),
            );
        }
        return (splice(at, &format!("{node}; ")), at);
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
                    splice(ins, &format!("{ind}{}\n", reindent(node, &ind))),
                    ins + ind.len(),
                )
            } else {
                // a block on one line: `stack { rock }`
                let s = format!("\n{inner}{}\n{pind}", reindent(node, &inner));
                (splice(close, &s), close + 1 + inner.len())
            }
        }
        Some(_) => {
            let close = text[..end].rfind('}').unwrap_or(end);
            let open = text[..close].rfind('{').unwrap_or(close);
            let s = format!("\n{inner}{}\n{pind}", reindent(node, &inner));
            (
                format!("{}{s}{}", &text[..=open], &text[close..]),
                open + 2 + inner.len(),
            )
        }
        None => {
            let s = format!(" {{\n{inner}{}\n{pind}}}", reindent(node, &inner));
            (splice(end, &s), end + 3 + inner.len())
        }
    }
}

/// Write a new node (one or more lines of KDL, unindented) into the
/// file's tile: at the end of its stack, or in its body before the stack.
/// Everything else stays byte for byte.  Returns the new text and the
/// byte the new node's name starts at, to select it.
///
/// # Errors
/// If the text doesn't parse, has no `tile`, or `node` is not KDL.
pub fn insert_node(text: &str, place: Place, node: &str) -> Result<(String, usize), EditError> {
    let node = node.trim();
    KdlDocument::parse(node).map_err(|e| EditError::Parse(e.to_string()))?;
    let doc = KdlDocument::parse(text).map_err(|e| EditError::Parse(e.to_string()))?;
    let tile = doc
        .nodes()
        .iter()
        .find(|n| n.name().value() == "tile")
        .ok_or_else(|| EditError::NoNode("tile".into()))?;
    let stack = tile
        .children()
        .and_then(|c| c.nodes().iter().find(|n| n.name().value() == "stack"));
    Ok(match (place, stack) {
        (Place::Stack, Some(s)) => add_child(text, s, None, node),
        (Place::Stack, None) => {
            let (t, at) = add_child(text, tile, None, &format!("stack {{\n{INDENT}{node}\n}}"));
            let ind = indent_of(&t, at).len();
            (t, at + "stack {\n".len() + ind + INDENT.len())
        }
        (Place::Tile, s) => add_child(text, tile, s, node),
    })
}

/// The innermost node whose text holds byte `offset`.
fn node_at(doc: &KdlDocument, offset: usize) -> Option<&KdlNode> {
    let mut found = None;
    let mut nodes = doc.nodes();
    'down: loop {
        for n in nodes {
            let sp = n.span();
            if offset >= sp.offset() && offset < sp.offset() + sp.len().max(1) {
                found = Some(n);
                match n.children() {
                    Some(c) => {
                        nodes = c.nodes();
                        continue 'down;
                    }
                    None => break 'down,
                }
            }
        }
        break;
    }
    found
}

/// The properties written on the node at byte `offset` (see [`path_at`]):
/// each name, its value as written, and the value's byte span.
///
/// # Errors
/// If the text doesn't parse, or no node holds the offset.
pub fn props_at(text: &str, offset: usize) -> Result<Vec<(String, String, [usize; 2])>, EditError> {
    let doc = KdlDocument::parse(text).map_err(|e| EditError::Parse(e.to_string()))?;
    let node = node_at(&doc, offset).ok_or_else(|| EditError::NoNode(format!("byte {offset}")))?;
    Ok(node
        .entries()
        .iter()
        .filter_map(|e| {
            let name = e.name()?.value().to_owned();
            let sp = e.span();
            let whole = &text[sp.offset()..sp.offset() + sp.len()];
            let eq = whole.find('=')? + 1;
            let lead = whole[eq..].len() - whole[eq..].trim_start().len();
            let value = whole[eq + lead..].trim_end();
            Some((
                name,
                value.to_owned(),
                [sp.offset() + eq + lead, value.len()],
            ))
        })
        .collect())
}

/// Take out the node at byte `offset` (see [`path_at`]) with its children,
/// and its line when it had the line to itself.
///
/// # Errors
/// If the text doesn't parse, or no node holds the offset.
pub fn remove_node_at(text: &str, offset: usize) -> Result<String, EditError> {
    let doc = KdlDocument::parse(text).map_err(|e| EditError::Parse(e.to_string()))?;
    let sp = node_at(&doc, offset)
        .ok_or_else(|| EditError::NoNode(format!("byte {offset}")))?
        .span();
    let (mut s, mut e) = (sp.offset(), sp.offset() + sp.len());
    if text[e..].starts_with(';') {
        e += 1;
    }
    let ls = text[..s].rfind('\n').map_or(0, |n| n + 1);
    let le = text[e..].find('\n').map_or(text.len(), |n| e + n);
    let rest = text[e..le].trim();
    if text[ls..s].trim().is_empty() && (rest.is_empty() || rest.starts_with("//")) {
        // the whole line, with any comment on it
        s = ls;
        e = (le + 1).min(text.len());
    } else {
        e += text[e..le].len() - text[e..le].trim_start().len();
    }
    Ok(format!("{}{}", &text[..s], &text[e..]))
}

/// What a tile draws and names: a `line` or a `region`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    /// `line <name> { through … }`, named by `bounded-by` and `along=`.
    Line,
    /// `region <name> { … }`, named by `in=`.
    Region,
}

/// Rename the tile's line or region `old` to `new`, and every place the
/// tile names it: a line in `bounded-by` and `along=`, a region in `in=`.
/// Only those names change, each written as it was (quoted or bare);
/// everything else stays byte for byte.
///
/// # Errors
/// If the text doesn't parse or has no such line or region, or `new` is
/// not a plain name (a letter, then letters, digits, `-` and `_`), is a
/// tile edge or corner, or is taken.
pub fn rename_mark(text: &str, mark: Mark, old: &str, new: &str) -> Result<String, EditError> {
    let doc = KdlDocument::parse(text).map_err(|e| EditError::Parse(e.to_string()))?;
    let tile = doc
        .nodes()
        .iter()
        .find(|n| n.name().value() == "tile")
        .ok_or_else(|| EditError::NoNode("tile".into()))?;
    let kind = if mark == Mark::Line { "line" } else { "region" };
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
    let named = |n: &KdlNode, what: &str, name: &str| {
        n.name().value() == what
            && n.entries()
                .first()
                .is_some_and(|e| e.name().is_none() && e.value().as_string() == Some(name))
    };
    let body = tile.children().map(KdlDocument::nodes).unwrap_or_default();
    if new != old && body.iter().any(|n| named(n, kind, new)) {
        return Err(EditError::BadName(format!(
            "there is already a {kind} called `{new}`"
        )));
    }
    let def = body
        .iter()
        .find(|n| named(n, kind, old))
        .ok_or_else(|| EditError::NoNode(format!("{kind} {old}")))?;
    // the value's own bytes in an entry: after `=` for a property
    let value_span = |e: &KdlEntry| {
        let sp = e.span();
        let whole = &text[sp.offset()..sp.offset() + sp.len()];
        let from = if e.name().is_some() {
            whole.find('=').map_or(0, |i| i + 1)
        } else {
            0
        };
        let rest = &whole[from..];
        let lead = rest.len() - rest.trim_start().len();
        (sp.offset() + from + lead, rest.trim().len())
    };
    let mut spans = vec![value_span(&def.entries()[0])];
    fn walk<'a>(nodes: &'a [KdlNode], out: &mut Vec<&'a KdlNode>) {
        for n in nodes {
            out.push(n);
            if let Some(c) = n.children() {
                walk(c.nodes(), out);
            }
        }
    }
    let mut all = Vec::new();
    walk(body, &mut all);
    for n in all {
        for (i, e) in n.entries().iter().enumerate() {
            if e.value().as_string() != Some(old) {
                continue;
            }
            let refers = match (mark, e.name().map(|k| k.value())) {
                (Mark::Line, None) => n.name().value() == "bounded-by",
                (Mark::Line, Some("along")) | (Mark::Region, Some("in")) => true,
                _ => false,
            };
            // the definition's own name is already in
            if refers && !(i == 0 && std::ptr::eq(n, def)) {
                spans.push(value_span(e));
            }
        }
    }
    spans.sort_unstable();
    spans.dedup();
    let mut out = text.to_owned();
    for &(at, len) in spans.iter().rev() {
        let was = &text[at..at + len];
        let word = if was.starts_with('"') {
            format!("\"{new}\"")
        } else {
            new.to_owned()
        };
        out.replace_range(at..at + len, &word);
    }
    Ok(out)
}

#[cfg(test)]
mod place_tests {
    use super::*;

    const T: &str = "tile size=\"1x1\" {\n    line shore { through \"0,0.38\" east }\n    stack {\n        dirt in=bank // hi\n        rock at=\"0.5,0.5\"\n    }\n}\n";

    #[test]
    fn a_new_entry_goes_at_the_end_of_the_stack() {
        let (t, at) = insert_node(T, Place::Stack, "boulder at=\"0.2,0.3\"").unwrap();
        assert!(
            t.contains("        rock at=\"0.5,0.5\"\n        boulder at=\"0.2,0.3\"\n    }"),
            "{t}"
        );
        assert!(t[at..].starts_with("boulder"));
        let (t, at) =
            insert_node(T, Place::Stack, "scatter in=pool count=3 {\n    rock\n}").unwrap();
        assert!(
            t.contains("        scatter in=pool count=3 {\n            rock\n        }\n    }"),
            "{t}"
        );
        assert!(t[at..].starts_with("scatter"));
    }

    #[test]
    fn a_line_goes_before_the_stack() {
        let (t, at) = insert_node(T, Place::Tile, "line path { through west east }").unwrap();
        assert!(
            t.contains("}\n    line path { through west east }\n    stack {"),
            "{t}"
        );
        assert!(t[at..].starts_with("line path"));
    }

    #[test]
    fn a_tile_without_a_stack_gets_one() {
        let (t, at) = insert_node("tile size=\"1x1\" {\n}\n", Place::Stack, "rock").unwrap();
        assert_eq!(
            t,
            "tile size=\"1x1\" {\n    stack {\n        rock\n    }\n}\n"
        );
        assert!(t[at..].starts_with("rock"));
        let (t, at) = insert_node("tile {}", Place::Stack, "rock").unwrap();
        assert!(t[at..].starts_with("rock"), "{t} {at}");
        KdlDocument::parse(&t).unwrap();
    }

    #[test]
    fn a_node_comes_out_with_its_line() {
        let at = T.find("dirt").unwrap();
        let t = remove_node_at(T, at).unwrap();
        assert!(t.contains("    stack {\n        rock at"), "{t}");
        let at = T.find("line shore").unwrap();
        let t = remove_node_at(T, at).unwrap();
        assert!(t.starts_with("tile size=\"1x1\" {\n    stack {"), "{t}");
        let one = "tile { stack { rock; tree } }";
        assert_eq!(
            remove_node_at(one, one.find("rock").unwrap()).unwrap(),
            "tile { stack { tree } }"
        );
    }

    #[test]
    fn a_nodes_properties_and_where_their_values_are() {
        let at = T.find("rock").unwrap();
        let p = props_at(T, at).unwrap();
        assert_eq!(p[0].0, "at");
        assert_eq!(p[0].1, "\"0.5,0.5\"");
        assert_eq!(&T[p[0].2[0]..p[0].2[0] + p[0].2[1]], "\"0.5,0.5\"");
    }

    #[test]
    fn renames_a_line_or_region_and_what_names_it() {
        let t = "tile size=\"1x1\" {\n    line shore wobble=5 { through \"0,0.38\" east }\n    line trail { through south north }\n    region pool { bounded-by shore trail; fill \"0.5,0.25\" }\n    region bank { bounded-by \"shore\"; fill \"0.5,0.75\" }\n    stack {\n        dirt in=bank // the bank\n        wall along=shore\n        rock in=\"bank\" at=\"0.5,0.5\"\n    }\n}\n";
        let r = rename_mark(t, Mark::Line, "shore", "beach").unwrap();
        assert_eq!(
            r,
            t.replace("line shore", "line beach")
                .replace("by shore", "by beach")
                .replace("by \"shore\"", "by \"beach\"")
                .replace("along=shore", "along=beach")
        );
        let r = rename_mark(t, Mark::Region, "bank", "meadow").unwrap();
        assert_eq!(
            r,
            t.replace("region bank", "region meadow")
                .replace("in=bank", "in=meadow")
                .replace("in=\"bank\"", "in=\"meadow\"")
        );
        assert!(matches!(
            rename_mark(t, Mark::Line, "shore", "trail"),
            Err(EditError::BadName(_))
        ));
        assert!(matches!(
            rename_mark(t, Mark::Line, "shore", "north"),
            Err(EditError::BadName(_))
        ));
        assert!(matches!(
            rename_mark(t, Mark::Line, "shore", "2nd"),
            Err(EditError::BadName(_))
        ));
        assert!(matches!(
            rename_mark(t, Mark::Region, "nope", "x"),
            Err(EditError::NoNode(_))
        ));
    }
}
