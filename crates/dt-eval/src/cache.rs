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
use crate::doc::Tile;
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

use crate::hash::field as mix;

fn mix_f64s(h: u64, v: &[f64]) -> u64 {
    let h = v.iter().fold(h, |h, x| mix(h, &x.to_bits().to_le_bytes()));
    mix(h, &(v.len() as u64).to_le_bytes())
}

/// What every step reads besides its own entry.  `{:?}` prints floats
/// exactly (shortest round trip), so equal text is equal input.
pub(crate) fn base_key(
    frame: &Frame,
    seed: u64,
    opts: &Options,
    edges: [Option<f64>; 2],
    fills: bool,
    foots: &[Foot],
) -> u64 {
    let text = format!(
        "{frame:?}|{seed}|{}|{:?}|{}|{}|{edges:?}|{fills}",
        opts.system, opts.printer, opts.draft, opts.low
    );
    foots
        .iter()
        .fold(mix(crate::hash::OFFSET, text.as_bytes()), |h, f| {
            let h = mix(h, format!("{}|{:?}|{}", f.kind, f.squares, f.nx).as_bytes());
            let h = mix_f64s(h, &f.dist);
            f.cells
                .iter()
                .fold(h, |h, (p, d, w)| mix_f64s(h, &[p[0], p[1], d[0], d[1], *w]))
        })
}

/// Each step's key: the one before it and what the step itself reads.
pub(crate) fn step_keys(base: u64, tile: &Tile, steps: &[Step], jobs: &[Job]) -> Vec<u64> {
    let mut h = base;
    steps
        .iter()
        .map(|step| {
            h = match *step {
                Step::Lift => mix(h, b"lift"),
                Step::Entry(i) => {
                    let entry = tile.stack.get(i).map(|e| {
                        let controls: Vec<_> =
                            e.controls.iter().map(|c| (&c.name, &c.value)).collect();
                        let children: Vec<_> = e
                            .children
                            .iter()
                            .map(|c| {
                                let cs: Vec<_> =
                                    c.controls.iter().map(|s| (&s.name, &s.value)).collect();
                                (&c.what.value, cs, c.weight)
                            })
                            .collect();
                        // a drawn line is read at build time: its points count
                        let line = e.line.as_ref().map(|n| (n, e.side, tile.lines.get(n)));
                        format!(
                            "{:?}|{controls:?}|{children:?}|{:?}|{:?}|{:?}|{line:?}",
                            e.what.value, e.along, e.at, e.against
                        )
                    });
                    // past the stack: the connector, which the base key's
                    // system names
                    let text = format!(
                        "{i}|{}|{}|{:?}|{}|{}|{}|{}",
                        entry.unwrap_or_else(|| "connector".to_owned()),
                        jobs.get(i).map_or("", |j| j.id.as_str()),
                        jobs[i].seed,
                        jobs[i].area_hash,
                        jobs[i].band_hash,
                        jobs[i].under_hash,
                        jobs[i].regions_hash
                    );
                    mix(h, text.as_bytes())
                }
            };
            h
        })
        .collect()
}
