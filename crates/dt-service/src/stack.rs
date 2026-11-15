//! The preview's stack line: each entry named by what it places, with a
//! link to that thing's page on the documentation site, and where it is
//! written (`file`, byte `span`, 1-based `lines`) so the editor can light
//! its text when its geometry is picked.  An entry is a [`StackItem`]: one
//! type for the check's answer, the plan and every reader (its TypeScript
//! type is generated from it).

/// The page (under the docs site) that documents an element: every built-in
/// element has its own page under `elements/`.
#[must_use]
pub fn page(element: &str) -> String {
    if dt_doc::schema::element(element).is_some() {
        format!("elements/{element}.html")
    } else {
        "elements/index.html".to_owned()
    }
}

/// A control written on an entry: its name, the text written (`size="6dmm .. 10dmm"`, or a child line `height 35`),
/// its byte span, and the file it is in when that is not the entry's own (a definition's controls are written in the
/// definition's file).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct WrittenControl {
    pub name: String,
    /// The whole control as written (`name=value`, or the line `name value`), and its span.
    pub text: String,
    pub span: [usize; 2],
    /// The value alone, as written.
    pub value: String,
    /// The value as words: one string's quotes off, its escapes undone.
    pub plain: String,
    /// The file it is written in, when not the entry's own.
    pub file: Option<String>,
    /// Written in a part the entry calls: what the sheet says it comes from.
    pub from: Option<String>,
}

/// What a control shows where nothing is written.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct UnwrittenControl {
    pub control: String,
    pub text: String,
    /// `text` as words: one string's quotes off, its escapes undone.
    pub plain: String,
    pub from: String,
}

/// A cloud an entry writes (`top=`, a pin): where its text is.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct CloudAt {
    /// `top` or `pin`.
    pub kind: String,
    pub file: String,
    pub span: [usize; 2],
    /// A pin's place among its entry's pins.
    pub pin: Option<usize>,
}

/// A property of an entry as written, for the map's handles (`at=bank`).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct WrittenPlace {
    pub name: String,
    pub text: String,
    /// `text` as words: one string's quotes off, its escapes undone.
    pub plain: String,
    pub span: [usize; 2],
}

/// Where an entry made by a definition call is called, as the document writes it.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct CallAt {
    pub file: String,
    pub span: [usize; 2],
}

/// One thing of a scatter, with its own controls, for the lens.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct Thing {
    pub what: String,
    /// The offset its name starts at: a node is addressed by it.
    pub at: usize,
    pub controls: Vec<WrittenControl>,
    pub defaults: Vec<UnwrittenControl>,
}

/// One entry of a tile's stack: what it is, where it is written and what its controls are.  The base line is one too,
/// with nothing written on it but its words (`controls`, `defaults`, `clouds`, `place` and `things` are empty).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct StackItem {
    /// The entry's id ([`dt_eval::doc::Entry::id`]): what the page keeps to find the entry again whatever the stack
    /// looks like later.
    pub key: String,
    pub label: String,
    pub element: String,
    /// The page on the site that documents the element.
    pub href: String,
    /// The word that places it (`dirt`, `scatter`), or `base`.
    pub what: String,
    /// The seed the entry pins (`seed=`): the page says which entries keep their own.
    pub seed: Option<u64>,
    pub file: String,
    /// Where it is written: byte offset and length.
    pub span: [usize; 2],
    /// First and last line (1-based).
    pub lines: [usize; 2],
    pub call: Option<CallAt>,
    pub controls: Vec<WrittenControl>,
    pub defaults: Vec<UnwrittenControl>,
    pub clouds: Vec<CloudAt>,
    pub place: Vec<WrittenPlace>,
    /// Where a fixed `at=` point is, in squares from the south-west corner, as the program reads it.
    pub at: Option<[f64; 2]>,
    pub things: Vec<Thing>,
}

impl StackItem {
    /// The one line an STL file's stack is: nothing is written on it, it names what the file is.
    #[must_use]
    pub fn stl_file() -> Self {
        Self {
            key: "stl".to_owned(),
            label: "stl file".to_owned(),
            element: "stl".to_owned(),
            href: String::new(),
            what: "stl".to_owned(),
            seed: None,
            file: String::new(),
            span: [0, 0],
            lines: [1, 1],
            call: None,
            controls: Vec::new(),
            defaults: Vec::new(),
            clouds: Vec::new(),
            place: Vec::new(),
            at: None,
            things: Vec::new(),
        }
    }
}

use dt_eval::doc::{Entry, Named, Origin, Tile};
use dt_eval::solve::{SolveReport, SolvedTop};
use dt_eval::unwritten::{Unwritten, library_name, unwritten};

/// The documentation site with its trailing slash.  It is derived from the repository address in `Cargo.toml`
/// ([`dt_lang::codes::site_base`]), the one place the address is written.
#[must_use]
pub fn docs() -> String {
    format!("{}/", dt_lang::codes::site_base())
}

/// The 1-based line a byte offset of `text` lies on.
fn line_of(text: &str, o: usize) -> usize {
    text.get(..o.min(text.len()))
        .map_or(1, |t| t.matches('\n').count() + 1)
}

/// The tile's stack as [`StackItem`]s, in written order, ending with the base when the stack has a `base` line.
///
/// Each entry also says what its controls are where nothing is written
/// (`defaults`, see [`dt_eval::unwritten`]): `solved` is what the solver made
/// of the bodies, which is where a body's default `top` is picked.
///
/// # Errors
/// If a span the plan holds does not lie in its own source (a bug: the
/// page would be sent empty text in its place), or a control has nothing to
/// show where it is not written (a mistake in the schema's table).
pub fn items(
    tile: &Tile,
    solved: Option<&SolveReport>,
    system: dt_lang::System,
) -> Result<Vec<StackItem>, String> {
    let names = tile.names(system);
    let mut out = Vec::with_capacity(names.len());
    for (k, (e, named)) in tile.stack.iter().zip(&names).enumerate() {
        let top = solved
            .and_then(|s| s.bodies.iter().find(|b| b.entry == k))
            .map(|b| &b.top);
        out.push(entry(e, named, top)?);
    }
    if let Some(span) = tile.base_span {
        let key = names
            .last()
            .ok_or_else(|| "the stack has a base line but no name for it".to_owned())?
            .id
            .as_str()
            .to_owned();
        let (label, page) = dt_eval::doc::base_label(system);
        let (start, len) = (span.offset(), span.len());
        let text = tile.src.inner();
        out.push(StackItem {
            key,
            label: label.to_owned(),
            element: "connector".to_owned(),
            href: format!("{}elements/{page}.html", docs()),
            what: "base".to_owned(),
            seed: None,
            file: tile.src.name().to_owned(),
            span: [start, len],
            lines: [
                line_of(text, start),
                line_of(text, start + len.saturating_sub(1)),
            ],
            call: None,
            controls: Vec::new(),
            defaults: Vec::new(),
            clouds: Vec::new(),
            place: Vec::new(),
            at: None,
            things: Vec::new(),
        });
    }
    Ok(out)
}

/// What a control shows where nothing is written, as the page reads it.
fn defaults_of(list: &[Unwritten]) -> Vec<UnwrittenControl> {
    list.iter()
        .map(|u| UnwrittenControl {
            control: u.control.clone(),
            text: u.text.clone(),
            plain: dt_doc::nodeid::plain_of(&u.text),
            from: u.from.clone(),
        })
        .collect()
}

fn entry(e: &Entry, named: &Named, top: Option<&SolvedTop>) -> Result<StackItem, String> {
    let what = e.what.value.as_str();
    let (start, len) = (e.span.offset(), e.span.len());
    let text = e.src.inner();
    let controls = settings(&e.controls, e.src.name())?;
    let missing = unwritten(what, &e.controls, &e.children, top).map_err(|err| {
        format!(
            "the `{what}` entry at byte {start} of {}: {err}",
            e.src.name()
        )
    })?;
    // where it is put (at=), as written, for the map's handles
    const PLACE: [&str; 1] = ["at"];
    let props = dt_doc::nodeid::props_at(text, e.what.span.offset()).map_err(|err| {
        format!(
            "cannot read the properties of the `{what}` entry at byte {} of {}: {err}",
            e.what.span.offset(),
            e.src.name()
        )
    })?;
    // a scatter's things, each with its own controls, for the lens; a
    // node is addressed by the offset its name starts at
    let mut things = Vec::with_capacity(e.children.len());
    for c in &e.children {
        let missing = unwritten(&c.what.value, &c.controls, &[], None).map_err(|err| {
            format!(
                "the `{}` in the `{what}` entry at byte {start}: {err}",
                c.what.value
            )
        })?;
        things.push(Thing {
            what: c.what.value.clone(),
            at: c.what.span.offset(),
            controls: settings(&c.controls, e.src.name())?,
            defaults: defaults_of(&missing),
        });
    }
    Ok(StackItem {
        key: named.id.as_str().to_owned(),
        label: named.label.clone(),
        element: named.element.clone(),
        href: format!("{}{}", docs(), page(&named.element)),
        what: what.to_owned(),
        // the seed the entry pins, from the decoded entry
        seed: e.seed,
        file: e.src.name().to_owned(),
        span: [start, len],
        lines: [
            line_of(text, start),
            line_of(text, start + len.saturating_sub(1)),
        ],
        // an entry a definition call makes: the call, as the document writes it
        call: e.call.as_ref().map(|(src, span)| CallAt {
            file: src.name().to_owned(),
            span: [span.offset(), span.len()],
        }),
        controls,
        defaults: defaults_of(&missing),
        clouds: clouds(e)?,
        place: props
            .into_iter()
            .filter(|(n, ..)| PLACE.contains(&n.as_str()))
            .map(|(name, text, span)| WrittenPlace {
                plain: dt_doc::nodeid::plain_of(&text),
                name,
                text,
                span,
            })
            .collect(),
        // where a fixed `at=` point is, in squares from the south-west corner,
        // as the program reads it (the page never reads a written point)
        at: e
            .at
            .as_ref()
            .and_then(|d| (d[0].is_point() && d[1].is_point()).then(|| [d[0].lo(), d[1].lo()])),
        things,
    })
}

/// The text of `span` in `src`.
fn written(src: &dt_eval::diag::Src, span: miette::SourceSpan) -> Result<&str, String> {
    let (o, l) = (span.offset(), span.len());
    src.inner().get(o..o + l).ok_or_else(|| {
        format!(
            "the span ({o}, {l}) is not in {}: the plan holds a bad span",
            src.name()
        )
    })
}

/// The clouds an entry writes: its `top=` (a body's, a tree's or rock's, or
/// the call's it was given), each pin of a body, and the `top=` of each thing
/// of a scatter, each with the file it is written in and its byte span.  They
/// are read off the parsed entry, so a comment, a hidden (`/-`) node or a raw
/// string holds none.  `pin` is a pin's place among its entry's pins.
fn clouds(e: &Entry) -> Result<Vec<CloudAt>, String> {
    let cloud =
        |kind: &str, src: &dt_eval::diag::Src, span: miette::SourceSpan, pin: Option<usize>| {
            // a node's span may run on past its last word
            let text = written(src, span)?;
            Ok::<_, String>(CloudAt {
                kind: kind.to_owned(),
                file: src.name().to_owned(),
                span: [span.offset(), text.trim_end().len()],
                pin,
            })
        };
    let mut out = Vec::new();
    if let Some(t) = &e.top {
        out.push(cloud("top", &t.src, t.prop, None)?);
    }
    for (k, p) in e.pins.iter().enumerate() {
        out.push(cloud("pin", &p.from.src, p.span, Some(k))?);
    }
    for c in &e.children {
        if let Some(t) = &c.top {
            out.push(cloud("top", &t.src, t.prop, None)?);
        }
    }
    Ok(out)
}

/// The controls written on an entry (a default, a number a word stands for
/// or one worked out from a line is not written: [`Origin`]): name, the text
/// written (`size="6dmm .. 10dmm"`, or a child line `height 35`) and its byte
/// span, and the file it is in (a definition's controls are written in the
/// definition's file).
fn settings(controls: &[dt_eval::doc::Setting], file: &str) -> Result<Vec<WrittenControl>, String> {
    controls
        .iter()
        .filter(|c| c.origin == Origin::Written)
        .map(|c| {
            let (o, l) = (c.span.offset(), c.span.len());
            let text = c.src.inner();
            let written = text.get(o..o + l).ok_or_else(|| {
                format!(
                    "the setting `{}` has a span ({o}, {l}) that is not in {}",
                    c.name,
                    c.src.name()
                )
            })?;
            Ok(WrittenControl {
                name: c.name.clone(),
                text: written.to_owned(),
                span: [o, l],
                value: dt_doc::nodeid::written_value(written, &c.name).to_owned(),
                plain: dt_doc::nodeid::plain_of(dt_doc::nodeid::written_value(written, &c.name)),
                file: (c.src.name() != file).then(|| c.src.name().to_owned()),
                // written in a part the entry calls: what the sheet says it comes from
                from: (c.src.name() != file).then(|| library_name(c.src.name())),
            })
        })
        .collect()
}
