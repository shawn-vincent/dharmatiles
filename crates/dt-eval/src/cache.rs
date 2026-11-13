//! The build cache: the scene after each step of the last build, kept for
//! the next build of the same tile.
//!
//! Each step is keyed by everything it and the steps before it read: the
//! tile's frame, seed, system, printer, edges and planned feet, then each
//! step's element (its name, controls and children), its path, seed,
//! place and area, in order.  A build restores the scene after the last
//! step whose key it shares with the build before and runs only the rest,
//! so editing the grass on a cliff tile does not carve the cliff again.
//! A step's scene depends only on what its key covers, so a restored
//! build is the same, hash for hash, as one from nothing (a test checks
//! it).
//!
//! A clone of a scene shares its solids and paints, so keeping one after
//! every step costs little more than the last scene alone.

use std::sync::{Mutex, MutexGuard, PoisonError};

use dt_elements::Foot;
use dt_kit::{Frame, Scene};

use crate::build::Job;
use crate::canon::Canon;
use crate::doc::{Entry, Setting, Tile};
use crate::options::Options;
use crate::plan::Step;

/// The scenes one tile's last build left, step by step.  Give each tile
/// (and its drafts) its own: a build keeps only its own steps.  Builds
/// sharing one stay right (each step's key covers every step before it,
/// and all of them must match), they only evict each other.
#[derive(Default)]
pub struct Cache(Mutex<Vec<Kept>>);

/// The scene after one step, and whether the ground was banked by then.
#[derive(Clone)]
pub(crate) struct Kept {
    pub key: u64,
    pub scene: Scene,
    pub painted: bool,
    /// What the scene collected for the editor beside its geometry (clouds,
    /// the debug layer's marks): not part of the key, since it changes
    /// nothing built, but a build that wants more than a scene has cannot
    /// use it.
    pub collected: dt_kit::debug::Collect,
}

impl Cache {
    /// An empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Forget every kept scene (and the memory it holds).
    pub fn clear(&self) {
        self.lock().clear();
    }

    /// How many steps are kept.
    #[must_use]
    pub fn len(&self) -> usize {
        self.lock().len()
    }

    /// Whether nothing is kept.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn lock(&self) -> MutexGuard<'_, Vec<Kept>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The last kept step whose key, and every key before it, matches
    /// `keys` and that collected at least what `want` asks for: its index
    /// and a copy of what it kept.
    pub(crate) fn longest(
        &self,
        keys: &[u64],
        want: dt_kit::debug::Collect,
    ) -> Option<(usize, Kept)> {
        let kept = self.lock();
        let n = kept
            .iter()
            .zip(keys)
            .take_while(|(k, key)| k.key == **key && k.collected.covers(want))
            .count();
        n.checked_sub(1).map(|last| (last, kept[last].clone()))
    }

    /// Keep `step` as step `n`, forgetting what came after it: the steps
    /// before it are this build's already (restored or put), so a build
    /// stopped part way (a newer edit) still leaves what it finished.
    pub(crate) fn put(&self, n: usize, step: Kept) {
        let mut kept = self.lock();
        kept.truncate(n);
        if kept.len() == n {
            kept.push(step);
        }
    }
}

/// What every step reads besides its own entry, in the canonical form
/// ([`crate::canon`]): equal keys are equal inputs, floats by their bits.
pub(crate) fn base_key(
    frame: &Frame,
    seed: u64,
    opts: &Options,
    edges: [Option<f64>; 2],
    fills: bool,
    foots: &[Foot],
) -> u64 {
    let mut c = Canon::new();
    c.field("frame", frame)
        .field("seed", &seed)
        .field("system", &opts.system)
        .field("printer", &opts.printer)
        .field("draft", &opts.draft)
        .field("low", &opts.low)
        .field("edges", &edges)
        .field("fills", &fills)
        .field("foots", &foots.len());
    for f in foots {
        let Foot {
            kind,
            dist,
            cells,
            squares,
            nx,
        } = f;
        c.field("kind", &kind.to_string())
            .field("dist", dist)
            .field("cells", cells)
            .field("squares", squares)
            .field("nx", nx);
    }
    c.fnv()
}

/// Each step's key: the one before it and what the step itself reads.
pub(crate) fn step_keys(base: u64, tile: &Tile, steps: &[Step], jobs: &[Job]) -> Vec<u64> {
    let mut h = base;
    steps
        .iter()
        .map(|step| {
            let mut c = Canon::new();
            c.field("before", &h);
            match *step {
                Step::Lift => {
                    c.str("lift");
                }
                Step::Entry(i) => {
                    c.field("entry", &i);
                    match tile.stack.get(i) {
                        Some(e) => entry(&mut c, tile, e),
                        // past the stack: the connector, which the base
                        // key's system names
                        None => {
                            c.str("connector");
                        }
                    }
                    let job = &jobs[i];
                    c.field("job", job.id.as_str())
                        .field("seed", &job.seed.value())
                        .field("area", &job.area_hash)
                        .field("band", &job.band_hash)
                        .field("under", &job.under_hash)
                        .field("regions", &job.regions_hash);
                }
            }
            h = c.fnv();
            h
        })
        .collect()
}

/// One stack entry as a step reads it: its element, controls, place and
/// children.
fn entry(c: &mut Canon, tile: &Tile, e: &Entry) {
    c.field("what", &e.what.value)
        .field("controls", &settings(&e.controls))
        .field("along", &e.along)
        .field("at", &e.at)
        .field("against", &e.against);
    c.str("children").u64(e.children.len() as u64);
    for child in &e.children {
        c.field("what", &child.what.value)
            .field("controls", &settings(&child.controls))
            .field("weight", &child.weight);
    }
    // a drawn line is read at build time: its points count
    match &e.line {
        Some(name) => {
            c.field("line", name)
                .field("side", &e.side)
                .field("points", &tile.lines.get(name));
        }
        None => {
            c.str("no-line");
        }
    }
}

/// Controls as name and value pairs: where they were written is not read.
fn settings(controls: &[Setting]) -> Vec<(&String, &dt_elements::Value)> {
    controls.iter().map(|s| (&s.name, &s.value)).collect()
}
