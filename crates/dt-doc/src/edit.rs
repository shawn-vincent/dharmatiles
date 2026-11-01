//! Format-preserving edits through the kdl crate's document API.
//!
//! This is how the GUI, the MCP server and agents change one control: the
//! file is parsed, one entry's value is replaced, and the document is
//! written back.  Comments, blank lines, alignment and every other entry
//! stay byte for byte as they were.

use kdl::{KdlDocument, KdlNode, KdlValue};

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
    /// The node has no such entry.
    NoEntry(String),
}

impl std::fmt::Display for EditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EditError::Parse(e) => write!(f, "not KDL v2: {e}"),
            EditError::NoNode(s) => write!(f, "no node matches `{s}`"),
            EditError::NoEntry(s) => write!(f, "the node has no `{s}`"),
        }
    }
}

impl std::error::Error for EditError {}

/// A path step matches a node by name, and optionally by its first
/// argument: `"stack"`, `"define long-grass"`, `"tile cliff-run"`.
fn matches(node: &KdlNode, step: &str) -> bool {
    let (name, arg) = match step.split_once(' ') {
        Some((n, a)) => (n, Some(a)),
        None => (step, None),
    };
    node.name().value() == name
        && arg.is_none_or(|a| {
            node.entries()
                .first()
                .is_some_and(|e| e.name().is_none() && e.value().as_string() == Some(a))
        })
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
    let mut doc = KdlDocument::parse(text).map_err(|e| EditError::Parse(e.to_string()))?;
    let mut nodes = doc.nodes_mut();
    let mut target: Option<&mut KdlNode> = None;
    for (i, step) in path.iter().enumerate() {
        let node = nodes
            .iter_mut()
            .find(|n| matches(n, step))
            .ok_or_else(|| EditError::NoNode(path[..=i].join(" > ")))?;
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
    let value = value.into();
    let repr = value.to_string();
    entry.set_value(value);
    // The entry prints its original text unless told otherwise; only the
    // value's own text changes, never its surroundings.
    if let Some(f) = entry.format_mut() {
        f.value_repr = repr;
    }
    Ok(doc.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = "// top\nthing a=1   b=\"x\" { // note\n    kid 4.5 // keep\n}\n";

    #[test]
    fn edits_a_property_in_place() {
        let out = set_value(SRC, &["thing"], Key::Prop("a"), 2).unwrap();
        assert_eq!(out, SRC.replace("a=1", "a=2"));
    }

    #[test]
    fn edits_a_child_argument_in_place() {
        let out = set_value(SRC, &["thing", "kid"], Key::Arg(0), 5.25).unwrap();
        assert_eq!(out, SRC.replace("kid 4.5", "kid 5.25"));
    }

    #[test]
    fn strings_come_out_quoted() {
        let out = set_value(SRC, &["thing"], Key::Prop("b"), "9..15").unwrap();
        assert_eq!(out, SRC.replace("b=\"x\"", "b=\"9..15\""));
    }

    #[test]
    fn missing_things_are_errors() {
        assert!(matches!(
            set_value(SRC, &["nope"], Key::Arg(0), 1),
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
