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
//! - `"tile cliff-run"` — the first `tile` whose first argument is
//!   `cliff-run`;
//! - `"scatter#2"` — the second child called `scatter`.
//!
//! [`set_control`] is the call a GUI wants: it changes a control in
//! whichever form it is written (`height=35` on the line, or `height 35`
//! on its own line), and writes a control that isn't there yet on the
//! line, as a property (R6.2).  [`remove_control`] takes one out in
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
}

impl std::fmt::Display for EditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EditError::Parse(e) => write!(f, "not KDL v2: {e}"),
            EditError::NoNode(s) => write!(f, "no node matches `{s}`"),
            EditError::NoEntry(s) => write!(f, "the node has no `{s}`"),
            EditError::Exists(s) => write!(f, "`{s}` is already written"),
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
/// isn't written yet goes on the line as a property (R6.2).
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
        let t = "tile a {\n    stack {\n        dirt\n        scatter count=2 { rock size=\"6..10\" }\n        scatter\n    }\n}\n";
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
