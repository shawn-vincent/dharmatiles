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
#[serde(deny_unknown_fields)]
pub struct TiersWantReq {
    /// Whose wants these are (a map, the picture shelf): a post replaces the last of the same scope.
    pub scope: String,
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
}

/// How many results are kept (the oldest go first).
const KEPT: usize = 300;

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
        ev.send(
            &json!({"type": "tier", "rev": rev, "key": key, "docRev": doc_rev, "error": error}),
        );
    }

    /// Replace `scope`'s wanted tiers with `wants`: what is no longer wanted by anyone is cancelled, what is wanted and
    /// neither built nor building at the revision of its tile now is queued, and what is already built is announced.
    pub(super) fn want_tiers(&self, scope: &str, wants: &[TierWant]) -> Out<Vec<String>> {
        let ready: Vec<Ready> = wants
            .iter()
            .map(|w| self.prepare(w))
            .collect::<Result<_, _>>()?;
        let keys: Vec<String> = ready.iter().map(|r| r.key.clone()).collect();
        let dropped: Vec<String> = {
            let mut book = lock(&self.tier_book);
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
            dropped
        };
        for k in &dropped {
            self.cancel_lane(Lane::Tier, k);
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
            self.submit_lane(
                Lane::Tier,
                key,
                rev,
                &tile,
                Box::new(move |shared, cancel| shared.build_wanted_tier(&r, rev, cancel)),
            );
        }
        Ok(keys)
    }

    /// The job: build the tier, keep the result and say so; a build that was cancelled says nothing.
    fn build_wanted_tier(&self, r: &Ready, rev: u64, cancel: &Arc<AtomicBool>) {
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
                self.announce_tier(&r.key, rev, Some(&e.message));
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
                self.announce_tier(&r.key, rev, Some(&message));
            }
        }
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
        keys: shared.want_tiers(&req.scope, &req.wants)?,
    })
}
