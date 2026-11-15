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
