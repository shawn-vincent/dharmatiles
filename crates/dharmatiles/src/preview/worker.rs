//! The worker: builds the tiles one at a time and rebuilds on change.

use super::*;
use crate::message::{Kind, Scope, Severity};

/// A progress reporter that also notices when any source read by the plan
/// pass changes. Progress events are safe interruption boundaries in the
/// otherwise synchronous geometry pipeline.
struct LatestBuild {
    tracker: Option<Arc<Tracker>>,
    files: BTreeMap<String, Option<SystemTime>>,
    /// Where sketches go (none in tests), with the tile's id.
    sketches: Option<(Arc<Shared>, TileId)>,
    /// Set when the build is moot: its tile left the list.
    cancel: Arc<AtomicBool>,
    /// The stage running now, which a sketch belongs to.
    stage: Mutex<String>,
}

impl LatestBuild {
    fn changed(&self) -> bool {
        self.cancel.load(Ordering::Relaxed) || self.files.iter().any(|(f, at)| mtime(f) != *at)
    }
}

impl dt_kit::Reporter for LatestBuild {
    fn report(&self, event: &dt_kit::Event<'_>) {
        if self.changed() {
            std::panic::resume_unwind(Box::new(Superseded));
        }
        if let dt_kit::Event::Begin { key, .. } = event {
            (*key).clone_into(&mut *lock(&self.stage));
        }
        if let Some(t) = &self.tracker {
            dt_kit::Reporter::report(t.as_ref(), event);
        }
    }

    fn sketching(&self) -> bool {
        self.sketches.is_some()
    }

    fn sketch(&self, mesh: &dt_kit::TriMesh, materials: &[dt_kit::Material], rough: bool) {
        let Some((shared, id)) = &self.sketches else {
            return;
        };
        if self.changed() {
            std::panic::resume_unwind(Box::new(Superseded));
        }
        // a sketch that cannot be encoded is a bug: it unwinds into the build's
        // catch, which shows it as a failed build (never an empty sketch)
        let bytes = match payload::sketch(mesh, materials, rough, &shared.palette()) {
            Ok(bytes) => bytes,
            Err(e) => panic!("a sketch could not be encoded: {e}"),
        };
        let stage = lock(&self.stage).clone();
        shared.add_sketch(id, &stage, rough, bytes);
    }
}

/// The failure a caught panic becomes for the page: not a mistake in the
/// document but a bug, with the panic's own words.  Logged here too (the
/// panic hook has already written the message and where it happened).
fn panicked(what: &str, panic: &(dyn std::any::Any + Send)) -> (Diag, fail::ServerError) {
    let msg = panic_message(panic);
    tell!("error: the build of {what} panicked: {msg}");
    let diag = Diag::plain("dt::panic", format!("the build panicked: {msg}")).help(
        "this is a bug in dharmatiles, not in your tile: the terminal has the location; the tile builds again when it changes",
    );
    let doing = fail::Doing {
        route: Some("build worker".to_owned()),
        tile: Some(what.to_owned()),
    };
    (diag, fail::ServerError::bug(panic, Some(doing)))
}

/// A build step that failed without a panic, as the page gets it.
fn build_failure(what: &str, message: String) -> fail::ServerError {
    fail::ServerError::failure("500 Internal Server Error", message)
        .doing("build worker", Some(what))
}

/// Build tile `id`: plan pass, then geometry, pushing each result.
pub(super) fn rebuild(shared: &Arc<Shared>, id: &str, cancel: Arc<AtomicBool>, opts: &Options) {
    let Some(path) = lock(&shared.tiles)
        .iter()
        .find(|t| t.id == id)
        .map(|t| t.path.clone())
    else {
        return;
    };
    // the stream says a build is under way until this function ends, however it does
    let _building = shared.building(id);
    shared.update(id, |t| {
        t.status = State::Building;
        t.stale = false;
        t.errors.clear();
        t.failure = None;
        t.progress = None;
        t.sketches.clear();
    });
    if path.to_ascii_lowercase().ends_with(".stl") {
        return rebuild_stl(shared, id, opts, &path);
    }
    if super::plates::is_plates(&path) {
        return super::plates::rebuild(shared, id, &path);
    }
    if super::inventory::is_inventory(&path) {
        return super::inventory::rebuild(shared, id, &path);
    }
    let rec = Recording::watching(&path);
    // every file the build reads, with its text: what the disk cache's key covers
    let tracked = dt_eval::Tracked::new(&rec);
    let t0 = Instant::now();
    // the plan's stack, once it is known: a mistake in the build finds the entry it is in by it
    let plan_stack: std::cell::RefCell<Vec<Value>> = std::cell::RefCell::new(Vec::new());
    let finish_with =
        |rec: Recording, errs: &[Diag], failure: Option<fail::ServerError>, scope: Scope| {
            let errors = super::api::mistakes(errs);
            let failure = failure.map(|f| f.detail());
            // said before the tile is: the page sees what this report cleared while the build still shows as running
            shared.report(id, scope, |name| {
                let mut found = messages::of_errors(errs, scope, id, name, &plan_stack.borrow());
                if let Some(f) = &failure {
                    for m in found.iter_mut().filter(|m| m.kind == Kind::Server) {
                        m.data =
                            messages::of_server_error(&fail::to_value(f), (id, name), "server")
                                .data;
                    }
                }
                found
            });
            shared.update(id, |t| {
                t.status = State::Failed;
                t.progress = None;
                t.sketches.clear();
                t.errors = errors;
                t.failure = failure;
                t.files = rec.files();
                // a build the cache kept was only on show until this one landed
                if t.shown.take().is_some() {
                    t.mesh = None;
                }
            });
        };
    let finish_err = |rec: Recording, errs: &[Diag]| finish_with(rec, errs, None, Scope::Check);
    // plan pass: no geometry, so under a second
    let checked = match dt_eval::check(&tracked, &path, opts) {
        Ok(c) => c,
        Err(errs) => return finish_err(rec, &errs),
    };
    let plan = match checked.tile.as_ref().map(|t| {
        let frame = dt_kit::Frame::for_system(&opts.system, t.cols, t.rows);
        let [w, h] = frame.size_mm();
        Ok::<_, String>(json!({
            "cols": t.cols, "rows": t.rows,
            "square_mm": frame.square_mm, "size_mm": [w, h],
            // the tile's zero height: where the base meets the tile body
            "datum_z": 0.0,
            "stack": stack::items(t, &opts.system)?,
        }))
    }) {
        Some(Err(e)) => {
            let message = format!("cannot describe the stack: {e}");
            return finish_with(
                rec,
                &[Diag::plain("dt::stack", message.clone())],
                Some(build_failure(&path, message)),
                Scope::Check,
            );
        }
        Some(Ok(p)) => Some(p),
        None => None,
    };
    if let Some(p) = &plan {
        shared.update(id, |t| t.plan = Some(p.clone()));
    }
    // what the plan pass found: the notes (dt::steep, dt::edge) that never reached the page before
    let stack: Vec<Value> = plan
        .as_ref()
        .and_then(|p| p["stack"].as_array().cloned())
        .unwrap_or_default();
    plan_stack.replace(stack.clone());
    shared.report(id, Scope::Check, |name| {
        messages::of_check(&checked, id, name, &stack)
    });
    if let Some(store) = shared.store.as_deref() {
        show_stored(shared, id, store, &tracked.reads(), &path, opts);
    }
    // the progress bar: stages weighted by what the last build of this
    // tile measured, else by the elements' defaults
    let mut timings = shared
        .cache
        .as_ref()
        .map_or_else(Timings::default, |f| Timings::load(f));
    let mut stages = plan_stages(&checked, &path, opts, &timings);
    stages.push(StageInfo::new(
        ENCODE,
        "preparing the view",
        0.4,
        &path,
        &opts.system,
        &timings,
    ));
    drop(checked);
    let tracker = (stages.len() > 1).then(|| {
        let sink = shared.sink(id);
        Arc::new(Tracker::new(stages, Box::new(move |s| sink(s))))
    });
    let latest = Arc::new(LatestBuild {
        tracker: tracker.clone(),
        files: watched(&rec, shared),
        sketches: Some((Arc::clone(shared), id.to_owned())),
        cancel,
        stage: Mutex::new(String::new()),
    });
    let reporter = dt_eval::Progress::new(latest.clone());
    let cache = shared.build_cache(&path, false);
    let mut bug = None;
    let built = match std::panic::catch_unwind(AssertUnwindSafe(|| {
        #[cfg(test)]
        assert!(
            !path.contains("dt-test-panic-in-build"),
            "a test panic in the build"
        );
        dt_eval::build_stored(
            &tracked,
            &path,
            opts,
            &reporter,
            Some(&cache),
            shared.store.as_deref(),
        )
    })) {
        Ok(built) => built,
        Err(panic) if panic.is::<Superseded>() => {
            tell!("cancelled superseded build of {path}");
            shared.update(id, |t| {
                t.progress = None;
                t.files = latest.files.clone();
            });
            return;
        }
        Err(panic) => {
            let (diag, failure) = panicked(&path, panic.as_ref());
            bug = Some(failure);
            Err(vec![diag])
        }
    };
    if latest.changed() {
        tell!("discarded superseded build of {path}");
        shared.update(id, |t| {
            t.progress = None;
            t.files = latest.files.clone();
        });
        return;
    }
    match built {
        Err(errs) => finish_with(rec, &errs, bug, Scope::Build),
        Ok(b) => {
            let built_s = t0.elapsed().as_secs_f64();
            let failures = b.audit.failures(&opts.printer);
            let mut record = tile_json(&b, &failures, built_s);
            record["cache"] = json!(match b.cache.state {
                dt_eval::CacheState::Off => "off",
                dt_eval::CacheState::Miss => "miss",
                dt_eval::CacheState::Hit => "hit",
            });
            for problem in &b.cache.problems {
                tell!("warning: {path}: the build cache: {problem}");
            }
            // Cancellation panics are caught only around build_with. Encoding
            // uses the ordinary tracker and explicit changed checks instead.
            let encode_progress = tracker.as_ref().map_or_else(dt_eval::Progress::none, |t| {
                dt_eval::Progress::new(t.clone())
            });
            #[cfg(test)]
            assert!(
                !path.contains("dt-test-panic-in-encode"),
                "a test panic in the encoding"
            );
            encode_progress.begin(ENCODE, "preparing the view");
            let t_encode = Instant::now();
            let mesh = match payload::from_built(&b, opts.printer.overhang_deg, &shared.palette()) {
                Ok(m) => Arc::new(m),
                Err(e) => {
                    let message = format!("cannot prepare the view: {e}");
                    return finish_with(
                        rec,
                        &[Diag::plain("dt::payload", message.clone())],
                        Some(build_failure(&path, message)),
                        Scope::Build,
                    );
                }
            };
            if latest.changed() {
                tell!("discarded superseded view of {path}");
                shared.update(id, |t| {
                    t.progress = None;
                    t.files = latest.files.clone();
                });
                return;
            }
            encode_progress.end(ENCODE);
            if let Some(t) = &tracker {
                t.finish();
            }
            if shared.cache.is_some() {
                let mut secs = b.stage_secs.clone();
                secs.push((ENCODE.to_owned(), t_encode.elapsed().as_secs_f64()));
                timings.record(&path, &opts.system, &secs);
                timings.save();
            }
            let reused = match b.reused {
                _ if b.cache.state == dt_eval::CacheState::Hit => " (from the cache)".to_owned(),
                0 => String::new(),
                n => format!(
                    " ({n} of {} steps kept from the last build)",
                    n + b.stage_secs.len() - 2
                ),
            };
            tell!(
                "built {path}: {} triangles, overhang {:.2} mm², {built_s:.2} s{reused}",
                b.audit.triangles,
                b.audit.overhang_mm2
            );
            // said before the tile is: what this build no longer reports clears while it still shows as running
            let palette = shared.palette_diags();
            shared.report(id, Scope::Build, |name| {
                let mut found = messages::of_build(&record, b.reused, &b.cache, id, name);
                for m in messages::of_errors(&palette, Scope::Build, id, name, &[]) {
                    found.push(crate::message::Message {
                        severity: Severity::Warning,
                        ..m
                    });
                }
                found
            });
            shared.update(id, |t| {
                t.status = State::Ready;
                // what to say beside a good build: the palette
                t.errors = shared.palette_problem();
                t.progress = None;
                t.sketches.clear();
                t.record = Some(record);
                t.mesh = Some(mesh);
                t.mesh_version += 1;
                t.shown = None;
                t.files = watched(&rec, shared);
            });
        }
    }
}

/// A tile about to be built whose build is not in the disk cache but has
/// something there to look at: an earlier build of it (it has changed since)
/// or the library's stored preview.  Shown at once, with the tile still
/// building and `shown` saying what it is, so the page marks it "rebuilding";
/// said in the messages too, and replaced by the build's own report.  Only
/// when nothing is on show yet (a tile already showing its last build keeps
/// it).  A damaged entry met on the way is reported, never skipped.
pub(super) fn show_stored(
    shared: &Arc<Shared>,
    id: &str,
    store: &dt_eval::Store,
    reads: &dt_eval::store::Reads,
    path: &str,
    opts: &Options,
) {
    use dt_eval::store::Tier;
    let key = store.key(reads, path, opts);
    if store.contains(&key)
        || lock(&shared.tiles)
            .iter()
            .any(|t| t.id == id && t.mesh.is_some())
    {
        return;
    }
    let want = dt_kit::debug::Collect {
        clouds: true,
        marks: opts.debug,
    };
    let problem = |e: String| {
        tell!("warning: {path}: the build cache: {e}");
        shared.report(id, Scope::Build, |name| {
            vec![messages::of_cache_problem(&e, id, name)]
        });
    };
    let found = match store.previous(&key, want) {
        Ok(Some(p)) => {
            let note = format!(
                "the build made by {} {}",
                p.made_by,
                crate::cache::age_text(p.made_at)
            );
            Some((p.built, note, "earlier", false))
        }
        // no earlier full build: a super-low (or a low) one, not yet checked
        Ok(None) => {
            let mut found = None;
            for tier in [Tier::SuperLow, Tier::Low] {
                match store.get(
                    &store.key_at(reads, path, opts, tier),
                    dt_kit::debug::Collect::default(),
                ) {
                    Ok(Some(b)) => {
                        found = Some((
                            b,
                            format!("the stored {} preview", tier.name()),
                            "preview",
                            true,
                        ));
                        break;
                    }
                    Ok(None) => {}
                    Err(e) => problem(e),
                }
            }
            found
        }
        Err(e) => {
            problem(e);
            None
        }
    };
    let Some((built, note, kind, draft)) = found else {
        return;
    };
    let palette = shared.palette();
    let bytes = payload::from_built(&built, opts.printer.overhang_deg, &palette);
    // a view that cannot be made is not an error of the tile's: the build
    // that follows shows its own, so say it and show nothing
    let bytes = match bytes {
        Ok(b) => b,
        Err(e) => return problem(format!("cannot prepare the kept build for the view: {e}")),
    };
    shared.report(id, Scope::Build, |name| {
        vec![messages::of_shown(&note, kind, id, name)]
    });
    shared.update(id, |t| {
        if t.mesh.is_none() {
            t.mesh = Some(Arc::new(bytes));
            t.mesh_version += 1;
            // a low or super-low build is not yet checked: the status line says so
            t.shown = Some(json!({"kind": kind, "note": note, "checked": !draft,
                                  "tier": if draft { "low" } else { "full" }}));
        }
    });
}

/// What a tile's build depends on: the files it read, and the palette.
fn watched(rec: &Recording, shared: &Shared) -> BTreeMap<String, Option<std::time::SystemTime>> {
    let mut files = rec.files();
    if let Some(f) = &shared.palette {
        let f = f.to_string_lossy().into_owned();
        let m = mtime(&f);
        files.insert(f, m);
    }
    files
}

/// An STL file: the gate runs on it as a slicer would read it, and its
/// overhangs are painted (the earth colour for everything else).  Lets you
/// look at any printable mesh, not only a document's.
pub(super) fn rebuild_stl(shared: &Shared, id: &str, opts: &Options, path: &str) {
    let t0 = Instant::now();
    let files = BTreeMap::from([(path.to_owned(), mtime(path))]);
    let fail = |msg: String| {
        shared.report(id, Scope::Build, |name| {
            messages::of_errors(
                &[Diag::plain("dt::source", msg.clone())],
                Scope::Build,
                id,
                name,
                &[],
            )
        });
        shared.update(id, |t| {
            t.status = State::Failed;
            t.errors = super::api::mistakes(&[Diag::plain("dt::source", msg)]);
            t.files = files.clone();
        });
    };
    let mesh = match std::fs::read(path) {
        Err(e) => return fail(format!("cannot read {path}: {e}")),
        Ok(b) => match crate::read_stl(&b) {
            Some(m) => m,
            None => return fail(format!("{path} is not a binary STL")),
        },
    };
    let footprint = crate::stl_footprint(&mesh, None, None);
    let (export, a) = dt_kit::audit_rounded(
        &mesh,
        footprint,
        crate::stl_corner_round(footprint),
        &opts.printer,
        &[],
        Some(&dt_kit::HangBand::DEFAULT),
    );
    let failures = a.failures(&opts.printer);
    let materials = vec![dt_kit::Material::Earth; export.mesh.triangles.len()];
    let mut painted = dt_kit::paint(&materials, &a.overhang);
    for (c, _) in painted.iter_mut().zip(&a.band).filter(|(_, b)| **b) {
        *c = dt_kit::ORANGE;
    }
    let clean = dt_kit::paint(&materials, &vec![false; materials.len()]);
    let meta = json!({"overhang_deg": opts.printer.overhang_deg, "footprint": footprint});
    let bytes = match payload::encode(&export.mesh, &painted, &clean, &meta) {
        Ok(b) => Arc::new(b),
        Err(e) => return fail(format!("cannot prepare the view of {path}: {e}")),
    };
    let sq = dt_kit::Frame::for_system(&opts.system, 1, 1).square_mm;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let n = |v: f64| (libm::round(v / sq).max(1.0)) as u32;
    let plan = json!({
        "tile": path.rsplit('/').next().unwrap_or(path), "cols": n(footprint[0]), "rows": n(footprint[1]),
        "square_mm": sq, "size_mm": footprint, "datum_z": Value::Null,
        "stack": [{"label": "stl file", "element": "stl", "href": Value::Null}],
    });
    let record = json!({
        "record": {"seed": Value::Null, "system": opts.system},
        "audit": {
            "triangles": a.triangles, "watertight": a.watertight, "overhang_mm2": a.overhang_mm2,
            "overhang_budget_mm2": a.overhang_budget_mm2, "band_mm2": a.band_mm2, "band_budget_mm2": a.band_budget_mm2, "dropped": a.dropped.len(),
            "voids_filled": a.voids.len(), "holes": a.holes, "failures": failures,
        },
        "build_s": t0.elapsed().as_secs_f64(),
    });
    shared.report(id, Scope::Build, |name| messages::of_stl(&record, id, name));
    shared.update(id, |t| {
        t.status = State::Ready;
        t.plan = Some(plan);
        t.record = Some(record);
        t.mesh = Some(bytes);
        t.mesh_version += 1;
        t.files = files;
    });
}

/// A draft: the text the page holds, built quickly (see
/// [`dt_eval::Options::draft`]) and answered to whoever asked.  A newer draft
/// of the same tile, or the tile leaving the list, stops it at its next
/// progress report.
fn draft(shared: &Arc<Shared>, d: &queue::Draft, opts: &Options) {
    use queue::Outcome;
    let opts = Options {
        draft: true,
        ..opts.clone()
    };
    let src = edit::Overlay(BTreeMap::from([(d.file.clone(), d.text.clone())]));
    let progress = dt_eval::Progress::new(Arc::new(Cancelled(Arc::clone(&d.cancel))));
    let cache = shared.build_cache(&d.path, true);
    // the build and its encoding together: a panic in either must not end
    // the lane, or builds stop for the rest of the run
    let made = std::panic::catch_unwind(AssertUnwindSafe(|| {
        let b = dt_eval::build_cached(&src, &d.path, &opts, &progress, Some(&cache))?;
        Ok::<_, Vec<Diag>>(payload::from_draft(
            &b,
            opts.printer.overhang_deg,
            &shared.palette(),
        ))
    }));
    let outcome = match made {
        Err(panic) if panic.is::<Superseded>() => Outcome::Superseded,
        Err(panic) => {
            let msg = panic_message(panic.as_ref());
            tell!("error: the draft of {} panicked: {msg}", d.path);
            let doing = fail::Doing {
                route: Some("draft worker".to_owned()),
                tile: Some(d.path.clone()),
            };
            Outcome::Failed(Box::new(fail::ServerError::bug(
                panic.as_ref(),
                Some(doing),
            )))
        }
        // a mistake in the text: the check shows it, the last mesh stays
        Ok(Err(_)) => Outcome::Mistake,
        Ok(Ok(_)) if d.cancel.load(Ordering::Relaxed) => Outcome::Superseded,
        Ok(Ok(Err(why))) => Outcome::Failed(Box::new(
            fail::ServerError::failure(
                "500 Internal Server Error",
                format!("cannot prepare the draft's view: {why}"),
            )
            .doing("draft worker", Some(&d.path)),
        )),
        Ok(Ok(Ok(mesh))) => Outcome::Mesh(Arc::new(mesh)),
    };
    // Allowed: the requester gave up waiting (its request timed out or its
    // connection closed); there is nobody left to tell.
    let _ = d.reply.send(outcome);
}

/// A draft's reporter: stops the build once its token is set.
struct Cancelled(Arc<AtomicBool>);

impl dt_kit::Reporter for Cancelled {
    fn report(&self, _event: &dt_kit::Event<'_>) {
        if self.0.load(Ordering::Relaxed) {
            std::panic::resume_unwind(Box::new(Superseded));
        }
    }
}

/// One worker lane: take jobs from the queue until `stop`.
fn lane(shared: &Arc<Shared>, eager: bool, stop: &AtomicBool) {
    while !stop.load(Ordering::Relaxed) {
        shared.rescan_due();
        let Some(job) = shared.next_job(eager) else {
            continue;
        };
        // the target the page chose now, for this job; the clouds are always
        // collected (a selected top= or pin is drawn from them), the debug
        // layer's marks only when the page has the layer on for the tile
        let opts = Options {
            clouds: true,
            ..shared.options()
        };
        match &job {
            queue::Job::Draft(d) => draft(
                shared,
                d,
                &Options {
                    debug: d.marks,
                    ..opts
                },
            ),
            queue::Job::Full(id, cancel) => {
                let opts = Options {
                    debug: shared.marks_of(id),
                    ..opts
                };
                // `rebuild` catches a panic in the build itself; this catches
                // one anywhere else in it (encoding, bookkeeping), so the lane
                // goes on and the page is told
                let ran = std::panic::catch_unwind(AssertUnwindSafe(|| {
                    rebuild(shared, id, Arc::clone(cancel), &opts);
                }));
                if let Err(panic) = ran
                    && !panic.is::<Superseded>()
                {
                    let what = lock(&shared.tiles)
                        .iter()
                        .find(|t| t.id == *id)
                        .map_or_else(|| id.clone(), |t| t.path.clone());
                    let (d, failure) = panicked(&what, panic.as_ref());
                    shared.report(id, Scope::Build, |name| {
                        let mut found = messages::of_errors(
                            std::slice::from_ref(&d),
                            Scope::Build,
                            id,
                            name,
                            &[],
                        );
                        for m in &mut found {
                            m.data =
                                messages::of_server_error(&failure.json(), (id, name), "server")
                                    .data;
                        }
                        found
                    });
                    shared.update(id, |t| {
                        t.status = State::Failed;
                        t.progress = None;
                        t.sketches.clear();
                        t.errors = super::api::mistakes(&[d]);
                        t.failure = Some(failure.detail());
                    });
                }
            }
        }
        shared.finish(&job);
    }
}

/// The workers: two lanes on one queue, so a draft never waits for a full
/// build (and no two full builds run at once), until `stop`.
pub fn work(shared: &Arc<Shared>, eager: bool, stop: &AtomicBool) {
    std::thread::scope(|s| {
        s.spawn(|| lane(shared, eager, stop));
        lane(shared, eager, stop);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_changed_source_cancels_at_the_next_progress_boundary() {
        let missing = "/a/path/that/dharmatiles-does-not-have".to_owned();
        let latest = LatestBuild {
            tracker: None,
            files: BTreeMap::from([(missing, Some(SystemTime::UNIX_EPOCH))]),
            sketches: None,
            cancel: Arc::new(AtomicBool::new(false)),
            stage: Mutex::new(String::new()),
        };
        assert!(latest.changed());
        let stopped = std::panic::catch_unwind(AssertUnwindSafe(|| {
            dt_kit::Reporter::report(
                &latest,
                &dt_kit::Event::Begin {
                    key: "test",
                    label: "test",
                },
            );
        }));
        assert!(stopped.is_err_and(|p| p.is::<Superseded>()));
    }
}
