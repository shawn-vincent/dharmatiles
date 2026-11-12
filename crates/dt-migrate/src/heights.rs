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

use dt_doc::edit::EditError;

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
    let mut now = text.to_owned();
    let mut changed = false;
    let mut step = |f: fn(&str) -> Result<Option<String>, EditError>| -> Result<(), EditError> {
        if let Some(next) = f(&now)? {
            now = next;
            changed = true;
        }
        Ok(())
    };
    step(control_names)?;
    step(mostly)?;
    step(floors)?;
    step(cliffs)?;
    step(heights_only)?;
    step(places)?;
    step(tree_size)?;
    step(tree_top)?;
    step(flowers)?;
    step(dirt_stones)?;
    Ok(changed.then_some(now))
}

/// One word names a place: `in=bank` and `along=trail` are `at=bank` and
/// `at=trail`, and `against=cliff` is `at="foot(cliff)"`.  An entry that
/// writes two places (`wall in=south along=north`, or an old word beside an
/// `at=`) is left as written: it has no one-word form, and the decoder says
/// what to do.  Every other byte is as written.  `Ok(None)` when nothing
/// changes.
///
/// # Errors
/// If the text is not KDL.
pub fn places(text: &str) -> Result<Option<String>, EditError> {
    fn walk(nodes: &[KdlNode], edits: &mut Vec<Edit>) {
        for n in nodes {
            let skipped = matches!(
                n.name().value(),
                "line" | "region" | "point" | "let" | "override" | "use" | "param" | "pin"
            );
            if !skipped {
                let old: Vec<_> = n
                    .entries()
                    .iter()
                    .filter(|e| {
                        e.name()
                            .is_some_and(|k| matches!(k.value(), "in" | "along" | "against"))
                    })
                    .collect();
                let places = n
                    .entries()
                    .iter()
                    .filter(|e| {
                        e.name()
                            .is_some_and(|k| matches!(k.value(), "in" | "along" | "against" | "at"))
                    })
                    .count();
                if old.len() == 1 && places == 1 {
                    let e = old[0];
                    if let Some(k) = e.name() {
                        let (start, end) = (k.span().offset(), k.span().offset() + k.span().len());
                        if k.value() == "against" {
                            if let Some(kind) = e.value().as_string() {
                                edits.push(Edit {
                                    start,
                                    end: e.span().offset() + e.span().len(),
                                    with: format!("at=\"foot({kind})\""),
                                });
                            }
                        } else {
                            edits.push(Edit {
                                start,
                                end,
                                with: "at".to_owned(),
                            });
                        }
                    }
                }
            }
            if let Some(kids) = n.children() {
                walk(kids.nodes(), edits);
            }
        }
    }
    let doc = KdlDocument::parse(text).map_err(|e| EditError::Parse(e.to_string()))?;
    let mut edits = Vec::new();
    walk(doc.nodes(), &mut edits);
    if edits.is_empty() {
        return Ok(None);
    }
    edits.sort_by_key(|e| std::cmp::Reverse(e.start));
    let mut out = text.to_owned();
    for e in edits {
        out.replace_range(e.start..e.end, &e.with);
    }
    Ok(Some(out))
}

/// The rock family: the things `mostly=` skewed.
const ROCKS: [&str; 4] = ["rock", "stone", "boulder", "pebble"];

/// `mostly=` is gone: its word becomes a weighting written on the size.
/// `size="1..6" mostly=small` is `size="low(1..6, 11)"`, `large` is
/// `high(1..6)` and `even` the plain range.  A `mostly` on a scatter was never
/// built: it goes to each child thing that has none of its own.  A thing
/// that writes `mostly` and no size gets its element's default size.  Every
/// other byte is as written.  `Ok(None)` when no `mostly` is written.
///
/// # Errors
/// If the text is not KDL.
pub fn mostly(text: &str) -> Result<Option<String>, EditError> {
    /// The word a node writes, as a property or as a child line.
    fn word(n: &KdlNode) -> Option<String> {
        if let Some(e) = n.entry("mostly") {
            return num_text(e.value());
        }
        let c = n
            .children()?
            .nodes()
            .iter()
            .find(|c| c.name().value() == "mostly")?;
        num_text(c.entries().first()?.value())
    }

    /// Delete the `mostly` a node writes.
    fn remove(text: &str, n: &KdlNode, edits: &mut Vec<Edit>) {
        if let Some(e) = n.entry("mostly") {
            let s = e.span().offset();
            let start = if text[..s].ends_with(' ') { s - 1 } else { s };
            edits.push(Edit {
                start,
                end: s + e.span().len(),
                with: String::new(),
            });
        }
        if let Some(kids) = n.children() {
            for c in kids.nodes().iter().filter(|c| c.name().value() == "mostly") {
                let (co, cl) = (c.span().offset(), c.span().len());
                let (ls, le) = (line_start(text, co), line_end(text, co + cl));
                let alone = text[ls..co].trim().is_empty() && text[co + cl..le].trim().is_empty();
                let (start, end) = if alone { (ls, le) } else { (co, co + cl) };
                edits.push(Edit {
                    start,
                    end,
                    with: String::new(),
                });
            }
        }
    }

    /// Weight the size of one thing by `word`.
    fn weigh(n: &KdlNode, word: &str, edits: &mut Vec<Edit>) {
        let make = |range: &str| -> Option<String> {
            match word {
                "small" => Some(format!("low({range}, 11)")),
                "large" => Some(format!("high({range})")),
                _ => None,
            }
        };
        // where the size is written: a property, or a child line
        let child = n
            .children()
            .and_then(|k| k.nodes().iter().find(|c| c.name().value() == "size"));
        let written = n
            .entry("size")
            .map(|e| (e.span().offset(), e.span().len(), e.value(), "size="))
            .or_else(|| {
                let c = child?;
                let e = c.entries().first()?;
                Some((e.span().offset(), e.span().len(), e.value(), ""))
            });
        match written {
            Some((off, len, v, key)) => {
                let Some(range) = v.as_string().map(str::trim) else {
                    return; // one number: nothing to weigh
                };
                if !range.contains("..") {
                    return;
                }
                if let Some(new) = make(range) {
                    edits.push(Edit {
                        start: off,
                        end: off + len,
                        with: format!("{key}\"{new}\""),
                    });
                }
            }
            None => {
                let Some(default) = dt_doc::schema::element(n.name().value())
                    .and_then(|l| l.controls.iter().find(|c| c.name == "size"))
                    .map(|c| c.default)
                else {
                    return;
                };
                let Some(new) = make(default) else { return };
                // after the name, on its line
                let at = n.name().span().offset() + n.name().span().len();
                edits.push(Edit {
                    start: at,
                    end: at,
                    with: format!(" size=\"{new}\""),
                });
            }
        }
    }

    fn walk(text: &str, nodes: &[KdlNode], edits: &mut Vec<Edit>) {
        for n in nodes {
            let name = n.name().value();
            if ROCKS.contains(&name) {
                if let Some(w) = word(n) {
                    weigh(n, &w, edits);
                    remove(text, n, edits);
                }
            } else if name == "scatter"
                && let Some(w) = word(n)
            {
                if let Some(kids) = n.children() {
                    for c in kids
                        .nodes()
                        .iter()
                        .filter(|c| ROCKS.contains(&c.name().value()))
                    {
                        if word(c).is_none() {
                            weigh(c, &w, edits);
                        }
                    }
                }
                remove(text, n, edits);
            }
            if let Some(kids) = n.children() {
                walk(text, kids.nodes(), edits);
            }
        }
    }

    let doc = KdlDocument::parse(text).map_err(|e| EditError::Parse(e.to_string()))?;
    let mut edits = Vec::new();
    walk(text, doc.nodes(), &mut edits);
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

/// A tree's `height` is its `size`, the one word a thing's measure goes by:
/// `tree height=40` is `tree size=40` (the drawn range and its weighting
/// are the same, so the build is too), on the line or as a control's own
/// line.  The `trunk` group's own `height` stays.  Every other byte is as
/// written.  `Ok(None)` when no tree writes a `height`.
///
/// # Errors
/// If the text is not KDL.
pub fn tree_size(text: &str) -> Result<Option<String>, EditError> {
    fn walk(nodes: &[KdlNode], edits: &mut Vec<Edit>) {
        for n in nodes {
            if n.name().value() == "tree" {
                let keys = n
                    .entries()
                    .iter()
                    .filter_map(|e| e.name())
                    .chain(
                        n.children()
                            .into_iter()
                            .flat_map(|k| k.nodes().iter().map(KdlNode::name)),
                    )
                    .filter(|k| k.value() == "height");
                for k in keys {
                    edits.push(Edit {
                        start: k.span().offset(),
                        end: k.span().offset() + k.span().len(),
                        with: "size".to_owned(),
                    });
                }
            }
            if let Some(kids) = n.children() {
                walk(kids.nodes(), edits);
            }
        }
    }
    let doc = KdlDocument::parse(text).map_err(|e| EditError::Parse(e.to_string()))?;
    let mut edits = Vec::new();
    walk(doc.nodes(), &mut edits);
    if edits.is_empty() {
        return Ok(None);
    }
    edits.sort_by_key(|e| std::cmp::Reverse(e.start));
    let mut out = text.to_owned();
    for e in edits {
        out.replace_range(e.start..e.end, &e.with);
    }
    Ok(Some(out))
}

/// A tree's height is its `top`: `tree size=40` is
/// `tree top="bottom + 40"`, `bottom` being the ground at its foot, so the
/// tree draws the same height from the same roll and builds as before.  A
/// bare range gains its brackets (`"30..36"` is `[30 .. 36]` after
/// `bottom +`); a weighted range or a sum is kept as written.  On the line
/// or as a control's own line.  A size that names a definition's
/// parameter is left as written: the decoder says what to do.  Every other
/// byte is as written.  `Ok(None)` when no tree writes a `size`.
///
/// # Errors
/// If the text is not KDL.
pub fn tree_top(text: &str) -> Result<Option<String>, EditError> {
    /// The height `v` as a top over the tree's foot, or `None` for a
    /// parameter's name.
    fn top(v: &kdl::KdlValue) -> Option<String> {
        let body = match v {
            kdl::KdlValue::Integer(i) => i.to_string(),
            kdl::KdlValue::Float(f) => f.to_string(),
            kdl::KdlValue::String(s) => {
                let s = s.trim();
                if s.chars().all(|c| c.is_alphanumeric() || c == '_')
                    && !s.starts_with(|c: char| c.is_ascii_digit())
                {
                    return None;
                }
                match s.split_once("..") {
                    Some((lo, hi))
                        if !s.contains(['[', '(', '+', '*', '/'])
                            && !lo.trim().is_empty()
                            && !hi.trim().is_empty() =>
                    {
                        format!("[{} .. {}]", lo.trim(), hi.trim())
                    }
                    _ => s.to_owned(),
                }
            }
            _ => return None,
        };
        Some(format!("bottom + {body}"))
    }
    fn walk(text: &str, nodes: &[KdlNode], edits: &mut Vec<Edit>) {
        for n in nodes {
            if n.name().value() == "tree" {
                for e in n.entries() {
                    if e.name().is_some_and(|k| k.value() == "size")
                        && let Some(t) = top(e.value())
                    {
                        edits.push(Edit {
                            start: e.span().offset(),
                            end: e.span().offset() + e.span().len(),
                            with: format!("top=\"{t}\""),
                        });
                    }
                }
                for c in n.children().into_iter().flat_map(KdlDocument::nodes) {
                    if c.name().value() == "size"
                        && c.entries().len() == 1
                        && c.entries()[0].name().is_none()
                        && c.children().is_none()
                        && let Some(t) = top(c.entries()[0].value())
                    {
                        // `size 40` on its own line: the top goes on the tree's line
                        let a = c.span().offset();
                        let line_start = text[..a].rfind('\n').map_or(0, |i| i + 1);
                        let mut end = a + c.span().len();
                        let rest = &text[end..];
                        if text[line_start..a].trim().is_empty()
                            && rest.trim_start_matches([' ', '\t', ';']).starts_with('\n')
                        {
                            end += rest.find('\n').map_or(rest.len(), |i| i + 1);
                            edits.push(Edit {
                                start: line_start,
                                end,
                                with: String::new(),
                            });
                        } else {
                            let start = text[..a].trim_end().len();
                            let rest = &text[end..];
                            end += rest.len() - rest.trim_start_matches(';').len();
                            edits.push(Edit {
                                start,
                                end,
                                with: String::new(),
                            });
                        }
                        let at = n.name().span().offset() + n.name().span().len();
                        edits.push(Edit {
                            start: at,
                            end: at,
                            with: format!(" top=\"{t}\""),
                        });
                    }
                }
            }
            if let Some(kids) = n.children() {
                walk(text, kids.nodes(), edits);
            }
        }
    }
    let doc = KdlDocument::parse(text).map_err(|e| EditError::Parse(e.to_string()))?;
    let mut edits = Vec::new();
    walk(text, doc.nodes(), &mut edits);
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

/// Dirt beds no stones: `stones=none` on a `dirt` line is dropped (it is
/// what dirt is now).  `stones=some` or `many` is left as written: the
/// decoder says to scatter rocks instead.  `Ok(None)` when no dirt writes
/// `stones=none`.
///
/// # Errors
/// If the text is not KDL.
pub fn dirt_stones(text: &str) -> Result<Option<String>, EditError> {
    fn walk(nodes: &[KdlNode], text: &str, edits: &mut Vec<Edit>) {
        for n in nodes {
            if n.name().value() == "dirt" {
                // `stones none` as a line of its own in the dirt's block
                for c in n.children().into_iter().flat_map(KdlDocument::nodes) {
                    if c.name().value() == "stones"
                        && c.entries().len() == 1
                        && c.entries()[0].value().as_string() == Some("none")
                        && c.children().is_none()
                    {
                        let a = c.span().offset();
                        let line_start = text[..a].rfind('\n').map_or(0, |i| i + 1);
                        let mut end = a + c.span().len();
                        let rest = &text[end..];
                        let on_own_line = text[line_start..a].trim().is_empty()
                            && rest.trim_start_matches([' ', '\t', ';']).starts_with('\n');
                        if on_own_line {
                            end += rest.find('\n').map_or(rest.len(), |i| i + 1);
                            edits.push(Edit {
                                start: line_start,
                                end,
                                with: String::new(),
                            });
                        } else {
                            let start = text[..a].trim_end().len();
                            let rest = &text[end..];
                            end += rest.len() - rest.trim_start_matches(';').len();
                            edits.push(Edit {
                                start,
                                end,
                                with: String::new(),
                            });
                        }
                    }
                }
                for e in n.entries() {
                    if e.name().is_some_and(|k| k.value() == "stones")
                        && e.value().as_string() == Some("none")
                    {
                        let end = e.span().offset() + e.span().len();
                        // the entry and the space before it
                        let start = text[..e.span().offset()].trim_end().len();
                        edits.push(Edit {
                            start,
                            end,
                            with: String::new(),
                        });
                    }
                }
            }
            if let Some(kids) = n.children() {
                walk(kids.nodes(), text, edits);
            }
        }
    }
    let doc = KdlDocument::parse(text).map_err(|e| EditError::Parse(e.to_string()))?;
    let mut edits = Vec::new();
    walk(doc.nodes(), text, &mut edits);
    if edits.is_empty() {
        return Ok(None);
    }
    edits.sort_by_key(|e| std::cmp::Reverse(e.start));
    let mut out = text.to_owned();
    for e in edits {
        out.replace_range(e.start..e.end, &e.with);
    }
    Ok(Some(out))
}

/// The `flowers` covering is a scatter of a `flower` thing:
/// `flowers at=bank perSquare=4 petals=5 radius=2.5` is
/// `scatter at=bank perSquare=4 { flower size=5 petals=5 }`.  The scatter
/// keeps the place, the density and the seed (`perSquare=4` is written even
/// when the old line left it to the default, which was 4); the flower takes
/// the rest, its `radius` as the width across the petals, `size` (twice
/// the radius).  A `flowers` written with a block, or a control that is not
/// a number, is left as written: the decoder names the new form.
/// `Ok(None)` when no `flowers` is written.
///
/// # Errors
/// If the text is not KDL.
pub fn flowers(text: &str) -> Result<Option<String>, EditError> {
    fn walk(text: &str, nodes: &[KdlNode], edits: &mut Vec<Edit>) {
        for n in nodes {
            if n.name().value() == "flowers" && n.children().is_none() {
                let (off, len) = (n.span().offset(), n.span().len());
                let node_text = text[off..off + len].trim_end();
                let mut scatter: Vec<String> = Vec::new();
                let mut flower: Vec<String> = Vec::new();
                let mut per_square = false;
                let mut size: Option<String> = None;
                let mut ok = true;
                for e in n.entries() {
                    let raw = text[e.span().offset()..e.span().offset() + e.span().len()].trim();
                    match e.name().map(|k| k.value()) {
                        None => ok = false,
                        Some("at" | "seed") => scatter.push(raw.to_owned()),
                        Some("perSquare") => {
                            per_square = true;
                            scatter.push(raw.to_owned());
                        }
                        Some("radius") => match e
                            .value()
                            .as_float()
                            .or_else(|| e.value().as_integer().map(|i| i as f64))
                        {
                            Some(r) => {
                                size = Some(num_text(&KdlValue::Float(r * 2.0)).unwrap_or_default())
                            }
                            None => ok = false,
                        },
                        Some("petals" | "centre" | "stem" | "dome" | "petal") => {
                            flower.push(raw.to_owned());
                        }
                        Some(_) => ok = false,
                    }
                }
                if ok {
                    if !per_square {
                        scatter.push("perSquare=4".to_owned());
                    }
                    if let Some(sz) = size {
                        flower.insert(0, format!("size={sz}"));
                    }
                    let body = if flower.is_empty() {
                        "flower".to_owned()
                    } else {
                        format!("flower {}", flower.join(" "))
                    };
                    edits.push(Edit {
                        start: off,
                        end: off + node_text.len(),
                        with: format!("scatter {} {{ {body} }}", scatter.join(" ")),
                    });
                }
            }
            if let Some(kids) = n.children() {
                walk(text, kids.nodes(), edits);
            }
        }
    }
    let doc = KdlDocument::parse(text).map_err(|e| EditError::Parse(e.to_string()))?;
    let mut edits = Vec::new();
    walk(text, doc.nodes(), &mut edits);
    if edits.is_empty() {
        return Ok(None);
    }
    edits.sort_by_key(|e| std::cmp::Reverse(e.start));
    let mut out = text.to_owned();
    for e in edits {
        out.replace_range(e.start..e.end, &e.with);
    }
    Ok(Some(out))
}

/// Rewrite the hyphenated control names of the old spelling
/// (`per-square=55`) as camelCase (`perSquare=55`), on the line or as a
/// control's own line, leaving every other byte as written.
/// `Ok(None)` when there is none.
///
/// # Errors
/// If the text is not KDL.
pub fn control_names(text: &str) -> Result<Option<String>, EditError> {
    fn walk(nodes: &[KdlNode], edits: &mut Vec<Edit>) {
        for n in nodes {
            let name = n.name();
            if let Some(new) = dt_doc::elements::camel_of(name.value()) {
                edits.push(Edit {
                    start: name.span().offset(),
                    end: name.span().offset() + name.span().len(),
                    with: new.to_owned(),
                });
            }
            for e in n.entries() {
                if let Some(k) = e.name()
                    && let Some(new) = dt_doc::elements::camel_of(k.value())
                {
                    edits.push(Edit {
                        start: k.span().offset(),
                        end: k.span().offset() + k.span().len(),
                        with: new.to_owned(),
                    });
                }
            }
            if let Some(kids) = n.children() {
                walk(kids.nodes(), edits);
            }
        }
    }
    let doc = KdlDocument::parse(text).map_err(|e| EditError::Parse(e.to_string()))?;
    let mut edits = Vec::new();
    walk(doc.nodes(), &mut edits);
    if edits.is_empty() {
        return Ok(None);
    }
    edits.sort_by_key(|e| std::cmp::Reverse(e.start));
    let mut out = text.to_owned();
    for e in edits {
        out.replace_range(e.start..e.end, &e.with);
    }
    Ok(Some(out))
}

/// A free-standing form's two masses, as discs round a point at the 70 mm
/// reference the old forms were drawn for: the radius of the foot and of the
/// summit, the foot's band and the summit's, each as shares of the height.
/// The migrator only carries a form over, one region and one pin; the
/// library's own tiles are where such a mass is designed.
struct Mass {
    foot_r: f64,
    foot: (f64, f64),
    summit_r: f64,
    summit: (f64, f64),
}

/// The shortest rise a foot's band starts at, mm.
const FOOT_LO_MM: f64 = 3.0;

impl Mass {
    const fn new(foot_r: f64, foot: (f64, f64), summit_r: f64, summit: (f64, f64)) -> Self {
        Self {
            foot_r,
            foot,
            summit_r,
            summit,
        }
    }

    /// The regions and pin of a free-standing form: a foot, and a summit
    /// inside it that carries the film.
    fn write(&self, out: &mut Recipe, name: &str, scale: f64, h: f64) {
        let summit = format!("{name}2");
        out.regions.push(format!(
            "region {name} {{ within {} of=center }}",
            round1(self.foot_r * scale)
        ));
        out.regions.push(format!(
            "region {summit} {{ within {} of=center }}",
            round1(self.summit_r * scale)
        ));
        out.top = format!(
            "\"dirtEdgeHeight + [{} .. {}]\"",
            round1((h * self.foot.0).max(FOOT_LO_MM)),
            round1(h * self.foot.1)
        );
        out.pins.push(format!(
            "pin top=\"dirtEdgeHeight + [{} .. {}]\" at={summit}",
            round1(h * self.summit.0),
            round1(h * self.summit.1)
        ));
        out.film = summit;
    }
}

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
        "plateau" => {
            out.regions.push(format!(
                "region {name} {{ within {} of=center }}",
                round1(26.0 * scale)
            ));
        }
        "massif" => {
            Mass::new(26.5, (0.41, 0.5), 15.0, (0.91, 1.0)).write(&mut out, &name, scale, h)
        }
        "mound" => Mass::new(27.0, (0.0, 0.3), 10.0, (0.7, 1.0)).write(&mut out, &name, scale, h),
        "spire" => Mass::new(26.0, (0.0, 0.12), 9.5, (0.7, 1.0)).write(&mut out, &name, scale, h),
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
                        round1(24.0 * 1.3 * 0.5 * scale * z),
                        round1_sq(x),
                        round1_sq(y)
                    )
                })
                .collect();
            out.regions
                .push(format!("region {name} {{ {} }}", foot.join("; ")));
            out.top = format!("\"dirtEdgeHeight + [3 .. {}]\"", round1(h * 0.18));
            // one pin to each needle's summit
            for (i, &(x, y, z)) in needles.iter().enumerate() {
                let region = format!("{name}n{}", i + 1);
                out.regions.push(format!(
                    "region {region} {{ within {} of=\"({}, {})\" }}",
                    round1(10.0 * scale * z),
                    round1_sq(x),
                    round1_sq(y)
                ));
                let hi = h * z;
                out.pins.push(format!(
                    "pin top=\"dirtEdgeHeight + [{} .. {}]\" at={region}",
                    round1(hi * 0.7),
                    round1(hi)
                ));
                if i == 0 {
                    out.film = region;
                }
            }
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
        let (_h0, h1) = range_of(prop(bank, "height"), (1.5, 3.5));
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
                "{nind}    pin top=\"[dirtEdgeHeight .. dirtEdgeHeight + {}]\" at=apron\n{nind}    pin top=dirtEdgeHeight at=edges\n",
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
        let old = "tile size=\"2x2\" seed=1 {\n    stack {\n        dirt level=\"edge + 0.3\" bumps=\"2.2..3.2\" bumpSize=14 {\n            clods some\n        }\n    }\n}\n";
        let new = heights(old).unwrap().unwrap();
        assert!(
            new.contains(
                "dirt bumpSize=14 top=\"[dirtEdgeHeight + 0.3 .. dirtEdgeHeight + 0.3 + 3.2]\" {"
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
        assert!(new.contains("dirt top=dirtEdgeHeight clods=none"), "{new}");
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
        assert!(new.contains("pin top=dirtEdgeHeight at=edges"), "{new}");
        assert!(!new.contains("bank"), "{new}");
        assert!(heights(&new).unwrap().is_none());
    }

    #[test]
    fn a_free_standing_form_is_one_region_and_one_pin() {
        let old = "tile size=\"1x1\" seed=1 {\n    stack {\n        base\n        dirt\n        cliff form=mound height=34 {\n            rock mixed\n        }\n    }\n}\n";
        let new = heights(old).unwrap().unwrap();
        assert!(
            new.contains("region crag { within 13.5 of=center }"),
            "{new}"
        );
        assert!(new.contains("region crag2 { within 5 of=center }"), "{new}");
        assert_eq!(new.matches("pin top=").count(), 1, "{new}");
        assert!(!new.contains("form="), "{new}");
        assert!(heights(&new).unwrap().is_none());
    }

    #[test]
    fn old_control_names_become_camel_case_and_nothing_else_moves() {
        let old = "tile size=\"1x1\" { // keep\n    stack {\n        scatter per-square=55 reach=4 { stone }   // a note\n        cliff {\n            stone-size massive\n        }\n        tree { crown clump-size=6; branch min-radius=0.9 }\n        dirt bump-size=\"14\"\n    }\n}\n";
        let new = control_names(old).unwrap().unwrap();
        assert_eq!(
            new,
            old.replace("per-square", "perSquare")
                .replace("stone-size", "stoneSize")
                .replace("clump-size", "clumpSize")
                .replace("min-radius", "minRadius")
                .replace("bump-size", "bumpSize")
        );
        assert!(control_names(&new).unwrap().is_none());
        // the migration entry point carries them too
        assert_eq!(heights(old).unwrap().unwrap().matches("-").count(), 0);
    }

    #[test]
    fn mostly_becomes_a_weighting_on_the_size() {
        let old = "tile size=\"1x1\" { // keep\n    stack {\n        scatter perSquare=55 reach=4 { stone size=\"1..6\" mostly=small }   // a note\n        scatter perSquare=6 { boulder size=\"2..8\" mostly=even; pebble size=\"1..2\" mostly=large }\n        rock mostly=small\n        rock size=4 mostly=small\n        stone {\n            size \"1..6\"\n            mostly small\n        }\n        scatter perSquare=2 mostly=small { boulder size=\"2..8\"; stone size=\"1..6\" mostly=large }\n    }\n}\n";
        let new = mostly(old).unwrap().unwrap();
        assert_eq!(
            new,
            "tile size=\"1x1\" { // keep\n    stack {\n        scatter perSquare=55 reach=4 { stone size=\"low(1..6, 11)\" }   // a note\n        scatter perSquare=6 { boulder size=\"2..8\"; pebble size=\"high(1..2)\" }\n        rock size=\"low(6..15, 11)\"\n        rock size=4\n        stone {\n            size \"low(1..6, 11)\"\n        }\n        scatter perSquare=2 { boulder size=\"low(2..8, 11)\"; stone size=\"high(1..6)\" }\n    }\n}\n"
        );
        assert!(mostly(&new).unwrap().is_none());
        // the migration entry point carries it too
        assert!(heights(old).unwrap().unwrap().find("mostly").is_none());
    }

    #[test]
    fn an_old_bounded_by_becomes_bounded_by_and_nothing_else_moves() {
        let old = "tile size=\"1x1\" { // keep\n    line shore { through west east }\n    region pool { bounded-by shore; fill \"0.5,0.2\" }   // a note\n    region bank {\n        bounded-by shore\n        fill \"0.5,0.8\"\n    }\n    stack { dirt in=bank }\n}\n";
        let new = control_names(old).unwrap().unwrap();
        assert_eq!(new, old.replace("bounded-by", "boundedBy"));
        assert!(control_names(&new).unwrap().is_none());
        assert_eq!(
            heights(old).unwrap().unwrap().matches("bounded-by").count(),
            0
        );
    }

    #[test]
    fn in_along_and_against_become_at_and_nothing_else_moves() {
        let old = "tile size=\"1x1\" { // keep\n    line shore { through west east }\n    region bank { boundedBy shore; fill \"0.5,0.8\" }\n    stack {\n        base\n        dirt in=bank   // a note\n        wall along=\"north west\" style=cut-stone\n        scatter against=cliff reach=4 perSquare=5 { stone }\n        wall in=bank along=north\n        scatter in=bank at=\"0.2,0.3\" count=2 { rock }\n    }\n}\n";
        let new = places(old).unwrap().unwrap();
        assert_eq!(
            new,
            old.replace("dirt in=bank", "dirt at=bank")
                .replace("wall along=\"north west\"", "wall at=\"north west\"")
                .replace("against=cliff", "at=\"foot(cliff)\"")
        );
        assert!(places(&new).unwrap().is_none());
        assert!(
            heights(old)
                .unwrap()
                .unwrap()
                .contains("at=\"foot(cliff)\"")
        );
    }

    #[test]
    fn a_hyphen_in_a_value_or_a_definition_name_is_left_alone() {
        let text = "tile size=\"1x1\" {\n    stack {\n        wall style=cut-stone\n        long-grass in=east\n    }\n}\n";
        assert!(control_names(text).unwrap().is_none());
    }
}

#[cfg(test)]
mod thing_tests {
    use super::*;

    #[test]
    fn a_trees_height_is_its_size_and_the_trunks_stays() {
        let old =
            "tile { stack { tree at=\"1,1\" height=\"low(36..40, 8)\" { trunk height=18 } } }";
        assert_eq!(
            tree_size(old).unwrap().unwrap(),
            "tile { stack { tree at=\"1,1\" size=\"low(36..40, 8)\" { trunk height=18 } } }"
        );
        let line = "tile { stack { tree { height 40 } } }";
        assert_eq!(
            tree_size(line).unwrap().unwrap(),
            "tile { stack { tree { size 40 } } }"
        );
        assert_eq!(
            tree_size("tile { stack { grass height=2 } }").unwrap(),
            None
        );
    }

    #[test]
    fn flowers_become_a_scatter_of_a_flower() {
        let old = "tile { stack { flowers at=bank perSquare=6 petals=7 radius=3 stem=1.5 } }";
        assert_eq!(
            flowers(old).unwrap().unwrap(),
            "tile { stack { scatter at=bank perSquare=6 { flower size=6 petals=7 stem=1.5 } } }"
        );
        // the old default density is kept
        assert_eq!(
            flowers("tile { stack { flowers } }").unwrap().unwrap(),
            "tile { stack { scatter perSquare=4 { flower } } }"
        );
        assert_eq!(
            flowers("tile { stack { scatter { rock } } }").unwrap(),
            None
        );
    }

    #[test]
    fn a_tree_size_is_its_top_over_its_foot() {
        let old = "tile {\n    stack {\n        tree size=35 { trunk height=18 }\n        scatter { tree size=\"26..32\" }\n        tree size=\"low(30..40, 4)\"\n        tree {\n            size 40\n            leaves bare\n        }\n    }\n}\n";
        let new = tree_top(old).unwrap().unwrap();
        assert!(
            new.contains("tree top=\"bottom + 35\" { trunk height=18 }"),
            "{new}"
        );
        assert!(new.contains("tree top=\"bottom + [26 .. 32]\""), "{new}");
        assert!(
            new.contains("tree top=\"bottom + low(30..40, 4)\""),
            "{new}"
        );
        assert!(
            new.contains("tree top=\"bottom + 40\" {\n            leaves bare\n"),
            "{new}"
        );
        assert!(tree_top(&new).unwrap().is_none());
        // a parameter's name is left to the decoder; a rock keeps its size
        assert!(
            tree_top("define oak { param size 50; stack { tree size=size } }")
                .unwrap()
                .is_none()
        );
        assert!(
            tree_top("tile { stack { rock size=8 } }")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn dirt_stones_none_is_dropped() {
        let old = "tile size=\"1x1\" {\n    stack {\n        base\n        dirt clods=none stones=none\n        dirt stones=none { pin top=4 at=edges }\n        dirt stones=some\n    }\n}\n";
        let new = dirt_stones(old).unwrap().unwrap();
        assert!(new.contains("        dirt clods=none\n"), "{new}");
        assert!(
            new.contains("        dirt { pin top=4 at=edges }\n"),
            "{new}"
        );
        assert!(new.contains("dirt stones=some"), "{new}");
        assert!(dirt_stones(&new).unwrap().is_none());
        let old = "tile {\n    stack {\n        dirt top=4 { stones none }\n        dirt {\n            stones none\n            pin top=4 at=edges\n        }\n    }\n}\n";
        let new = dirt_stones(old).unwrap().unwrap();
        assert!(new.contains("dirt top=4 {}"), "{new}");
        assert!(
            new.contains("        dirt {\n            pin top=4 at=edges\n"),
            "{new}"
        );
    }
}
