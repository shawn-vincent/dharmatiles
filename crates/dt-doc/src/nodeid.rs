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
    /// What each definition of this document is as a generator, in the
    /// language's one kind system: a thing when its stack is exactly one
    /// unplaced thing (an element, another such definition, or a parameter
    /// that holds one), else a layer.
    pub kinds: Vec<(String, crate::units::Kind)>,
    /// What each parameter holds, as (definition, parameter, kind): a
    /// written `kind=`, else the kind of a default that names a thing or a
    /// layer, else what its use decides (a scatter's child is a thing, a
    /// stack entry a layer, which a thing fits).
    pub params: Vec<(String, String, crate::units::Kind)>,
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
    let doc = parse(text)?;
    walk(doc.nodes(), &mut out);
    kinds_in(doc.nodes(), &mut out);
    Ok(out)
}

/// What the definitions are and what the parameters hold, read from the
/// nodes as they stand (a text being typed has no decode): the rules the
/// decoder holds, in the one kind system.  A definition is a thing when its
/// stack is exactly one unplaced thing; a parameter holds what `kind=` says,
/// else what its default names, else what its use asks.
fn kinds_in(nodes: &[KdlNode], out: &mut Names) {
    use crate::units::Kind;
    let defines: Vec<&KdlNode> = nodes
        .iter()
        .filter(|n| n.name().value() == "define")
        .collect();
    let name_of = |d: &KdlNode| {
        d.entries()
            .first()
            .and_then(|e| e.value().as_string())
            .map(str::to_owned)
    };
    for d in &defines {
        if let Some(dn) = name_of(d) {
            out.kinds.push((dn, Kind::Layer));
        }
    }
    let kind_of = |out: &Names, def: &str, name: &str| -> Option<Kind> {
        crate::schema::element(name)
            .map(|e| e.generator())
            .or_else(|| {
                out.params
                    .iter()
                    .find(|(d, p, _)| d == def && p == name)
                    .map(|(_, _, k)| *k)
            })
            .or_else(|| out.kinds.iter().find(|(d, _)| d == name).map(|(_, k)| *k))
    };
    // to a fixpoint: a default names a definition whose kind is being found
    for _ in 0..=defines.len() {
        let before = out.clone();
        out.params.clear();
        for d in &defines {
            let Some(dn) = name_of(d) else { continue };
            let param_nodes: Vec<&KdlNode> = d
                .iter_children()
                .filter(|c| c.name().value() == "param")
                .collect();
            let names: Vec<String> = param_nodes
                .iter()
                .filter_map(|p| p.entries().first().and_then(|e| e.value().as_string()))
                .map(str::to_owned)
                .collect();
            // how each parameter is used in the stack
            let mut used: Vec<(String, Kind)> = Vec::new();
            fn entries(n: &KdlNode, names: &[String], used: &mut Vec<(String, Kind)>) {
                for c in n.iter_children() {
                    if names.iter().any(|p| p == c.name().value()) {
                        let kind = if n.name().value() == "scatter" {
                            Kind::Thing
                        } else {
                            Kind::Layer
                        };
                        used.push((c.name().value().to_owned(), kind));
                    }
                    entries(c, names, used);
                }
            }
            for st in d.iter_children().filter(|c| c.name().value() == "stack") {
                entries(st, &names, &mut used);
            }
            for p in &param_nodes {
                let Some(name) = p.entries().first().and_then(|e| e.value().as_string()) else {
                    continue;
                };
                let written = p
                    .entry("kind")
                    .and_then(|e| e.value().as_string())
                    .and_then(Kind::from_word)
                    .filter(|k| k.holdable());
                let default = p
                    .entries()
                    .iter()
                    .filter(|e| e.name().is_none())
                    .nth(1)
                    .and_then(|e| e.value().as_string())
                    .and_then(|v| kind_of(&before, &dn, v))
                    .filter(|k| k.is_generator());
                let used_as = used
                    .iter()
                    .filter(|(u, _)| u == name)
                    .map(|(_, k)| *k)
                    .max_by_key(|k| u8::from(*k == Kind::Thing));
                if let Some(k) = written.or(default).or(used_as) {
                    out.params.push((dn.clone(), name.to_owned(), k));
                }
            }
        }
        // a definition is a thing: a stack of one unplaced entry that is one
        for d in &defines {
            let Some(dn) = name_of(d) else { continue };
            let stack: Vec<&KdlNode> = d
                .iter_children()
                .filter(|c| c.name().value() == "stack")
                .flat_map(|s| s.iter_children())
                .filter(|c| !matches!(c.name().value(), "let" | "override"))
                .collect();
            let placed = |n: &KdlNode| n.entry("at").is_some() || n.entry("side").is_some();
            let is = matches!(stack.as_slice(), [one]
                if !placed(one) && kind_of(out, &dn, one.name().value()) == Some(Kind::Thing));
            if let Some(k) = out.kinds.iter_mut().find(|(n, _)| *n == dn) {
                k.1 = if is { Kind::Thing } else { Kind::Layer };
            }
        }
        if *out == before {
            break;
        }
    }
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

/// A written control (`name=value`, or the line `name value`) without its name: the value as written.  A control whose
/// text does not start with its (last) name is taken whole.
#[must_use]
pub fn written_value<'a>(written: &'a str, name: &str) -> &'a str {
    let last = name.rsplit('/').next().unwrap_or(name);
    let Some(rest) = written.strip_prefix(last) else {
        return written.trim();
    };
    match rest.strip_prefix('=') {
        Some(v) => v.trim(),
        None if rest.starts_with(char::is_whitespace) => rest.trim(),
        None => written.trim(),
    }
}

/// A written value as its words: the quotes of one KDL string taken off and its escapes undone.  Anything that is not
/// one string (a number, an expression, the words that say what an absent control means) is its text as written.
#[must_use]
pub fn plain_of(written: &str) -> String {
    let t = written.trim();
    if !(t.starts_with('"') || t.starts_with("#\"")) {
        return t.to_owned();
    }
    // one string argument: a document of a node with that one entry.  `written` is a slice of a document the
    // program decoded, so it always reads as KDL: a failure here is the program's mistake, said loudly, never the
    // quoted text handed back as if it were the words
    let doc = match format!("n {t}").parse::<kdl::KdlDocument>() {
        Ok(doc) => doc,
        Err(e) => panic!("a value the program decoded does not read as KDL again: {t}: {e}"),
    };
    // several entries (`"0,0" "1,0"`) or a named one are not one string: their text as written is what they say
    let one = doc.nodes().first().and_then(|node| match node.entries() {
        [e] if e.name().is_none() => e.value().as_string().map(str::to_owned),
        _ => None,
    });
    match one {
        Some(words) => words,
        None => t.to_owned(),
    }
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
