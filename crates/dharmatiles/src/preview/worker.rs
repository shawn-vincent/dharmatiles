//! The worker: builds the tiles one at a time and rebuilds on change.

use super::*;

/// Build tile `i`: plan pass, then geometry, pushing each result.
pub(super) fn rebuild(shared: &Arc<Shared>, i: usize, opts: &Options) {
    let Some(path) = lock(&shared.tiles).get(i).map(|t| t.path.clone()) else {
        return;
    };
    shared.update(i, |t| {
        t.status = "building";
        t.errors.clear();
        t.progress = None;
    });
    if path.to_ascii_lowercase().ends_with(".stl") {
        return rebuild_stl(shared, i, opts, &path);
    }
    let rec = Recording::watching(&path);
    let t0 = Instant::now();
    let finish_err = |rec: Recording, errs: &[Diag]| {
        let errors = errors_json(errs);
        shared.update(i, |t| {
            t.status = "failed";
            t.progress = None;
            t.errors = errors;
            t.files = rec.files();
        });
    };
    // plan pass: no geometry, so under a second
    let checked = match dt_eval::check(&rec, &path, opts) {
        Ok(c) => c,
        Err(errs) => return finish_err(rec, &errs),
    };
    let plan = checked.tile.as_ref().map(|t| {
        let frame = dt_kit::Frame::for_system(&opts.system, t.cols, t.rows);
        let [w, h] = frame.size_mm();
        json!({
            "tile": t.name, "cols": t.cols, "rows": t.rows,
            "square_mm": frame.square_mm, "size_mm": [w, h],
            // the tile's zero height: where the base meets the tile body
            "datum_z": 0.0,
            "stack": stack::items(t, &opts.system),
        })
    });
    if let Some(p) = &plan {
        let name = p["tile"].as_str().unwrap_or_default().to_owned();
        shared.update(i, |t| {
            t.plan = Some(p.clone());
            t.name = name;
        });
    }
    // the progress bar: stages weighted by what the last build of this
    // tile measured, else by the looks' defaults
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
        let sink = shared.sink(i);
        Arc::new(Tracker::new(stages, Box::new(move |s| sink(s))))
    });
    let reporter = tracker.as_ref().map_or_else(dt_eval::Progress::none, |t| {
        dt_eval::Progress::new(t.clone())
    });
    let built = std::panic::catch_unwind(AssertUnwindSafe(|| {
        dt_eval::build_with(&rec, &path, opts, &reporter)
    }))
    .unwrap_or_else(|_| {
        Err(vec![Diag::plain(
            "dt::panic",
            "the build panicked (a bug in an element); see the terminal".to_owned(),
        )])
    });
    match built {
        Err(errs) => finish_err(rec, &errs),
        Ok(b) => {
            let built_s = t0.elapsed().as_secs_f64();
            let failures = b.audit.failures(&opts.printer);
            let record = tile_json(&b, &failures, built_s);
            reporter.begin(ENCODE, "preparing the view");
            let t_encode = Instant::now();
            let mesh = Arc::new(payload::from_built(
                &b,
                opts.printer.overhang_deg,
                &shared.palette(),
            ));
            reporter.end(ENCODE);
            if let Some(t) = &tracker {
                t.finish();
            }
            if let Some(f) = &shared.cache {
                let mut secs = b.stage_secs.clone();
                secs.push((ENCODE.to_owned(), t_encode.elapsed().as_secs_f64()));
                timings.record(&path, &opts.system, &secs);
                let _ = f;
                timings.save();
            }
            tell!(
                "built {path}: {} triangles, overhang {:.2} mm², {built_s:.2} s",
                b.audit.triangles,
                b.audit.overhang_mm2
            );
            shared.update(i, |t| {
                t.status = "ready";
                t.progress = None;
                t.record = Some(record);
                t.mesh = Some(mesh);
                t.mesh_version += 1;
                t.files = rec.files();
                if let Some(f) = &shared.palette {
                    let f = f.to_string_lossy().into_owned();
                    let m = mtime(&f);
                    t.files.insert(f, m);
                }
            });
        }
    }
}

/// An STL file: the gate runs on it as a slicer would read it, and its
/// overhangs are painted (the earth colour for everything else).  Lets you
/// look at any printable mesh, not only a document's.
pub(super) fn rebuild_stl(shared: &Shared, i: usize, opts: &Options, path: &str) {
    let t0 = Instant::now();
    let files = BTreeMap::from([(path.to_owned(), mtime(path))]);
    let fail = |msg: String| {
        shared.update(i, |t| {
            t.status = "failed";
            t.errors = errors_json(&[Diag::plain("dt::source", msg)]);
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
    let (export, a) = dt_kit::audit(&mesh, footprint, &opts.printer);
    let failures = a.failures(&opts.printer);
    let materials = vec![dt_kit::Material::Earth; export.mesh.triangles.len()];
    let mut painted = dt_kit::paint(&materials, &a.overhang);
    for (c, _) in painted.iter_mut().zip(&a.band).filter(|(_, b)| **b) {
        *c = dt_kit::ORANGE;
    }
    let clean = dt_kit::paint(&materials, &vec![false; materials.len()]);
    let meta = json!({"overhang_deg": opts.printer.overhang_deg, "footprint": footprint});
    let bytes = Arc::new(payload::encode(&export.mesh, &painted, &clean, &meta));
    let sq = if opts.system == "ol" { 25.4 } else { 35.0 };
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let n = |v: f64| (libm::round(v / sq).max(1.0)) as u32;
    let plan = json!({
        "tile": path.rsplit('/').next().unwrap_or(path), "cols": n(footprint[0]), "rows": n(footprint[1]),
        "square_mm": sq, "size_mm": footprint, "datum_z": Value::Null,
        "stack": [{"label": "stl file", "look": "stl", "href": Value::Null}],
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
    shared.update(i, |t| {
        t.status = "ready";
        t.plan = Some(plan);
        t.record = Some(record);
        t.mesh = Some(bytes);
        t.mesh_version += 1;
        t.files = files;
    });
}

/// The worker: build what is wanted, then what changed, until `stop`.
pub fn work(shared: &Arc<Shared>, opts: &Options, eager: bool, stop: &AtomicBool) {
    while !stop.load(Ordering::Relaxed) {
        shared.rescan_due();
        match shared.pick(eager) {
            Some(i) => rebuild(shared, i, opts),
            None => std::thread::sleep(POLL),
        }
    }
}
