//! The tiers a map wants: what the page asks of the program for a map's pieces, without holding a connection while
//! anything builds.
//!
//! The page posts the set of tiers it wants (`POST /api/tiers/want`: a scope, and for each a tile, a tier and the
//! piece's seed, system and printer).  Each post *replaces* the last of its scope, so a piece the page lets go of is
//! withdrawn by leaving it out, and the program cancels the jobs nobody wants any more.  Each wanted tier is a job on
//! the tier lane (see [`super::queue`]); when one is finished the program says so on the event stream (`{type:
//! "tier", key, docRev, error?}`) and keeps the result, which the page then fetches by key and revision
//! (`GET /api/tiers/result?key=K&docRev=N`; 409 for a result of another revision, never that one).  The program owns
//! the order and the work; the page owns the set of what it wants and nothing else.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::mpsc::Sender;

use serde::{Deserialize, Serialize};

use super::api::{Out, Reply};
use super::owner::Command;
use super::queue::Lane;
use super::*;

/// One tier of one piece's tile that the page wants.
#[derive(Deserialize, Clone, Serialize, Debug)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(optional_fields = nullable))]
#[serde(deny_unknown_fields)]
pub struct TierWant {
    pub tile: String,
    /// `super-low`, `low` or `full`.
    pub tier: dt_store::Tier,
    pub seed: Option<u64>,
    pub system: Option<dt_lang::System>,
    pub printer: Option<String>,
    /// A source text to build in place of the file's own (a step of the document's history): the file is not
    /// edited, and the picture is kept by the text, so a text built before is not built again.
    pub text: Option<String>,
    /// The file `text` is of: the tile's own document, or a file its last build read.
    pub file: Option<String>,
}

#[derive(Deserialize, Serialize, Debug)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(optional_fields = nullable))]
#[serde(deny_unknown_fields)]
pub struct TiersWantReq {
    /// Whose wants these are (a map, the picture shelf): a post replaces the last of the same scope.
    pub scope: String,
    /// The document the work is for (a map's tile id): its pieces loading is one job of it, said as the document's
    /// progress and in the messages like a build.  None for the picture shelf.
    #[serde(default)]
    pub doc: Option<String>,
    pub wants: Vec<TierWant>,
}

/// The key of each want, in the order asked: what its result is announced and fetched by.
#[derive(Serialize, Deserialize, Debug)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct TiersWantRes {
    pub keys: Vec<String>,
}

/// A result kept: the mesh and the revision of the document it is of.
struct Done {
    doc_rev: u64,
    bytes: Arc<Vec<u8>>,
}

/// What is wanted, what is being built and what has been built.
#[derive(Default)]
pub(super) struct TierBook {
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
    started: Instant,
}

/// What the owner is asked of the tier book: the book is the owner's ([`super::owner::Core`]), so each step of the
/// coordination (what is wanted, what is in flight, what is kept, the documents' jobs) is one command, applied in
/// the order sent.
pub(super) enum BookOp {
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
    pub(super) fn want(
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

    pub(super) fn have(&self, key: &str) -> Option<u64> {
        self.results.get(key).map(|d| d.doc_rev)
    }

    pub(super) fn claim(&mut self, key: &str) -> bool {
        self.inflight.insert(key.to_owned())
    }

    pub(super) fn release(&mut self, key: &str) {
        self.inflight.remove(key);
    }

    pub(super) fn keep(&mut self, key: &str, rev: u64, bytes: Arc<Vec<u8>>) {
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

    pub(super) fn docs_wanting(&self, key: &str) -> Vec<String> {
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

    pub(super) fn result(
        &self,
        key: &str,
        doc_rev: Option<u64>,
    ) -> Result<Option<Arc<Vec<u8>>>, u64> {
        match self.results.get(key) {
            None => Ok(None),
            Some(d) if doc_rev.is_none_or(|r| r == d.doc_rev) => Ok(Some(Arc::clone(&d.bytes))),
            Some(d) => Err(d.doc_rev),
        }
    }

    /// A piece was asked for on behalf of `doc`: true when this begins the document's job.
    pub(super) fn job_add(&mut self, doc: &str, key: &str) -> bool {
        let began = !self.jobs.contains_key(doc);
        let job = self.jobs.entry(doc.to_owned()).or_insert_with(|| Job {
            pending: BTreeSet::new(),
            pieces: BTreeMap::new(),
            total: 0,
            done: 0,
            current: None,
            started: Instant::now(),
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
    pub(super) fn job_done(&mut self, doc: &str, key: &str) -> Option<bool> {
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
    pub(super) fn owed(&self, key: &str) -> Vec<String> {
        self.jobs
            .iter()
            .filter(|(_, j)| j.pending.contains(key))
            .map(|(d, _)| d.clone())
            .collect()
    }

    /// `name` is being built for `key`: the documents whose job says which.
    pub(super) fn job_current(&mut self, key: &str, name: &str) -> Vec<String> {
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
    pub(super) fn job_progress(&self, doc: &str) -> Option<api::Progress> {
        let j = self.jobs.get(doc)?;
        let elapsed = j.started.elapsed().as_secs_f64();
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
fn piece_of(key: &str) -> String {
    key.split('|')
        .enumerate()
        .filter(|(i, _)| *i != 1)
        .map(|(_, p)| p)
        .collect::<Vec<_>>()
        .join("|")
}

/// FNV-1a, 64 bits: what names a text in a key.
pub(super) fn fnv(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

fn key_of(w: &TierWant) -> String {
    let dash = || "-".to_owned();
    format!(
        "{}|{}|{}|{}|{}|{}",
        w.tile,
        w.tier,
        w.seed.map_or_else(dash, |s| s.to_string()),
        w.system.map_or_else(dash, |s| s.to_string()),
        w.printer.clone().unwrap_or_else(dash),
        w.text.as_deref().map_or_else(dash, |t| format!(
            "{}:{:016x}",
            w.file.as_deref().unwrap_or("-"),
            fnv(t)
        )),
    )
}

/// A want, checked and ready to build.
struct Ready {
    key: String,
    tile: String,
    path: String,
    opts: Options,
    /// A text to build in place of a file's.
    overlay: Option<edit::Overlay>,
}

impl Shared {
    fn prepare(&self, w: &TierWant) -> Result<Ready, Reply> {
        let Some((path, files)) = self.facts(&w.tile).map(|t| (t.path, t.files)) else {
            return Err(fail::refused("404 Not Found", "no such tile"));
        };
        // the same rule as a draft's: the tile's own document, or a file its last build read
        let overlay = match &w.text {
            None => None,
            Some(text) => {
                let file = super::api::TileRef {
                    id: w.tile.clone(),
                    path: path.clone(),
                    files,
                }
                .file(w.file.as_deref())?;
                Some(edit::Overlay(BTreeMap::from([(file, text.clone())])))
            }
        };
        let mut opts = self.options();
        if w.tier == dt_store::Tier::Draft {
            return Err(fail::refused(
                "400 Bad Request",
                "a tier is low, super-low or full",
            ));
        }
        opts.tier = w.tier;
        if let Some(s) = w.system {
            opts.system = s;
        }
        opts.seed = w.seed.or(opts.seed);
        if let Some(p) = w.printer.as_deref()
            && p != opts.printer.name
            && let Err(e) = opts.use_printer(p, self.home.as_deref())
        {
            return Err(fail::refused(
                "400 Bad Request",
                e.first().map_or_else(String::new, |d| d.message.clone()),
            ));
        }
        Ok(Ready {
            key: key_of(w),
            tile: w.tile.clone(),
            path,
            opts,
            overlay,
        })
    }

    /// Say on the stream that `key` is done at `doc_rev` (or why it could not be, and whether that is a mistake in the
    /// document's text, which is the document's to show, or a failure of the program's).
    fn announce_tier(
        &self,
        key: &str,
        doc_rev: u64,
        error: Option<&str>,
        mistake: bool,
        code: Option<&str>,
    ) {
        self.owner.send(super::owner::Command::Tier {
            key: key.to_owned(),
            doc_rev,
            error: error.map(str::to_owned),
            mistake,
            code: code.map(str::to_owned),
        });
    }

    /// Replace `scope`'s wanted tiers with `wants`: what is no longer wanted by anyone is cancelled, what is wanted and
    /// neither built nor building at the revision of its tile now is queued, and what is already built is announced.
    pub(super) fn want_tiers(
        &self,
        scope: &str,
        doc: Option<String>,
        wants: &[TierWant],
    ) -> Out<Vec<String>> {
        let ready: Vec<Ready> = wants
            .iter()
            .map(|w| self.prepare(w))
            .collect::<Result<_, _>>()?;
        let keys: Vec<String> = ready.iter().map(|r| r.key.clone()).collect();
        let (dropped, old_doc): (Vec<String>, Option<String>) = self.owner.ask(|reply| {
            Command::Book(BookOp::Want {
                scope: scope.to_owned(),
                doc: doc.clone(),
                keys: keys.clone(),
                reply,
            })
        });
        for k in &dropped {
            self.cancel_lane(Lane::Tier, k);
            // withdrawn: its job row ends cleanly, the piece is no longer owed
            if let Some(d) = &old_doc {
                self.job_done(d, k);
            }
        }
        for r in ready {
            let rev = self.rev_of(&r.path);
            let have = self
                .owner
                .ask(|reply| Command::Book(BookOp::Have(r.key.clone(), reply)));
            if have == Some(rev) {
                self.announce_tier(&r.key, rev, None, false, None);
                continue;
            }
            if !self
                .owner
                .ask(|reply| Command::Book(BookOp::Claim(r.key.clone(), reply)))
            {
                continue; // being built; announced when done
            }
            let (key, tile) = (r.key.clone(), r.tile.clone());
            if let Some(d) = &doc {
                self.job_add(d, &key);
            }
            self.submit_lane(
                Lane::Tier,
                key,
                rev,
                &tile,
                Box::new(move |shared, cancel| shared.build_wanted_tier(&r, rev, cancel)),
            );
        }
        if let Some(d) = &doc {
            // said once the wants are all in: the total is what was asked, not the pieces counted one by one
            self.job_tell(d);
        }
        Ok(keys)
    }

    /// The job: build the tier, keep the result and say so; a build that was cancelled says nothing.
    fn build_wanted_tier(&self, r: &Ready, rev: u64, cancel: &Arc<AtomicBool>) {
        self.job_current(&r.key, &r.tile);
        let progress = dt_eval::Progress::new(Arc::new(queue::Cancelled(Arc::clone(cancel))));
        let ran = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let sources: &dyn dt_eval::Source = match &r.overlay {
                Some(o) => o,
                None => &dt_eval::Files,
            };
            dt_store::build_stored(
                sources,
                &r.path,
                &r.opts,
                &progress,
                None,
                self.store.as_deref(),
            )
            .map(|b| {
                if r.opts.tier == dt_store::Tier::Full {
                    dt_eval::tiers::thin_built(b, MAP_FULL_TRIANGLES)
                } else {
                    b
                }
            })
        }));
        let built = match ran {
            Ok(b) => b,
            Err(p) if p.is::<Superseded>() => {
                self.owner
                    .send(Command::Book(BookOp::Release(r.key.clone())));
                self.job_done_everywhere(&r.key);
                return;
            }
            Err(p) => {
                // a bug in a build: said loudly, and the page is told it failed
                let e = fail::ServerError::bug(p.as_ref(), None);
                tell!(
                    "error: preview: the {} build of {} panicked: {}",
                    r.opts.tier.name(),
                    r.path,
                    e.message
                );
                self.owner
                    .send(Command::Book(BookOp::Release(r.key.clone())));
                self.tier_failed(r, &e.message, &[]);
                self.announce_tier(&r.key, rev, Some(&e.message), false, None);
                self.job_done_everywhere(&r.key);
                return;
            }
        };
        // an error is told with whether it is a mistake in the document's text (a history step of a tile with a
        // mistake in it is an ordinary thing to ask for) or a failure of the program's own
        let outcome = match built {
            Err(errs) => Err((
                errs.iter()
                    .map(|d| d.message.clone())
                    .collect::<Vec<_>>()
                    .join("; "),
                errs.iter().all(|d| crate::build::is_document_error(d.code)),
                errs,
            )),
            Ok(b) => payload::for_map(&b, r.opts.tier.name(), rev, &r.opts, &self.palette())
                .map_err(|e| {
                    (
                        format!("cannot prepare the {} view: {e}", r.opts.tier.name()),
                        false,
                        Vec::new(),
                    )
                }),
        };
        match outcome {
            Ok(bytes) => {
                self.owner.send(Command::Book(BookOp::Keep {
                    key: r.key.clone(),
                    rev,
                    bytes: Arc::new(bytes),
                }));
                self.announce_tier(&r.key, rev, None, false, None);
            }
            Err((message, mistake, errs)) => {
                self.owner
                    .send(Command::Book(BookOp::Release(r.key.clone())));
                self.tier_failed(r, &message, &errs);
                // a document's mistake is told with its code, so every picture's caller shows it as the tile's
                let code = errs.first().filter(|_| mistake).map(|d| d.code);
                self.announce_tier(&r.key, rev, Some(&message), mistake, code);
            }
        }
        self.job_done_everywhere(&r.key);
    }

    /// A piece could not be built: an error in the panel of every document that wants it, never only on the stream.
    fn tier_failed(&self, r: &Ready, message: &str, errs: &[dt_eval::Diag]) {
        use crate::message::{Kind, Message, Scope, Severity};
        let docs = self.docs_wanting(&r.key);
        let piece = self.name_of(&r.tile).unwrap_or_else(|| r.tile.clone());
        let doing = {
            let o = &r.opts;
            let seed = o
                .seed
                .map_or_else(|| "its own seed".to_owned(), |s| format!("seed {s}"));
            format!(
                "the {} build of {piece}, for the map or list: system {}, printer {}, {seed}",
                r.opts.tier.name(),
                o.system,
                o.printer.name
            )
        };
        for d in docs {
            let Some(name) = self.name_of(&d) else {
                continue;
            };
            if errs.is_empty() {
                self.say(
                    Message::new(
                        Kind::Server,
                        Severity::Error,
                        Scope::Server,
                        &d,
                        &name,
                        format!(
                            "The {} build of {piece} failed: {message}",
                            r.opts.tier.name()
                        ),
                    )
                    .code("tier")
                    .about(&r.path, doing.clone()),
                );
                continue;
            }
            // the tile's own diagnostics, each with its code, place, source line and help: the piece's mistake, said
            // where the piece is used, never as a failure of the program
            for diag in errs {
                let mut m = Message::from_diag(diag, Scope::Server, false, &d, &name, &[]);
                m.doing = Some(doing.clone());
                m.file.get_or_insert_with(|| r.path.clone());
                self.say(m);
            }
        }
    }

    /// The documents whose scopes want `key`.
    fn docs_wanting(&self, key: &str) -> Vec<String> {
        self.owner
            .ask(|reply| Command::Book(BookOp::Docs(key.to_owned(), reply)))
    }

    // ---- the job: a map's pieces loading, said as the document's work in flight (the owner's: see `BookOp`) ----

    /// A piece was asked for on behalf of `doc`: the job begins with the first, and grows with each.
    fn job_add(&self, doc: &str, key: &str) {
        self.owner.send(Command::Book(BookOp::JobAdd {
            doc: doc.to_owned(),
            key: key.to_owned(),
        }));
    }

    /// The piece `key` is no longer owed to `doc` (built, failed, withdrawn or superseded).
    fn job_done(&self, doc: &str, key: &str) {
        self.owner.send(Command::Book(BookOp::JobDone {
            doc: doc.to_owned(),
            key: key.to_owned(),
        }));
    }

    /// `key` is no longer owed to any document.
    fn job_done_everywhere(&self, key: &str) {
        self.owner
            .send(Command::Book(BookOp::JobDoneEverywhere(key.to_owned())));
    }

    /// The piece `key` (of tile `tile`) is being built now: the job says which.
    fn job_current(&self, key: &str, tile: &str) {
        let name = self.name_of(tile).unwrap_or_else(|| tile.to_owned());
        self.owner.send(Command::Book(BookOp::JobCurrent {
            key: key.to_owned(),
            name,
        }));
    }

    /// The job's progress, as the one record kind of work in flight.
    fn job_tell(&self, doc: &str) {
        self.owner
            .send(Command::Book(BookOp::JobTell(doc.to_owned())));
    }

    /// The kept result for `key`, if it is of revision `doc_rev` (any, when none is asked): `Ok(None)` when there is
    /// none, `Err(now)` when it is of another revision.
    pub(super) fn tier_result(
        &self,
        key: &str,
        doc_rev: Option<u64>,
    ) -> Result<Option<Arc<Vec<u8>>>, u64> {
        self.owner.ask(|reply| {
            Command::Book(BookOp::Result {
                key: key.to_owned(),
                doc_rev,
                reply,
            })
        })
    }
}

/// `POST /api/tiers/want`.
pub(super) fn want(shared: &Shared, req: TiersWantReq) -> Out<TiersWantRes> {
    Ok(TiersWantRes {
        keys: shared.want_tiers(&req.scope, req.doc, &req.wants)?,
    })
}
