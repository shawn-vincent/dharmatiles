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
#[derive(Debug, Clone)]
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
    /// The edit cannot be made on text written this way, and what to change.
    Cannot(String),
    /// No piece of a print list has this id.
    NoPiece(String),
    /// The text does not read as the document it should be: its own mistakes,
    /// so nothing is ever edited in a file that is not understood.
    Unread(Vec<crate::error::DocError>),
}

impl std::fmt::Display for EditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EditError::Parse(e) => write!(f, "not KDL v2: {e}"),
            EditError::NoNode(s) => write!(f, "no node matches `{s}`"),
            EditError::NoEntry(s) => write!(f, "the node has no `{s}`"),
            EditError::Exists(s) => write!(f, "`{s}` is already written"),
            EditError::BadName(s) | EditError::Cannot(s) => f.write_str(s),
            EditError::NoPiece(id) => write!(f, "the list has no piece {id}"),
            EditError::Unread(e) => f.write_str(
                e.first()
                    .map_or("the document does not read", |d| d.message.as_str()),
            ),
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
    let value = value.into();
    // a sub-control (`stones/height`) is a property of its group's line: the
    // group's child line is made when it is not written yet
    if let Some((group, sub)) = name.split_once('/') {
        let doc = KdlDocument::parse(text).map_err(|e| EditError::Parse(e.to_string()))?;
        let parent =
            node_at(&doc, offset).ok_or_else(|| EditError::NoNode(format!("byte {offset}")))?;
        return match group_line(parent, group) {
            Some(at) => set_control_at(text, at, sub, value),
            None => insert_child_at(text, offset, &format!("{group} {sub}={value}"))
                .map(|(text, _)| text),
        };
    }
    let path = path_at(text, offset)?;
    let steps: Vec<&str> = path.iter().map(String::as_str).collect();
    set_control(text, &steps, name, value)
}

/// The byte where the group's child line of `parent` starts its name.
fn group_line(parent: &KdlNode, group: &str) -> Option<usize> {
    parent
        .children()?
        .nodes()
        .iter()
        .find(|c| c.name().value() == group)
        .map(|c| c.name().span().offset())
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
    // a sub-control (`stones/height`) comes off its group's line, and the
    // line goes with the last of its properties
    if let Some((group, sub)) = name.split_once('/') {
        let doc = KdlDocument::parse(text).map_err(|e| EditError::Parse(e.to_string()))?;
        let parent =
            node_at(&doc, offset).ok_or_else(|| EditError::NoNode(format!("byte {offset}")))?;
        let at = group_line(parent, group).ok_or_else(|| EditError::NoEntry(name.to_owned()))?;
        let out = remove_control_at(text, at, sub)?;
        let left = KdlDocument::parse(&out).map_err(|e| EditError::Parse(e.to_string()))?;
        let empty = node_at(&left, at).is_some_and(|n| {
            n.name().value() == group && n.entries().is_empty() && n.children().is_none()
        });
        return if empty {
            remove_node_at(&out, at)
        } else {
            Ok(out)
        };
    }
    let path = path_at(text, offset)?;
    let steps: Vec<&str> = path.iter().map(String::as_str).collect();
    remove_control(text, &steps, name)
}

#[cfg(test)]
mod tests {
    #[test]
    fn names_come_from_nodes_not_from_text() {
        let text = "// define fake\ndefine meadow { stack { dirt } }\nuse \"lib:parts/trees\" oak elm\n\
            tile size=\"1x1\" {\n about \"a line Foo\"\n line shore { through \"0,0\" }\n\
            /- region ghost { fill \"1,1\" }\n region pool { fill \"1,1\" }\n point p \"1,1\"\n}\n";
        let n = super::names_in(text).unwrap();
        assert_eq!(n.defs, ["meadow", "oak", "elm"]);
        assert_eq!(n.lines, ["shore"]);
        assert_eq!(n.regions, ["pool"]);
        assert_eq!(n.points, ["p"]);
        assert!(super::names_in("define {").is_err());
    }

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
        assert!(matches!(
            insert_property(SRC, &["thing"], "a", 3),
            Err(EditError::Exists(n)) if n == "a"
        ));
        assert!(matches!(
            insert_property(SRC, &["thing"], "kid", 3),
            Err(EditError::Exists(n)) if n == "kid"
        ));
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
    /// Before the node whose name starts at this byte, among its siblings:
    /// a stack entry written between two others.
    Before(usize),
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

/// Write `node` as a line of its own above the node starting at byte `at`
/// (above the `//` comment lines that sit on top of it, which belong to
/// it), at its indent; or, when it shares its line, in front of it as
/// `node; `.  Returns the text and the byte the new node starts at.
fn insert_above(text: &str, at: usize, node: &str) -> (String, usize) {
    let splice = |at: usize, s: &str| format!("{}{s}{}", &text[..at], &text[at..]);
    let ls = text[..at].rfind('\n').map_or(0, |n| n + 1);
    if text[ls..at].trim().is_empty() {
        let ind = &text[ls..at];
        let top = lead_start(text, at);
        return (
            splice(top, &format!("{ind}{}\n", reindent(node, ind))),
            top + ind.len(),
        );
    }
    (splice(at, &format!("{node}; ")), at)
}

/// The start of the line holding byte `at`, moved up over the `//` comment
/// lines directly above it: the comments that belong to the node there.
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

/// The sibling nodes, in order, one of which starts at byte `at`.
fn siblings_at(nodes: &[KdlNode], at: usize) -> Option<&[KdlNode]> {
    if nodes.iter().any(|n| n.span().offset() == at) {
        return Some(nodes);
    }
    nodes
        .iter()
        .filter_map(KdlNode::children)
        .find_map(|c| siblings_at(c.nodes(), at))
}

/// The lines a node takes up, as a byte range: from the comment lines
/// above it to the end of its own last line.
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

/// Move the node starting at byte `from` to stand before its sibling
/// starting at byte `before`, or after its last sibling when `before` is
/// `None`.  The node goes whole: its children, the comment lines directly
/// above it and a comment after it on its line.  Blank lines stay where
/// they are, except that the gap it leaves never keeps one on both sides.
/// Returns the new text and the byte the moved node starts at; the text is
/// unchanged when the node is already there.
///
/// # Errors
/// If the text doesn't parse, `from` or `before` starts no node, they are
/// not siblings, or a node shares its line with another.
pub fn move_node(
    text: &str,
    from: usize,
    before: Option<usize>,
) -> Result<(String, usize), EditError> {
    let doc = KdlDocument::parse(text).map_err(|e| EditError::Parse(e.to_string()))?;
    let sibs =
        siblings_at(doc.nodes(), from).ok_or_else(|| EditError::NoNode(format!("byte {from}")))?;
    let pos = |at: usize| sibs.iter().position(|n| n.span().offset() == at);
    let i = pos(from).ok_or_else(|| EditError::NoNode(format!("byte {from}")))?;
    let j = match before {
        Some(b) => pos(b).ok_or_else(|| {
            EditError::Cannot("a node moves among its own siblings: the target is not one".into())
        })?,
        None => sibs.len(),
    };
    // already there: it stays where it is, byte for byte
    if j == i || j == i + 1 {
        return Ok((text.to_owned(), from));
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
    Ok(if p < bs {
        (
            format!("{}{block}{}{}", &text[..p], &text[p..bs], &text[rm_end..]),
            p + inside,
        )
    } else {
        let p = p.max(rm_end);
        (
            format!("{}{}{block}{}", &text[..bs], &text[rm_end..p], &text[p..]),
            p - (rm_end - bs) + inside,
        )
    })
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
        (Place::Before(at), _) => {
            siblings_at(doc.nodes(), at).ok_or_else(|| EditError::NoNode(format!("byte {at}")))?;
            insert_above(text, at, node)
        }
    })
}

/// The names a document makes, for completion and the map: the definitions
/// (written, or taken in with `use "file" name…`), the regions, the lines and
/// the points, wherever a node is written.  A comment, a hidden (`/-`) node
/// and a string hold none: these are the parser's nodes.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Names {
    /// `define NAME`, and the names a `use` brings in.
    pub defs: Vec<String>,
    /// `region NAME`.
    pub regions: Vec<String>,
    /// `line NAME`.
    pub lines: Vec<String>,
    /// `point NAME`.
    pub points: Vec<String>,
}

/// The names `text` makes (see [`Names`]).
///
/// # Errors
/// If the text doesn't parse (a document being typed, mid-word).
pub fn names_in(text: &str) -> Result<Names, EditError> {
    fn walk(nodes: &[KdlNode], out: &mut Names) {
        for n in nodes {
            let arg = |i: usize| {
                n.entries()
                    .iter()
                    .filter(|e| e.name().is_none())
                    .nth(i)
                    .and_then(|e| e.value().as_string())
            };
            match n.name().value() {
                "define" => out.defs.extend(arg(0).map(str::to_owned)),
                "use" => out.defs.extend(
                    n.entries()
                        .iter()
                        .filter(|e| e.name().is_none())
                        .skip(1)
                        .filter_map(|e| e.value().as_string().map(str::to_owned)),
                ),
                "region" => out.regions.extend(arg(0).map(str::to_owned)),
                "line" => out.lines.extend(arg(0).map(str::to_owned)),
                "point" => out.points.extend(arg(0).map(str::to_owned)),
                _ => {}
            }
            if let Some(c) = n.children() {
                walk(c.nodes(), out);
            }
        }
    }
    let doc = KdlDocument::parse(text).map_err(|e| EditError::Parse(e.to_string()))?;
    let mut out = Names::default();
    walk(doc.nodes(), &mut out);
    Ok(out)
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

/// Write a new node (one or more lines of KDL, unindented) as the last child
/// of the node whose name starts at byte `offset` (see [`path_at`]): a clause
/// in a region.  Everything else stays byte for byte.  Returns the new text
/// and the byte the new node's name starts at.
///
/// # Errors
/// If the text doesn't parse, no node holds the offset, or `node` is not KDL.
pub fn insert_child_at(
    text: &str,
    offset: usize,
    node: &str,
) -> Result<(String, usize), EditError> {
    let node = node.trim();
    KdlDocument::parse(node).map_err(|e| EditError::Parse(e.to_string()))?;
    let doc = KdlDocument::parse(text).map_err(|e| EditError::Parse(e.to_string()))?;
    let parent =
        node_at(&doc, offset).ok_or_else(|| EditError::NoNode(format!("byte {offset}")))?;
    Ok(add_child(text, parent, None, node))
}

/// Take out every child named one of `names` of the node whose name starts
/// at byte `offset` (see [`path_at`]), each with its line when it had the
/// line to itself.  A region's `boundedBy` and its `fill` points go together.
///
/// # Errors
/// If the text doesn't parse or no node holds the offset.
pub fn remove_children_at(text: &str, offset: usize, names: &[&str]) -> Result<String, EditError> {
    let doc = KdlDocument::parse(text).map_err(|e| EditError::Parse(e.to_string()))?;
    let parent =
        node_at(&doc, offset).ok_or_else(|| EditError::NoNode(format!("byte {offset}")))?;
    let mut at: Vec<usize> = parent
        .children()
        .map(|c| c.nodes())
        .unwrap_or_default()
        .iter()
        .filter(|n| names.contains(&n.name().value()))
        .map(|n| n.name().span().offset())
        .collect();
    // last first, so the earlier offsets stay where they are
    at.sort_unstable_by(|a, b| b.cmp(a));
    let mut out = text.to_owned();
    for a in at {
        out = remove_node_at(&out, a)?;
    }
    Ok(out)
}

/// What a tile draws and names: a `line` or a `region`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    /// `line <name> { through … }`, named by `boundedBy` and `at=`.
    Line,
    /// `region <name> { … }`, named by `at=`.
    Region,
}

/// Rename the tile's line or region `old` to `new`, and every place the
/// tile names it: a line in `boundedBy` and `at=`, a region in `at=`.
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
                (Mark::Line, None) => n.name().value() == "boundedBy",
                (_, Some("at")) => true,
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

/// What [`apply_fixes`] did.
#[derive(Debug, Clone, PartialEq)]
pub struct Mended {
    /// The text with the fixes laid in.
    pub text: String,
    /// The fixes that were made, in the order of the text.
    pub applied: Vec<crate::error::Fix>,
    /// How many were left for another round: fixes whose text overlaps one made (the next check says them again if
    /// they still hold).
    pub deferred: usize,
}

/// Lay the program's fixes into `text`, the text they were made for: each writes its replacement over its bytes, and
/// only where the text still reads what the fix saw (`was`).  The one place a fix is applied in Rust; the editor's
/// page lays the same fix in the same way.  A fix that overlaps one already made waits (`deferred`), and the same
/// fix twice is made once.
///
/// # Errors
/// A fix whose place no longer reads what it was made for, or lies outside the text.
pub fn apply_fixes(text: &str, fixes: &[crate::error::Fix]) -> Result<Mended, EditError> {
    let mut order: Vec<&crate::error::Fix> = fixes.iter().collect();
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
    let mut out = text.to_owned();
    for f in applied.iter().rev() {
        out.replace_range(f.offset..f.offset + f.len, &f.replacement);
    }
    Ok(Mended {
        text: out,
        applied,
        deferred,
    })
}

#[cfg(test)]
mod fix_tests {
    use super::*;
    use crate::error::Fix;

    fn fix(text: &str, was: &str, with: &str) -> Fix {
        Fix {
            offset: text.find(was).unwrap(),
            len: was.len(),
            was: was.into(),
            replacement: with.into(),
            label: format!("Change to {with}"),
            moved: None,
        }
    }

    #[test]
    fn fixes_are_laid_in_from_the_end_and_only_where_the_text_matches() {
        let t = "a bb ccc";
        let m = apply_fixes(t, &[fix(t, "ccc", "z"), fix(t, "bb", "yyyy")]).unwrap();
        assert_eq!(m.text, "a yyyy z");
        assert_eq!(m.applied.len(), 2);
        let stale = Fix {
            offset: 2,
            len: 2,
            was: "xx".into(),
            ..fix(t, "bb", "q")
        };
        assert!(apply_fixes(t, &[stale]).is_err());
    }

    #[test]
    fn overlapping_fixes_wait_and_a_repeat_is_made_once() {
        let t = "abcdef";
        let m = apply_fixes(
            t,
            &[fix(t, "abc", "1"), fix(t, "abc", "1"), fix(t, "bcd", "2")],
        )
        .unwrap();
        assert_eq!((m.text.as_str(), m.deferred), ("1def", 1));
    }
}

#[cfg(test)]
mod place_tests {
    use super::*;

    const T: &str = "tile size=\"1x1\" {\n    line shore { through \"0,0.38\" east }\n    stack {\n        dirt at=bank // hi\n        rock at=\"0.5,0.5\"\n    }\n}\n";

    #[test]
    fn a_new_entry_goes_at_the_end_of_the_stack() {
        let (t, at) = insert_node(T, Place::Stack, "boulder at=\"0.2,0.3\"").unwrap();
        assert!(
            t.contains("        rock at=\"0.5,0.5\"\n        boulder at=\"0.2,0.3\"\n    }"),
            "{t}"
        );
        assert!(t[at..].starts_with("boulder"));
        let (t, at) =
            insert_node(T, Place::Stack, "scatter at=pool count=3 {\n    rock\n}").unwrap();
        assert!(
            t.contains("        scatter at=pool count=3 {\n            rock\n        }\n    }"),
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
        let t = "tile size=\"1x1\" {\n    line shore wobble=5 { through \"0,0.38\" east }\n    line trail { through south north }\n    region pool { boundedBy shore trail; fill \"0.5,0.25\" }\n    region bank { boundedBy \"shore\"; fill \"0.5,0.75\" }\n    stack {\n        dirt at=bank // the bank\n        wall at=shore\n        rock at=\"bank\"\n    }\n}\n";
        let r = rename_mark(t, Mark::Line, "shore", "beach").unwrap();
        assert_eq!(
            r,
            t.replace("line shore", "line beach")
                .replace("By shore", "By beach")
                .replace("By \"shore\"", "By \"beach\"")
                .replace("at=shore", "at=beach")
        );
        let r = rename_mark(t, Mark::Region, "bank", "meadow").unwrap();
        assert_eq!(
            r,
            t.replace("region bank", "region meadow")
                .replace("at=bank", "at=meadow")
                .replace("at=\"bank\"", "at=\"meadow\"")
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

#[cfg(test)]
mod clause_tests {
    use super::*;

    const REGION: &str = "tile size=\"1x1\" {\n    region pool {\n        boundedBy shore\n        fill \"0.5,0.2\"\n        within 6 of=center\n    }\n}\n";

    #[test]
    fn a_clause_is_written_last_in_its_region() {
        let at = REGION.find("pool").unwrap();
        let (t, node) = insert_child_at(REGION, at, "except crag").unwrap();
        assert!(
            t.contains("        within 6 of=center\n        except crag\n    }"),
            "{t}"
        );
        assert!(t[node..].starts_with("except crag"));
    }

    #[test]
    fn a_clause_goes_into_a_region_on_one_line() {
        let src = "tile size=\"1x1\" {\n    region a { within 4 of=center }\n}\n";
        let at = src.find("region a").unwrap() + 7;
        let (t, _) = insert_child_at(src, at, "except b").unwrap();
        assert!(KdlDocument::parse(&t).is_ok(), "{t}");
        assert!(t.contains("except b"));
    }

    #[test]
    fn the_bucket_comes_out_together() {
        let at = REGION.find("pool").unwrap();
        let t = remove_children_at(REGION, at, &["boundedBy", "fill"]).unwrap();
        assert_eq!(
            t,
            "tile size=\"1x1\" {\n    region pool {\n        within 6 of=center\n    }\n}\n"
        );
    }

    #[test]
    fn the_header_line_is_left_exactly_as_written() {
        const T: &str = "tile size=\"1x1\" {\n    line shore { through \"0,0.38\" east }\n    stack {\n        dirt at=bank // hi\n        rock at=\"0.5,0.5\"\n    }\n}\n";
        let head = "dharmatiles   version=\"0.9.0+5.gf9481c8.dirty\" // as written\n";
        let text = format!("{head}{T}");
        let (t, _) = insert_node(&text, Place::Stack, "boulder at=\"0.2,0.3\"").unwrap();
        assert!(t.starts_with(head), "{t}");
        let (t, at) = insert_node(&text, Place::Tile, "line path { through west east }").unwrap();
        assert!(t.starts_with(head), "{t}");
        let t = remove_node_at(&t, at).unwrap();
        assert_eq!(t, text);
        let at = text.find("rock").unwrap();
        let t = remove_node_at(&text, at).unwrap();
        assert!(t.starts_with(head), "{t}");
        let t = rename_mark(&text, Mark::Line, "shore", "bank").unwrap();
        assert!(t.starts_with(head), "{t}");
    }
}

#[cfg(test)]
mod move_tests {
    use super::*;

    const S: &str = "tile size=\"1x1\" {\n    stack {\n        base\n        // the bank\n        dirt at=bank // soil\n\n        // stones\n        rock at=\"0.5,0.5\"\n        scatter count=3 {\n            pebble\n        }\n        grass\n    }\n}\n";

    fn at(text: &str, word: &str) -> usize {
        text.find(word).unwrap()
    }

    #[test]
    fn a_node_moves_down_with_its_comments_and_children() {
        let (t, to) = move_node(S, at(S, "dirt"), Some(at(S, "scatter"))).unwrap();
        assert_eq!(
            t,
            "tile size=\"1x1\" {\n    stack {\n        base\n\n        // stones\n        rock at=\"0.5,0.5\"\n        // the bank\n        dirt at=bank // soil\n        scatter count=3 {\n            pebble\n        }\n        grass\n    }\n}\n"
        );
        assert!(t[to..].starts_with("dirt at=bank"));
        KdlDocument::parse(&t).unwrap();
    }

    #[test]
    fn a_node_with_children_moves_up_and_to_the_end() {
        let (t, to) = move_node(S, at(S, "scatter"), Some(at(S, "dirt"))).unwrap();
        assert!(
            t.contains(
                "base\n        scatter count=3 {\n            pebble\n        }\n        // the bank\n        dirt at=bank // soil\n"
            ),
            "{t}"
        );
        assert!(t[to..].starts_with("scatter count=3 {"));
        let (t, to) = move_node(S, at(S, "rock"), None).unwrap();
        assert!(
            t.ends_with(
                "        grass\n        // stones\n        rock at=\"0.5,0.5\"\n    }\n}\n"
            ),
            "{t}"
        );
        assert!(t[to..].starts_with("rock at"));
        KdlDocument::parse(&t).unwrap();
    }

    #[test]
    fn a_blank_line_is_not_doubled_where_the_node_left() {
        let t = "tile {\n    stack {\n        base\n\n        dirt\n\n        rock\n        grass\n    }\n}\n";
        let (m, _) = move_node(t, at(t, "dirt"), None).unwrap();
        assert_eq!(
            m,
            "tile {\n    stack {\n        base\n\n        rock\n        grass\n        dirt\n    }\n}\n"
        );
    }

    #[test]
    fn a_node_already_in_place_changes_nothing() {
        let (t, to) = move_node(S, at(S, "rock"), Some(at(S, "scatter"))).unwrap();
        assert_eq!(t, S);
        assert_eq!(to, at(S, "rock"));
        let (t, _) = move_node(S, at(S, "grass"), None).unwrap();
        assert_eq!(t, S);
        let (t, _) = move_node(S, at(S, "rock"), Some(at(S, "rock"))).unwrap();
        assert_eq!(t, S);
    }

    #[test]
    fn a_hidden_entry_stays_where_it_is_and_hides_only_itself() {
        let t = "tile {\n    stack {\n        dirt\n        /- grass\n        rock\n        tree\n    }\n}\n";
        let (m, to) = move_node(t, at(t, "dirt"), Some(at(t, "tree"))).unwrap();
        assert_eq!(
            m,
            "tile {\n    stack {\n        /- grass\n        rock\n        dirt\n        tree\n    }\n}\n"
        );
        assert!(m[to..].starts_with("dirt"));
        let (m, _) = move_node(t, at(t, "tree"), Some(at(t, "rock"))).unwrap();
        assert_eq!(
            m,
            "tile {\n    stack {\n        dirt\n        /- grass\n        tree\n        rock\n    }\n}\n"
        );
    }

    #[test]
    fn what_cannot_be_moved_is_refused_in_words() {
        let one = "tile { stack { rock; tree } }";
        assert!(matches!(
            move_node(one, at(one, "rock"), None),
            Err(EditError::Cannot(_))
        ));
        // a target that is not a sibling
        assert!(matches!(
            move_node(S, at(S, "dirt"), Some(at(S, "pebble"))),
            Err(EditError::Cannot(_))
        ));
        assert!(matches!(move_node(S, 3, None), Err(EditError::NoNode(_))));
        assert!(matches!(
            move_node("tile {", 0, None),
            Err(EditError::Parse(_))
        ));
    }

    #[test]
    fn a_new_entry_goes_between_two_others_above_the_comments_on_the_second() {
        let (t, at_new) = insert_node(S, Place::Before(at(S, "rock")), "boulder").unwrap();
        assert!(
            t.contains("dirt at=bank // soil\n\n        boulder\n        // stones\n        rock"),
            "{t}"
        );
        assert!(t[at_new..].starts_with("boulder"));
        KdlDocument::parse(&t).unwrap();
        assert!(matches!(
            insert_node(S, Place::Before(3), "boulder"),
            Err(EditError::NoNode(_))
        ));
    }
    #[test]
    fn a_sub_control_is_a_property_of_its_groups_line() {
        let t = "tile {\n    stack {\n        wall at=north {\n            height full\n        }\n    }\n}\n";
        let wall = t.find("wall").unwrap();
        // no group line yet: it is made, as a child line
        let a = set_control_at(t, wall, "stones/height", "3dmm").unwrap();
        assert!(a.contains("            stones height=\"3dmm\"\n"), "{a}");
        // a group line already written: the property goes on it
        let b = set_control_at(&a, wall, "stones/length", "9dmm").unwrap();
        assert!(b.contains("stones height=\"3dmm\" length=\"9dmm\""), "{b}");
        // and changes in place
        let c = set_control_at(&b, wall, "stones/height", "4dmm").unwrap();
        assert!(c.contains("stones height=\"4dmm\" length=\"9dmm\""), "{c}");
        // taken out, the line stays while it holds another, and goes with the last
        let d = remove_control_at(&c, wall, "stones/height").unwrap();
        assert!(
            d.contains("stones length=\"9dmm\"") && !d.contains("height=\"4"),
            "{d}"
        );
        let e = remove_control_at(&d, wall, "stones/length").unwrap();
        assert!(!e.contains("stones"), "{e}");
        assert!(e.contains("height full"), "{e}");
        KdlDocument::parse(&e).unwrap();
        assert!(matches!(
            remove_control_at(t, wall, "stones/height"),
            Err(EditError::NoEntry(_))
        ));
    }
}
