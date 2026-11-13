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

use serde::{Deserialize, Serialize};

use super::api::{Out, Reply};
use super::queue::Lane;
use super::*;

/// One tier of one piece's tile that the page wants.
#[derive(Deserialize, Clone)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(optional_fields = nullable))]
#[serde(deny_unknown_fields)]
pub struct TierWant {
    pub tile: String,
    /// `super-low`, `low` or `full`.
    pub tier: String,
    pub seed: Option<u64>,
    pub system: Option<String>,
    pub printer: Option<String>,
}

#[derive(Deserialize)]
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
#[derive(Serialize)]
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

fn key_of(w: &TierWant) -> String {
    let dash = || "-".to_owned();
    format!(
        "{}|{}|{}|{}|{}",
        w.tile,
        w.tier,
        w.seed.map_or_else(dash, |s| s.to_string()),
        w.system.clone().unwrap_or_else(dash),
        w.printer.clone().unwrap_or_else(dash),
    )
}

/// A want, checked and ready to build.
struct Ready {
    key: String,
    tile: String,
    path: String,
    tier: dt_eval::Tier,
    opts: Options,
}

impl Shared {
    fn prepare(&self, w: &TierWant) -> Result<Ready, Reply> {
        let tier = match w.tier.as_str() {
            "low" => dt_eval::Tier::Low,
            "super-low" => dt_eval::Tier::SuperLow,
            "full" => dt_eval::Tier::Full,
            _ => {
                return Err(fail::refused(
                    "400 Bad Request",
                    "a tier is low, super-low or full",
                ));
            }
        };
        let Some(path) = lock(&self.tiles)
            .iter()
            .find(|t| t.id == w.tile)
            .map(|t| t.path.clone())
        else {
            return Err(fail::refused("404 Not Found", "no such tile"));
        };
        let mut opts = self.options();
        match w.system.as_deref() {
            None => {}
            Some(s @ ("db" | "ol")) => opts.system = s.to_owned(),
            Some(_) => return Err(fail::refused("400 Bad Request", "the system is db or ol")),
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
            tier,
            opts,
        })
    }

    /// Say on the stream that `key` is done at `doc_rev` (or why it could not be).
    fn announce_tier(&self, key: &str, doc_rev: u64, error: Option<&str>) {
        let mut ev = lock(&self.events);
        let rev = ev.rev;
        ev.send(&super::api::StreamEvent::Tier {
            rev,
            key: key.to_owned(),
            doc_rev,
            error: error.map(str::to_owned),
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
        let (dropped, old_doc): (Vec<String>, Option<String>) = {
            let mut book = lock(&self.tier_book);
            let old_doc = match &doc {
                Some(d) => book.docs.insert(scope.to_owned(), d.clone()),
                None => book.docs.remove(scope),
            };
            let old = book.wanted.insert(scope.to_owned(), keys.clone());
            let still: BTreeSet<&String> = book.wanted.values().flatten().collect();
            let dropped: Vec<String> = old
                .into_iter()
                .flatten()
                .filter(|k| !still.contains(k))
                .collect();
            for k in &dropped {
                book.inflight.remove(k);
            }
            (dropped, old_doc)
        };
        for k in &dropped {
            self.cancel_lane(Lane::Tier, k);
            // withdrawn: its job row ends cleanly, the piece is no longer owed
            if let Some(d) = &old_doc {
                self.job_done(d, k);
            }
        }
        for r in ready {
            let rev = self.rev_of(&r.path);
            let have = lock(&self.tier_book).results.get(&r.key).map(|d| d.doc_rev);
            if have == Some(rev) {
                self.announce_tier(&r.key, rev, None);
                continue;
            }
            if !lock(&self.tier_book).inflight.insert(r.key.clone()) {
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
            dt_eval::build_tier(
                &dt_eval::Files,
                &r.path,
                &r.opts,
                r.tier,
                &progress,
                None,
                self.store.as_deref(),
            )
            .map(|b| {
                if r.tier == dt_eval::Tier::Full {
                    dt_eval::tiers::thin_built(b, MAP_FULL_TRIANGLES)
                } else {
                    b
                }
            })
        }));
        let built = match ran {
            Ok(b) => b,
            Err(p) if p.is::<Superseded>() => {
                lock(&self.tier_book).inflight.remove(&r.key);
                self.job_done_everywhere(&r.key);
                return;
            }
            Err(p) => {
                // a bug in a build: said loudly, and the page is told it failed
                let e = fail::ServerError::bug(p.as_ref(), None);
                tell!(
                    "error: preview: the {} build of {} panicked: {}",
                    r.tier.name(),
                    r.path,
                    e.message
                );
                lock(&self.tier_book).inflight.remove(&r.key);
                self.tier_failed(r, &e.message);
                self.announce_tier(&r.key, rev, Some(&e.message));
                self.job_done_everywhere(&r.key);
                return;
            }
        };
        let outcome = match built {
            Err(errs) => Err(errs
                .iter()
                .map(|d| d.message.clone())
                .collect::<Vec<_>>()
                .join("; ")),
            Ok(b) => payload::for_map(&b, r.tier.name(), rev, &r.opts, &self.palette())
                .map_err(|e| format!("cannot prepare the {} view: {e}", r.tier.name())),
        };
        let mut book = lock(&self.tier_book);
        book.inflight.remove(&r.key);
        match outcome {
            Ok(bytes) => {
                if book
                    .results
                    .insert(
                        r.key.clone(),
                        Done {
                            doc_rev: rev,
                            bytes: Arc::new(bytes),
                        },
                    )
                    .is_none()
                {
                    book.order.push_back(r.key.clone());
                }
                while book.order.len() > KEPT {
                    if let Some(old) = book.order.pop_front() {
                        book.results.remove(&old);
                    }
                }
                drop(book);
                self.announce_tier(&r.key, rev, None);
            }
            Err(message) => {
                drop(book);
                self.tier_failed(r, &message);
                self.announce_tier(&r.key, rev, Some(&message));
            }
        }
        self.job_done_everywhere(&r.key);
    }

    /// A piece could not be built: an error in the panel of every document that wants it, never only on the stream.
    fn tier_failed(&self, r: &Ready, message: &str) {
        use crate::message::{Kind, Message, Scope, Severity};
        let docs = self.docs_wanting(&r.key);
        let piece = self.name_of(&r.tile).unwrap_or_else(|| r.tile.clone());
        for d in docs {
            let Some(name) = self.name_of(&d) else {
                continue;
            };
            self.say(
                Message::new(
                    Kind::Server,
                    Severity::Error,
                    Scope::Server,
                    &d,
                    &name,
                    format!("The {} build of {piece} failed: {message}", r.tier.name()),
                )
                .code("tier"),
            );
        }
    }

    /// The documents whose scopes want `key`.
    fn docs_wanting(&self, key: &str) -> Vec<String> {
        let book = lock(&self.tier_book);
        let mut docs: Vec<String> = book
            .wanted
            .iter()
            .filter(|(_, keys)| keys.iter().any(|k| k == key))
            .filter_map(|(scope, _)| book.docs.get(scope).cloned())
            .collect();
        docs.sort();
        docs.dedup();
        docs
    }

    // ---- the job: a map's pieces loading, said as the document's work in flight --------------------------

    /// A piece was asked for on behalf of `doc`: the job begins with the first, and grows with each.
    fn job_add(&self, doc: &str, key: &str) {
        let began = {
            let mut book = lock(&self.tier_book);
            let began = !book.jobs.contains_key(doc);
            let job = book.jobs.entry(doc.to_owned()).or_insert_with(|| Job {
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
        };
        if began {
            self.job_begins(doc);
        }
    }

    /// The piece `key` is no longer owed to `doc` (built, failed, withdrawn or superseded).
    fn job_done(&self, doc: &str, key: &str) {
        let ended = {
            let mut book = lock(&self.tier_book);
            let Some(job) = book.jobs.get_mut(doc) else {
                return;
            };
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
                book.jobs.remove(doc);
                true
            } else {
                false
            }
        };
        if ended {
            self.job_ends(doc);
        } else {
            self.job_tell(doc);
        }
    }

    /// `key` is no longer owed to any document.
    fn job_done_everywhere(&self, key: &str) {
        let docs: Vec<String> = lock(&self.tier_book)
            .jobs
            .iter()
            .filter(|(_, j)| j.pending.contains(key))
            .map(|(d, _)| d.clone())
            .collect();
        for d in docs {
            self.job_done(&d, key);
        }
    }

    /// The piece `key` (of tile `tile`) is being built now: the job says which.
    fn job_current(&self, key: &str, tile: &str) {
        let name = self.name_of(tile).unwrap_or_else(|| tile.to_owned());
        let docs: Vec<String> = {
            let mut book = lock(&self.tier_book);
            book.jobs
                .iter_mut()
                .filter(|(_, j)| j.pending.contains(key))
                .map(|(d, j)| {
                    j.current = Some(name.clone());
                    d.clone()
                })
                .collect()
        };
        for d in docs {
            self.job_tell(&d);
        }
    }

    /// The job's progress, as the one record kind of work in flight: pieces done of pieces asked, the piece being
    /// built, and an estimate once one piece is done.
    fn job_tell(&self, doc: &str) {
        // said with the book held, so two threads' words about one job never reach the stream out of order
        let book = lock(&self.tier_book);
        let body = {
            let Some(j) = book.jobs.get(doc) else {
                return;
            };
            let elapsed = j.started.elapsed().as_secs_f64();
            #[allow(clippy::cast_precision_loss)]
            let (done, total) = (j.done as f64, j.total.max(1) as f64);
            let estimate = (j.done > 0).then(|| elapsed / done * total);
            api::Progress {
                what: "loading".to_owned(),
                fraction: done / total,
                label: j.current.clone().unwrap_or_else(|| "pieces".to_owned()),
                key: "pieces".to_owned(),
                count: Some(("pieces".to_owned(), j.done, j.total)),
                elapsed_s: elapsed,
                eta_s: estimate.map(|e| (e - elapsed).max(0.0)),
                estimate_s: estimate,
            }
        };
        self.tell_progress(doc, body, true);
    }

    /// The job's running message (dropped when it ends, never kept), like a build's.
    fn job_begins(&self, doc: &str) {
        use crate::message::{Kind, Message, Scope, Severity};
        let Some(name) = self.name_of(doc) else {
            return;
        };
        let mut ev = lock(&self.events);
        let m = Message::new(
            Kind::Progress,
            Severity::Info,
            Scope::Build,
            doc,
            &name,
            format!("Loading the pieces of {name}"),
        )
        .code(crate::message::JOB);
        let m = ev.messages.begin_progress(fail::now_ms(), m);
        Self::send_messages(&mut ev, &[m]);
    }

    /// The job is done: its message goes, and the document says it has no work in flight.
    fn job_ends(&self, doc: &str) {
        {
            let mut ev = lock(&self.events);
            let gone = ev.messages.end_job(doc);
            Self::send_messages(&mut ev, &gone);
        }
        // the job's words go; a document that is itself building keeps the progress of its own build
        self.update(doc, |s| {
            if s.progress.as_ref().is_some_and(|p| p.what == "loading") {
                s.progress = None;
            }
        });
    }

    /// The kept result for `key`, if it is of revision `doc_rev` (any, when none is asked): `Ok(None)` when there is
    /// none, `Err(now)` when it is of another revision.
    pub(super) fn tier_result(
        &self,
        key: &str,
        doc_rev: Option<u64>,
    ) -> Result<Option<Arc<Vec<u8>>>, u64> {
        let book = lock(&self.tier_book);
        match book.results.get(key) {
            None => Ok(None),
            Some(d) if doc_rev.is_none_or(|r| r == d.doc_rev) => Ok(Some(Arc::clone(&d.bytes))),
            Some(d) => Err(d.doc_rev),
        }
    }
}

/// `POST /api/tiers/want`.
pub(super) fn want(shared: &Shared, req: TiersWantReq) -> Out<TiersWantRes> {
    Ok(TiersWantRes {
        keys: shared.want_tiers(&req.scope, req.doc, &req.wants)?,
    })
}
