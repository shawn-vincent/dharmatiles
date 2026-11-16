//! A thing built alone: the thing editor's build.
//!
//! A **thing** (`dt_elements::Thing`: a rock, a tree, a flower, a definition whose stack is one of them) is made from its
//! own controls and a seed and reads nothing, so it can be drawn on its own, in its own frame (the origin where it meets
//! what holds it, +z up), with no tile and no base.  [`subject`] finds the thing a document means (a `define` by name, or
//! one entry or one child of a scatter by the id the check gave it) and reads its controls as the reader made them;
//! [`Subject::draw`] makes one draw of it at a seed, joins its parts and puts it through the gate's overhang paint, as a
//! [`Built`] the page's one mesh payload is made from.
//!
//! There is no second reader and no second build: a definition is read by the reader as a one-entry tile that calls it
//! (so its params, defaults and imports mean what they mean in a tile), the entry's controls go to the element's own
//! `thing()`, and `Thing::make` is the one thing that makes solids.  A build here is not kept (the disk cache's key is
//! made of a whole tile): it is the thing's own key's job to keep it, when it has one.

use dt_doc::schema::Generator;
use dt_elements::{Extent, Shown, Thing};
use dt_kit::{EntryId, Frame, Scene, Seed, Tag};
use miette::SourceSpan;

use crate::build::{Built, CacheUse, Product, Report};
use crate::check::controls_of;
use crate::diag::{Diag, Src};
use crate::doc;
use crate::options::Options;
use crate::source::Source;

/// Which thing of a document is meant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Which<'a> {
    /// A `define` of that name, called with its defaults.
    Define(&'a str),
    /// A stack entry of the tile by its id ([`doc::Entry::id`], what a check's stack gives as `key`): one thing of it
    /// when `child` is given and the entry is a scatter.
    Entry {
        /// The entry's id.
        key: &'a str,
        /// The place of one of a scatter's things.
        child: Option<usize>,
    },
}

/// Why no thing could be made of what was asked.
#[derive(Debug)]
pub enum Refused {
    /// The document, or the definition's call, does not read: each problem at its text.
    Mistakes(Vec<Diag>),
    /// The document has no definition of that name.
    NoDefine(String),
    /// The document's tile has no entry of that id (or the scatter no such child).
    NoEntry(String),
    /// What was named is a layer (it reads what is under it and changes it), not a thing.
    Layer {
        /// What it was named as.
        name: String,
        /// The element that makes it a layer.
        element: String,
    },
}

/// A thing a document means, with what the document says of it.
pub struct Subject {
    /// The element it is (`rock`, `tree`).
    pub element: String,
    /// What the document is called, for the answer.
    pub document: String,
    /// The hash of the document's text, for the build record.
    pub hash: u64,
    /// Its controls as built: what the property sheet shows.
    pub shown: Vec<Shown>,
    thing: Box<dyn Thing>,
    at: (Src, SourceSpan),
}

/// The thing the document at `path` means by `which`, read for `opts`' target.
///
/// # Errors
/// [`Refused`]: the document does not read, there is no such name or entry, or it is a layer.
pub fn subject(
    sources: &dyn Source,
    path: &str,
    which: Which<'_>,
    opts: &Options,
) -> Result<Subject, Refused> {
    let units = opts.units();
    let own = doc::read_with(sources, path, units).map_err(Refused::Mistakes)?;
    let hash = sources.read(path).map_or(0, |t| doc::text_hash(&t));
    let (element, settings, at) = match which {
        Which::Define(name) => {
            let Some(def) = own.file.doc.definition(name) else {
                return Err(Refused::NoDefine(name.to_owned()));
            };
            if !def.thing {
                // the first entry that is no thing is what makes it a layer
                let element = def
                    .stack
                    .iter()
                    .find(|e| !is_thing_word(&e.what.value))
                    .or_else(|| def.stack.first())
                    .map_or_else(|| name.to_owned(), |e| e.what.value.clone());
                return Err(Refused::Layer {
                    name: name.to_owned(),
                    element,
                });
            }
            // the reader's own way to call a definition: a tile of one entry that does
            let probe = Probe::of(sources, path, name);
            let read = doc::read_with(&probe, &probe.path, units).map_err(Refused::Mistakes)?;
            let Some(tile) = read.tile else {
                return Err(Refused::Mistakes(vec![Diag::plain(
                    dt_lang::codes::EMPTY,
                    format!("the call of `{name}` made no tile"),
                )]));
            };
            let [e] = tile.stack.as_slice() else {
                return Err(Refused::Layer {
                    name: name.to_owned(),
                    element: name.to_owned(),
                });
            };
            (
                e.what.value.clone(),
                controls_of(&e.controls),
                (e.src.clone(), e.what.span),
            )
        }
        Which::Entry { key, child } => {
            let Some(tile) = own.tile else {
                return Err(Refused::NoEntry(key.to_owned()));
            };
            let Some(e) = tile.stack.iter().find(|e| e.id.as_str() == key) else {
                return Err(Refused::NoEntry(key.to_owned()));
            };
            match child {
                None => (
                    e.what.value.clone(),
                    controls_of(&e.controls),
                    (e.src.clone(), e.what.span),
                ),
                Some(i) => {
                    let Some(c) = e.children.get(i) else {
                        return Err(Refused::NoEntry(format!("{key} thing {i}")));
                    };
                    (
                        c.what.value.clone(),
                        controls_of(&c.controls),
                        (e.src.clone(), c.what.span),
                    )
                }
            }
        }
    };
    if !is_thing_word(&element) {
        return Err(Refused::Layer {
            name: match which {
                Which::Define(n) => n.to_owned(),
                Which::Entry { key, .. } => key.to_owned(),
            },
            element,
        });
    }
    let thing = dt_elements::thing::thing(&element, &settings).map_err(|le| {
        Refused::Mistakes(vec![
            Diag::at(
                &at.0,
                at.1,
                le.code.unwrap_or(dt_lang::codes::ELEMENT),
                le.message,
                "here",
            )
            .maybe_help(le.help),
        ])
    })?;
    // the controls as built are the element's own account of them
    let shown = dt_elements::shown(&element, &settings, &[]).map_err(|le| {
        Refused::Mistakes(vec![Diag::at(
            &at.0,
            at.1,
            le.code.unwrap_or(dt_lang::codes::ELEMENT),
            le.message,
            "here",
        )])
    })?;
    Ok(Subject {
        element,
        document: path.to_owned(),
        hash,
        shown,
        thing,
        at,
    })
}

/// Whether the element named is a thing (the schema's own account: its role).
fn is_thing_word(name: &str) -> bool {
    dt_doc::schema::element(name).is_some_and(|e| e.generator() == Generator::Thing)
}

impl Subject {
    /// The room it takes, mm as printed for `opts`' target (the extent is written in mm at the 35 mm square).
    #[must_use]
    pub fn extent(&self, opts: &Options) -> Extent {
        let s = Frame::for_system(opts.system, 1, 1).scale();
        let e = self.thing.extent();
        let scaled = |b: dt_elements::Box3| dt_elements::Box3::new(b.x * s, b.y * s, b.z * s);
        Extent::new(scaled(e.least), scaled(e.most))
    }

    /// One draw of it at `seed`, alone in its own frame, put through the gate's overhang paint (not the whole gate: a
    /// thing has no footprint to fill).
    ///
    /// # Errors
    /// The thing could not be made, joined or drawn.
    pub fn draw(&self, seed: u64, opts: &Options) -> Result<Built, Vec<Diag>> {
        let frame = Frame::for_system(opts.system, 1, 1);
        let at = |message: String| {
            Diag::at(
                &self.at.0,
                self.at.1,
                dt_lang::codes::BUILD,
                message,
                "this thing",
            )
        };
        let mut scene = Scene::new(frame, Seed::new(seed));
        let solids = self
            .thing
            .make(&mut scene, Seed::new(seed), &opts.printer)
            .map_err(|le| {
                vec![
                    Diag::at(
                        &self.at.0,
                        self.at.1,
                        le.code.unwrap_or(dt_lang::codes::ELEMENT),
                        le.message,
                        "here",
                    )
                    .maybe_help(le.help),
                ]
            })?;
        let owner = EntryId::new(&format!("thing/{}/0", self.element));
        for s in solids {
            scene.add(
                Tag {
                    kind: self.thing.writes(),
                    owner: owner.clone(),
                },
                s,
            );
        }
        let (mesh, mats) = scene.union().map_err(|e| vec![at(e.to_string())])?;
        let most = self.extent(opts).most;
        let footprint = [most.x, most.y];
        // the band every element gets: the printer's overhang to its steepest
        let band = dt_kit::HangBand {
            max_deg: opts.printer.steepest_deg,
            ..dt_kit::HangBand::DEFAULT
        };
        let (export, audit) =
            dt_kit::audit_rounded(&mesh, footprint, 0.0, &opts.printer, &mats, Some(&band));
        let materials = export.source.iter().map(|&t| mats[t as usize]).collect();
        let built = Built {
            product: Product {
                export,
                materials,
                audit,
                record: dt_kit::BuildRecord {
                    engine: opts.engine.clone(),
                    document: self.document.clone(),
                    document_hash: self.hash,
                    seed,
                    system: opts.system,
                    printer: opts.printer.name.clone(),
                },
                out: std::path::PathBuf::new(),
                footprint,
                checked: true,
            },
            report: Report {
                stages: scene.stages().to_vec(),
                edge_mm: None,
                notes: scene.notes().to_vec(),
                left_out: scene.left_outs().to_vec(),
                surface: None,
                covered: None,
                stage_secs: Vec::new(),
                stage_peak_mb: Vec::new(),
                reused: 0,
                look: dt_kit::Look::default(),
                entries: Vec::new(),
                pieces: Vec::new(),
                stack: Vec::new(),
                clouds: None,
                debug: None,
                placed: Vec::new(),
                cache: CacheUse::default(),
            },
        };
        Ok(built)
    }
}

/// A one-entry tile that calls the definition `name` of the document at `path`: what the reader reads to make the
/// definition's entry (its params at their defaults, its imports resolved), laid over the document's own source.
struct Probe<'a> {
    inner: &'a dyn Source,
    /// Beside the document, so a relative `use` of it resolves.
    path: String,
    text: String,
}

impl<'a> Probe<'a> {
    fn of(inner: &'a dyn Source, path: &str, name: &str) -> Self {
        let stem = path
            .rsplit('/')
            .next()
            .unwrap_or(path)
            .trim_end_matches(".dtile");
        let dir = &path[..path.len() - path.rsplit('/').next().map_or(0, str::len)];
        // a bare path is the library, `./` is the project beside this file
        let target = if path.starts_with(crate::source::LIB) {
            format!("{dir}{stem}")
        } else {
            format!("./{stem}")
        };
        Self {
            inner,
            path: format!("{dir}{stem}.thing.dtile"),
            text: format!(
                "dharmatiles version=\"1.0.0\"\nuse \"{target}\" {name}\ntile size=\"1x1\" seed=1 {{\n    stack {{\n        {name}\n    }}\n}}\n"
            ),
        }
    }
}

impl Source for Probe<'_> {
    fn read(&self, path: &str) -> Result<String, String> {
        if path == self.path {
            return Ok(self.text.clone());
        }
        self.inner.read(path)
    }
}
