//! dt-eval: from a document path to an audited mesh.
//!
//! Finding the project ([`discover`]), reading documents and their imports
//! through the [`Source`] trait (`lib:` is the embedded library), the
//! plan-then-build scheduler ([`plan::schedule`]), seeds by path, and the
//! one public call every front end uses: [`build`] this document for this
//! system. [`check`] does the plan pass only (no geometry).
//!
//! The document is read through [`doc`], a thin adapter (see its module
//! docs) until the reworked dt-doc lands.

pub mod diag;
pub mod doc;
pub mod plan;
pub mod source;

use std::path::{Path, PathBuf};

use dt_kit::{Audit, BuildRecord, Export, Frame, Mask, Material, Printer, Scene, Seed, Stage};
use dt_looks::{Control, Ctx, Look, Value};

pub use diag::Diag;
pub use source::{Files, LIB, Library, Memory, Source};

use crate::doc::{Raw, Tile};
use crate::plan::{Refusal, Step};

/// A project: the folder holding `dharmatiles.kdl` (or the start folder
/// when there is none) and where builds go.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Project {
    /// The project root.
    pub root: PathBuf,
    /// Build output: `out/` beside the project file unless it says
    /// `out "…"`.
    pub out: PathBuf,
}

/// **Project discovery**: walk up from `start` to the first folder with a
/// `dharmatiles.kdl`, as Cargo finds `Cargo.toml`.
#[must_use]
pub fn discover(start: &Path) -> Project {
    let found = start
        .ancestors()
        .find(|d| d.join("dharmatiles.kdl").is_file());
    let root = found.unwrap_or(start).to_path_buf();
    let out = found
        .and_then(|d| std::fs::read_to_string(d.join("dharmatiles.kdl")).ok())
        .and_then(|t| kdl::KdlDocument::parse(&t).ok())
        .and_then(|k| {
            k.get("out")
                .and_then(|n| n.entries().first())
                .and_then(|e| e.value().as_string().map(PathBuf::from))
        })
        .unwrap_or_else(|| PathBuf::from("out"));
    Project {
        out: root.join(out),
        root,
    }
}

/// How to build.
#[derive(Clone, Debug)]
pub struct Options {
    /// The connector system: `db`.
    pub system: String,
    /// Replaces the tile's seed (the parity harness's seeds 1–4).
    pub seed: Option<u64>,
    /// The printer the audit checks against.
    pub printer: Printer,
    /// Engine version for the build record.
    pub engine: String,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            system: "db".to_owned(),
            seed: None,
            printer: Printer::default(),
            engine: concat!("dt-eval-", env!("CARGO_PKG_VERSION")).to_owned(),
        }
    }
}

/// A finished build.
pub struct Built {
    /// The mesh as written, with each triangle's input triangle.
    pub export: Export,
    /// Material of each written triangle.
    pub materials: Vec<Material>,
    /// The gate's findings.
    pub audit: Audit,
    /// Stage hashes, in build order.
    pub stages: Vec<Stage>,
    /// Where it came from.
    pub record: BuildRecord,
    /// Output path below the project's `out/`, such as
    /// `db/test/flat-1x1-db.stl`.
    pub out: PathBuf,
    /// Tile width and height in mm.
    pub footprint: [f64; 2],
}

/// The plan of a document, without geometry.
pub struct Checked {
    /// The tile as read.
    pub tile: Tile,
    /// Its build order.
    pub steps: Vec<Step>,
    looks: Vec<Box<dyn Look>>,
    areas: Vec<Mask>,
    frame: Frame,
    seed: u64,
}

/// **check**: read `path` and plan it (no geometry).
///
/// # Errors
/// Every problem found, each at its text.
pub fn check(sources: &dyn Source, path: &str, opts: &Options) -> Result<Checked, Vec<Diag>> {
    let tile = doc::read(sources, path)?;
    let frame = Frame::dungeonblocks(tile.cols, tile.rows);
    let mut errors = Vec::new();
    let mut looks: Vec<Box<dyn Look>> = Vec::new();
    let mut areas = Vec::new();
    let (nx, ny) = frame.nodes();
    for e in &tile.stack {
        let controls = controls(&tile, e, &mut errors);
        match dt_looks::look(&e.what.value, &controls) {
            Ok(l) => looks.push(l),
            Err(le) => {
                let (span, label) = match &le.control {
                    Some(c) => (
                        e.controls
                            .iter()
                            .find(|(n, _)| &n.value == c)
                            .map_or(e.what.span, |(n, _)| n.span),
                        "here",
                    ),
                    None => (e.what.span, "not a look"),
                };
                let help = le.help.clone().or_else(|| {
                    diag::did_you_mean(&e.what.value, dt_looks::LOOKS.iter().copied())
                        .map(|w| format!("did you mean `{w}`?"))
                        .or_else(|| {
                            Some(format!("the looks so far: {}", dt_looks::LOOKS.join(", ")))
                        })
                });
                errors.push(Diag::at(&e.src, span, "dt::look", le.message, label).maybe_help(help));
                continue;
            }
        }
        areas.push(area(&tile, &frame, e, &mut errors));
    }
    match dt_looks::connector(&opts.system) {
        Ok(c) => {
            looks.push(c);
            areas.push(Mask::new(nx, ny, true));
        }
        Err(le) => errors.push(Diag::plain("dt::system", le.message).maybe_help(le.help)),
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    let plans: Vec<_> = looks.iter().map(|l| l.plan()).collect();
    let steps = plan::schedule(&plans).map_err(|r| {
        let (msg, entry) = match &r {
            Refusal::NoWriter { entry, kind } => (
                format!("this reads every {kind}, and nothing in the tile makes one"),
                *entry,
            ),
            Refusal::SurfaceReadsSolid { entry } => {
                ("a surface cannot read solids yet".to_owned(), *entry)
            }
            Refusal::Cycle { entries } => (
                "these entries read each other's solids".to_owned(),
                entries[0],
            ),
        };
        let d = match tile.stack.get(entry) {
            Some(e) => Diag::at(&e.src, e.span, "dt::plan", msg, "cannot be satisfied"),
            None => Diag::plain("dt::plan", msg),
        };
        vec![d.help("remove the read, or add what it reads to the stack")]
    })?;
    let seed = opts.seed.unwrap_or(tile.seed);
    Ok(Checked {
        tile,
        steps,
        looks,
        areas,
        frame,
        seed,
    })
}

/// An entry's area: the whole tile, or its `in=` region by paint bucket.
fn area(tile: &Tile, frame: &Frame, e: &doc::Entry, errors: &mut Vec<Diag>) -> Mask {
    let (nx, ny) = frame.nodes();
    let Some(name) = &e.area else {
        return Mask::new(nx, ny, true);
    };
    let Some(r) = tile.regions.iter().find(|r| r.name == name.value) else {
        errors.push(
            Diag::at(
                &e.src,
                name.span,
                "dt::unknown-region",
                format!("no region named `{}`", name.value),
                "not a region",
            )
            .maybe_help(
                diag::did_you_mean(&name.value, tile.regions.iter().map(|r| r.name.as_str()))
                    .map(|w| format!("did you mean `{w}`?"))
                    .or_else(|| Some("without in=, a look covers the whole tile".into())),
            ),
        );
        return Mask::new(nx, ny, false);
    };
    let lines: Vec<Vec<[f64; 2]>> = r
        .bounded_by
        .iter()
        .filter_map(|l| tile.lines.get(&l.value))
        .map(|pts| {
            pts.iter()
                .map(|p| p.squares(tile.cols, tile.rows))
                .collect()
        })
        .collect();
    let fills: Vec<[f64; 2]> = r
        .fills
        .iter()
        .map(|p| p.squares(tile.cols, tile.rows))
        .collect();
    dt_kit::area(frame, &lines, &fills)
}

/// Controls for a look: numbers as they are, `level` words resolved
/// against the tile's levels (`ground`, `"ground - 1"`).
fn controls(tile: &Tile, e: &doc::Entry, errors: &mut Vec<Diag>) -> Vec<Control> {
    let mut out = Vec::new();
    for (name, value) in &e.controls {
        let v = match (&value.value, name.value.as_str()) {
            (Raw::Number(n), _) => Value::Number(*n),
            (Raw::Text(t), "level") => {
                if let Some(v) = level(tile, t) {
                    Value::Number(v)
                } else {
                    let word = t.split(" + ").next().unwrap_or(t);
                    let word = word.split(" - ").next().unwrap_or(word).trim();
                    let help =
                        diag::did_you_mean(word, tile.levels.iter().map(|(n, _)| n.as_str()))
                            .map(|w| format!("did you mean `{w}`?"))
                            .unwrap_or_else(|| {
                                let names: Vec<String> = tile
                                    .levels
                                    .iter()
                                    .map(|(n, v)| format!("{n} ({v} mm)"))
                                    .collect();
                                format!("levels: {}; or mm, such as level=5", names.join(", "))
                            });
                    errors.push(
                        Diag::at(
                            &e.src,
                            value.span,
                            "dt::level",
                            format!("no level named `{word}`"),
                            "not a level",
                        )
                        .help(help),
                    );
                    continue;
                }
            }
            (Raw::Text(t), _) => Value::Word(t.clone()),
            (Raw::Bool(_), _) => {
                errors.push(Diag::at(
                    &e.src,
                    value.span,
                    "dt::control",
                    "no P1 control takes a switch",
                    "here",
                ));
                continue;
            }
        };
        out.push(Control {
            name: name.value.clone(),
            value: v,
        });
    }
    out
}

/// `name`, `name + n` or `name - n` against the tile's levels, in mm.
fn level(tile: &Tile, text: &str) -> Option<f64> {
    let find = |n: &str| {
        tile.levels
            .iter()
            .find(|(k, _)| k == n.trim())
            .map(|(_, v)| *v)
    };
    // level names may contain '-', so the operator is spaced
    for (op, sign) in [(" + ", 1.0), (" - ", -1.0)] {
        if let Some((a, b)) = text.split_once(op) {
            return Some(find(a)? + sign * b.trim().parse::<f64>().ok()?);
        }
    }
    find(text)
}

fn fnv(h: u64, v: u64) -> u64 {
    (h ^ v).wrapping_mul(0x0000_0100_0000_01b3)
}

/// **build**: `path` for `opts.system`, through the audit gate.
///
/// # Errors
/// Every problem found; a failed gate is not an error (see
/// [`Audit::failures`]).
pub fn build(sources: &dyn Source, path: &str, opts: &Options) -> Result<Built, Vec<Diag>> {
    let text = sources
        .read(path)
        .map_err(|e| vec![Diag::plain("dt::source", e)])?;
    let c = check(sources, path, opts)?;
    let fail = |e: String| vec![Diag::plain("dt::build", e)];
    let root = Seed::new(c.seed);
    let mut scene = Scene::new(c.frame, root);
    let mut h = c.steps.iter().fold(0xcbf2_9ce4_8422_2325, |h, s| {
        fnv(
            h,
            match s {
                Step::Entry(i) => *i as u64,
                Step::Lift => u64::MAX,
            },
        )
    });
    h = c.areas.iter().fold(h, |h, a| fnv(h, a.content_hash()));
    scene.record("plan", h);
    let paths = entry_paths(&c);
    for step in &c.steps {
        match *step {
            Step::Lift => scene
                .lift(&format!("tile/{}/ground", c.tile.name))
                .map_err(|e| fail(e.to_string()))?,
            Step::Entry(i) => {
                let pinned = c.tile.stack.get(i).and_then(|e| e.seed);
                let ctx = Ctx {
                    area: &c.areas[i],
                    path: &paths[i],
                    seed: pinned.map_or_else(|| root.path(&paths[i]), Seed::new),
                };
                c.looks[i].build(&mut scene, &ctx).map_err(|e| {
                    let msg = format!("{}: {}", paths[i], e.message);
                    match c.tile.stack.get(i) {
                        Some(en) => vec![
                            Diag::at(&en.src, en.span, "dt::build", msg, "while building this")
                                .maybe_help(e.help),
                        ],
                        None => fail(msg),
                    }
                })?;
            }
        }
    }
    let (mesh, mats) = scene.union().map_err(|e| fail(e.to_string()))?;
    let footprint = c.frame.size_mm();
    let (export, audit) = dt_kit::audit(&mesh, footprint, &opts.printer);
    let materials = export.source.iter().map(|&t| mats[t as usize]).collect();
    scene.record("export", export.mesh.content_hash());
    let record = BuildRecord {
        engine: opts.engine.clone(),
        document: path.to_owned(),
        document_hash: doc::text_hash(&text),
        seed: c.seed,
        system: opts.system.clone(),
    };
    Ok(Built {
        export,
        materials,
        audit,
        stages: scene.stages().to_vec(),
        record,
        out: out_path(path, &c.tile.name, &opts.system),
        footprint,
    })
}

/// Seeds by path: `tile/<name>/<look>/<k>`, `k` counting same-named
/// entries, so adding an entry of another look changes no other seed.
fn entry_paths(c: &Checked) -> Vec<String> {
    let mut seen: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    let mut out: Vec<String> = c
        .tile
        .stack
        .iter()
        .map(|e| {
            let k = seen.entry(e.what.value.as_str()).or_default();
            *k += 1;
            format!("tile/{}/{}/{}", c.tile.name, e.what.value, *k - 1)
        })
        .collect();
    out.push(format!("tile/{}/connector", c.tile.name));
    out
}

/// `lib:tiles/test/flat-1x1.kdl` → `db/test/flat-1x1-db.stl`; a disk path
/// keeps its folders below `tiles/`, if it has one.
fn out_path(path: &str, name: &str, system: &str) -> PathBuf {
    let p = path.strip_prefix(LIB).unwrap_or(path).replace('\\', "/");
    let rel = p.rsplit_once("tiles/").map_or(p.as_str(), |(_, r)| r);
    let dir = rel.rsplit_once('/').map_or("", |(d, _)| d);
    let stem = Path::new(rel)
        .file_stem()
        .map_or_else(|| name.to_owned(), |s| s.to_string_lossy().into_owned());
    let mut out = PathBuf::from(system);
    if !dir.is_empty() && !dir.starts_with('/') && !dir.contains(':') {
        out.push(dir);
    }
    out.push(format!("{stem}-{system}.stl"));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn out_paths_mirror_the_library() {
        assert_eq!(
            out_path("lib:tiles/test/flat-1x1.kdl", "flat", "db"),
            PathBuf::from("db/test/flat-1x1-db.stl")
        );
        assert_eq!(out_path("my.kdl", "x", "db"), PathBuf::from("db/my-db.stl"));
    }
}
