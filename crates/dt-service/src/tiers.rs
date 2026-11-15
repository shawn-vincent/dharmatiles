//! The tier book: what the page wants of a map's pieces, what is building and what is built, and each document's job.
//! The book is the owner's ([`crate::owner::Core`]); each step of the coordination is one [`BookOp`], applied in the
//! order sent.  The jobs themselves (the builds) are the transport's.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::time::Duration;

use crate::api;

/// A result kept: the mesh and the revision of the document it is of.
struct Done {
    doc_rev: u64,
    bytes: Arc<Vec<u8>>,
}

/// What is wanted, what is being built and what has been built.
#[derive(Default)]
pub struct TierBook {
    wanted: BTreeMap<String, Vec<String>>,
    inflight: BTreeSet<String>,
    results: BTreeMap<String, Done>,
    order: VecDeque<String>,
    /// The document each scope's wants are for.
    docs: BTreeMap<String, String>,
    /// The work in flight of each document: a job from its first piece wanted to its last one done.
    jobs: BTreeMap<String, Job>,
}

/// A document's pieces loading: the meshes not done (a piece has one per tier it wants), and how many pieces were
/// asked and finished.
struct Job {
    pending: BTreeSet<String>,
    /// For each piece with a mesh not done, how many.
    pieces: BTreeMap<String, usize>,
    total: usize,
    done: usize,
    /// The piece being built last started.
    current: Option<String>,
    started: Duration,
}

/// What the owner is asked of the tier book: the book is the owner's ([`crate::owner::Core`]), so each step of the
/// coordination (what is wanted, what is in flight, what is kept, the documents' jobs) is one command, applied in
/// the order sent.
pub enum BookOp {
    /// Replace `scope`'s wants: answers the keys nobody wants any more and the scope's document before this.
    Want {
        scope: String,
        doc: Option<String>,
        keys: Vec<String>,
        reply: Sender<(Vec<String>, Option<String>)>,
    },
    /// The revision of the kept result for a key.
    Have(String, Sender<Option<u64>>),
    /// Mark a key in flight: answers false when it already was.
    Claim(String, Sender<bool>),
    /// A key is no longer in flight.
    Release(String),
    /// A key is built: kept (the oldest go first), and no longer in flight.
    Keep {
        key: String,
        rev: u64,
        bytes: Arc<Vec<u8>>,
    },
    /// The documents whose scopes want a key.
    Docs(String, Sender<Vec<String>>),
    /// The kept result for a key.
    Result {
        key: String,
        doc_rev: Option<u64>,
        reply: Sender<Result<Option<Arc<Vec<u8>>>, u64>>,
    },
    JobAdd {
        doc: String,
        key: String,
    },
    JobDone {
        doc: String,
        key: String,
    },
    JobDoneEverywhere(String),
    JobCurrent {
        key: String,
        name: String,
    },
    JobTell(String),
}

impl TierBook {
    /// What `Want` does: the keys no longer wanted by anyone, and the scope's document before.
    pub fn want(
        &mut self,
        scope: &str,
        doc: Option<String>,
        keys: Vec<String>,
    ) -> (Vec<String>, Option<String>) {
        let old_doc = match doc {
            Some(d) => self.docs.insert(scope.to_owned(), d),
            None => self.docs.remove(scope),
        };
        let old = self.wanted.insert(scope.to_owned(), keys);
        let still: BTreeSet<&String> = self.wanted.values().flatten().collect();
        let dropped: Vec<String> = old
            .into_iter()
            .flatten()
            .filter(|k| !still.contains(k))
            .collect();
        for k in &dropped {
            self.inflight.remove(k);
        }
        (dropped, old_doc)
    }

    pub fn have(&self, key: &str) -> Option<u64> {
        self.results.get(key).map(|d| d.doc_rev)
    }

    pub fn claim(&mut self, key: &str) -> bool {
        self.inflight.insert(key.to_owned())
    }

    pub fn release(&mut self, key: &str) {
        self.inflight.remove(key);
    }

    pub fn keep(&mut self, key: &str, rev: u64, bytes: Arc<Vec<u8>>) {
        self.inflight.remove(key);
        if self
            .results
            .insert(
                key.to_owned(),
                Done {
                    doc_rev: rev,
                    bytes,
                },
            )
            .is_none()
        {
            self.order.push_back(key.to_owned());
        }
        while self.order.len() > KEPT {
            if let Some(old) = self.order.pop_front() {
                self.results.remove(&old);
            }
        }
    }

    pub fn docs_wanting(&self, key: &str) -> Vec<String> {
        let mut docs: Vec<String> = self
            .wanted
            .iter()
            .filter(|(_, keys)| keys.iter().any(|k| k == key))
            .filter_map(|(scope, _)| self.docs.get(scope).cloned())
            .collect();
        docs.sort();
        docs.dedup();
        docs
    }

    pub fn result(&self, key: &str, doc_rev: Option<u64>) -> Result<Option<Arc<Vec<u8>>>, u64> {
        match self.results.get(key) {
            None => Ok(None),
            Some(d) if doc_rev.is_none_or(|r| r == d.doc_rev) => Ok(Some(Arc::clone(&d.bytes))),
            Some(d) => Err(d.doc_rev),
        }
    }

    /// A piece was asked for on behalf of `doc`: true when this begins the document's job.
    pub fn job_add(&mut self, doc: &str, key: &str, now: Duration) -> bool {
        let began = !self.jobs.contains_key(doc);
        let job = self.jobs.entry(doc.to_owned()).or_insert_with(|| Job {
            pending: BTreeSet::new(),
            pieces: BTreeMap::new(),
            total: 0,
            done: 0,
            current: None,
            started: now,
        });
        if job.pending.insert(key.to_owned()) {
            let n = job.pieces.entry(piece_of(key)).or_insert(0);
            if *n == 0 {
                job.total += 1;
            }
            *n += 1;
        }
        began
    }

    /// The piece is no longer owed to `doc`: none when the document has no job, else whether the job is over (and
    /// gone).
    pub fn job_done(&mut self, doc: &str, key: &str) -> Option<bool> {
        let job = self.jobs.get_mut(doc)?;
        if job.pending.remove(key) {
            let piece = piece_of(key);
            if let Some(n) = job.pieces.get_mut(&piece) {
                *n -= 1;
                if *n == 0 {
                    job.pieces.remove(&piece);
                    job.done += 1;
                }
            }
        }
        if job.pending.is_empty() {
            self.jobs.remove(doc);
            Some(true)
        } else {
            Some(false)
        }
    }

    /// The documents that are owed `key`.
    pub fn owed(&self, key: &str) -> Vec<String> {
        self.jobs
            .iter()
            .filter(|(_, j)| j.pending.contains(key))
            .map(|(d, _)| d.clone())
            .collect()
    }

    /// `name` is being built for `key`: the documents whose job says which.
    pub fn job_current(&mut self, key: &str, name: &str) -> Vec<String> {
        self.jobs
            .iter_mut()
            .filter(|(_, j)| j.pending.contains(key))
            .map(|(d, j)| {
                j.current = Some(name.to_owned());
                d.clone()
            })
            .collect()
    }

    /// The job's progress: pieces done of pieces asked, the piece being built, and an estimate once one piece is done.
    pub fn job_progress(&self, doc: &str, now: Duration) -> Option<api::Progress> {
        let j = self.jobs.get(doc)?;
        let elapsed = now.saturating_sub(j.started).as_secs_f64();
        let (done, total) = (j.done as f64, j.total.max(1) as f64);
        let estimate = (j.done > 0).then(|| elapsed / done * total);
        Some(api::Progress {
            what: "loading".to_owned(),
            fraction: done / total,
            label: j.current.clone().unwrap_or_else(|| "pieces".to_owned()),
            key: "pieces".to_owned(),
            count: Some(("pieces".to_owned(), j.done, j.total)),
            elapsed_s: elapsed,
            eta_s: estimate.map(|e| (e - elapsed).max(0.0)),
            estimate_s: estimate,
        })
    }
}

/// How many results are kept (the oldest go first).
const KEPT: usize = 300;

/// The piece a mesh's key is of: the key without its tier.
pub fn piece_of(key: &str) -> String {
    key.split('|')
        .enumerate()
        .filter(|(i, _)| *i != 1)
        .map(|(_, p)| p)
        .collect::<Vec<_>>()
        .join("|")
}
