//! The name of every node of a `.dtile`: what an edit says it changes ([`crate::ops`]), instead of a byte offset
//! that is stale as soon as the text moves.
//!
//! An id is the path of steps from the top of the file, joined by `/`:
//!
//! - `tile/stack/rock#2`: the second `rock` among its siblings (the first is plain `rock`);
//! - `tile/line shore`, `tile/region pool`, `tile/point home`, `define grass/stack`: a node that names itself with
//!   its first word is that name (it keeps its id when nodes are added, taken out or moved);
//! - `tile/stack/rock id=old-oak`: a node that carries an `id=` is that id, for the same reason.
//!
//! The decoder gives every stack entry the same id as its [`crate::model::Entry::path`] (this module makes both),
//! and [`table`] lists every node with the byte it starts at, so a page that shows the text can name the node it
//! shows.  An id that names nothing in the text is a stale one: [`locate`] says so and an edit is refused.

use kdl::{KdlDocument, KdlNode};

use crate::ops::EditError;

/// Nodes that name themselves with their first word.
const NAMED: [&str; 4] = ["line", "region", "point", "define"];

/// A word an id can carry: letters, digits, `-`, `_`, `.`.
fn plain(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// The steps of a block's nodes, in order (see the module's words).
#[must_use]
pub fn steps(nodes: &[KdlNode]) -> Vec<String> {
    let mut seen: Vec<&str> = Vec::new();
    nodes
        .iter()
        .map(|n| {
            let name = n.name().value();
            let k = seen.iter().filter(|s| **s == name).count();
            seen.push(name);
            let first = n
                .entries()
                .iter()
                .find(|e| e.name().is_none())
                .and_then(|e| e.value().as_string());
            let id = n
                .entry("id")
                .and_then(|e| e.value().as_string())
                .filter(|v| plain(v));
            if let (true, Some(a)) = (NAMED.contains(&name), first.filter(|a| plain(a))) {
                format!("{name} {a}")
            } else if let Some(v) = id {
                format!("{name} id={v}")
            } else if k == 0 {
                name.to_owned()
            } else {
                format!("{name}#{}", k + 1)
            }
        })
        .collect()
}

/// One node of a document: its id, where its text starts and how long it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    /// The node's id.
    pub id: String,
    /// The node's name as written (`rock`).
    pub what: String,
    /// The node's first word when it names itself (`shore` of `line shore`).
    pub name: Option<String>,
    /// The byte the node's text starts at.
    pub at: usize,
    /// The node's length in bytes, children included.
    pub len: usize,
}

/// Parse `text` as KDL v2.
///
/// # Errors
/// [`EditError::Parse`] when it is not.
pub fn parse(text: &str) -> Result<KdlDocument, EditError> {
    KdlDocument::parse(text).map_err(|e| EditError::Parse(e.to_string()))
}

/// Every node of `doc`, outermost first, each block in order.
#[must_use]
pub fn nodes_of(doc: &KdlDocument) -> Vec<Node> {
    fn walk(nodes: &[KdlNode], prefix: &str, out: &mut Vec<Node>) {
        for (n, step) in nodes.iter().zip(steps(nodes)) {
            let id = if prefix.is_empty() {
                step.clone()
            } else {
                format!("{prefix}/{step}")
            };
            let sp = n.span();
            out.push(Node {
                id: id.clone(),
                what: n.name().value().to_owned(),
                name: step
                    .strip_prefix(&format!("{} ", n.name().value()))
                    .filter(|r| !r.starts_with("id="))
                    .map(str::to_owned),
                at: sp.offset(),
                len: sp.len(),
            });
            if let Some(c) = n.children() {
                walk(c.nodes(), &id, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(doc.nodes(), "", &mut out);
    out
}

/// The table of a text: every node's id and place (see [`nodes_of`]).
///
/// # Errors
/// [`EditError::Parse`] when the text is not KDL v2.
pub fn table(text: &str) -> Result<Vec<Node>, EditError> {
    Ok(nodes_of(&parse(text)?))
}

/// The node `id` names.
///
/// # Errors
/// [`EditError::NoNode`]: the text has no such node (the id is from other text).
pub fn locate<'d>(doc: &'d KdlDocument, id: &str) -> Result<&'d KdlNode, EditError> {
    chain(doc, id)
        .and_then(|c| c.last().copied())
        .ok_or_else(|| EditError::NoNode(id.to_owned()))
}

/// The nodes along `id`, the top one first.
fn chain<'d>(doc: &'d KdlDocument, id: &str) -> Option<Vec<&'d KdlNode>> {
    let mut nodes = doc.nodes();
    let mut out = Vec::new();
    for step in id.split('/') {
        let i = steps(nodes).iter().position(|s| s == step)?;
        out.push(&nodes[i]);
        nodes = match nodes[i].children() {
            Some(c) => c.nodes(),
            None => &[],
        };
    }
    Some(out)
}

/// The siblings of the node `id` names (its own block's nodes, in order), and where it stands among them.
///
/// # Errors
/// [`EditError::NoNode`] as [`locate`].
pub fn siblings<'d>(doc: &'d KdlDocument, id: &str) -> Result<(&'d [KdlNode], usize), EditError> {
    let c = chain(doc, id).ok_or_else(|| EditError::NoNode(id.to_owned()))?;
    let me = *c.last().ok_or_else(|| EditError::NoNode(id.to_owned()))?;
    let nodes: &[KdlNode] = match c.len() {
        1 => doc.nodes(),
        n => c[n - 2].children().map_or(&[], KdlDocument::nodes),
    };
    let i = nodes
        .iter()
        .position(|n| std::ptr::eq(n, me))
        .ok_or_else(|| EditError::NoNode(id.to_owned()))?;
    Ok((nodes, i))
}

/// The id of the node whose text starts at byte `at` in `text`.
///
/// # Errors
/// [`EditError::Parse`] when the text is not KDL v2; [`EditError::NoNode`] when no node starts there.
pub fn id_at(text: &str, at: usize) -> Result<String, EditError> {
    table(text)?
        .into_iter()
        .find(|n| n.at == at)
        .map(|n| n.id)
        .ok_or_else(|| EditError::NoNode(format!("byte {at}")))
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
    let mut out = Names::default();
    walk(parse(text)?.nodes(), &mut out);
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

/// Where a property's (or argument's) value is written: the entry's bytes, less the space around it and, for a
/// property, the `name=`.  The one reading of an entry's text the reads and the edits share.
#[must_use]
pub fn value_range(text: &str, e: &kdl::KdlEntry) -> (usize, usize) {
    let (start, end) = entry_range(text, e);
    let whole = &text[start..end];
    let from = if e.name().is_some() {
        whole.find('=').map_or(0, |i| i + 1)
    } else {
        0
    };
    let rest = &whole[from..];
    let lead = rest.len() - rest.trim_start().len();
    (start + from + lead, end)
}

/// The bytes an entry writes, without the space around it.
#[must_use]
pub fn entry_range(text: &str, e: &kdl::KdlEntry) -> (usize, usize) {
    let sp = e.span();
    let (o, l) = (sp.offset(), sp.len());
    let whole = &text[o..o + l];
    let lead = whole.len() - whole.trim_start().len();
    (o + lead, o + l - (whole.len() - whole.trim_end().len()))
}

/// The properties written on the node at byte `offset`: each name, its value as written, and the value's byte
/// span `[start, length]`.
///
/// # Errors
/// If the text doesn't parse, or no node holds the offset.
pub fn props_at(text: &str, offset: usize) -> Result<Vec<(String, String, [usize; 2])>, EditError> {
    let doc = parse(text)?;
    let node = node_at(&doc, offset).ok_or_else(|| EditError::NoNode(format!("byte {offset}")))?;
    Ok(node
        .entries()
        .iter()
        .filter_map(|e| {
            let name = e.name()?.value().to_owned();
            let (s, end) = value_range(text, e);
            Some((name, text[s..end].to_owned(), [s, end - s]))
        })
        .collect())
}
