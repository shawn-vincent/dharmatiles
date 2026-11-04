//! `dharmatiles preview`: a live, painted 3D view of tiles in the browser.
//!
//! One small HTTP server on 127.0.0.1 (never 0.0.0.0) serves the viewer
//! (`web/preview/`, embedded in the program) and the tiles' state:
//!
//! | route | what |
//! |---|---|
//! | `GET /` and assets | the viewer |
//! | `GET /api/tiles` | every tile's state as JSON |
//! | `GET /api/tiles/N/mesh` | the tile's mesh as one binary buffer ([`payload`]) |
//! | `POST /api/tiles/N/build` | build a tile that has not been built yet |
//! | `GET /api/events` | server-sent events: a snapshot, then one event per state change |
//!
//! A worker thread builds the tiles one at a time and, as `watch` does,
//! polls the modification time of every file a build read, rebuilding the
//! tile when one changes.  Each build pushes the plan pass first (the
//! tile's name, size and stack, from `check`, before any geometry), then
//! the finished mesh.  Errors are pushed with their file, line and column.

mod assets;
pub mod payload;
mod server;

use std::collections::{BTreeMap, VecDeque};
use std::net::TcpListener;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant, SystemTime};

use dt_eval::progress::{StageInfo, Status, Timings, Tracker, plan_stages};
use dt_eval::{Diag, Options};
use serde_json::{Value, json};

use crate::watch::{Recording, mtime};
use crate::{errors_json, tile_json};

/// How often the worker looks for work.
const POLL: Duration = Duration::from_millis(150);
/// The first port tried, when `--port` is not given.
pub const FIRST_PORT: u16 = 7878;
/// The stage after the build: the mesh's colours and binary buffer.
const ENCODE: &str = "encode";

/// One served tile.
pub struct Slot {
    path: String,
    name: String,
    /// `idle` (not built yet), `building`, `ready` or `failed`.
    status: &'static str,
    plan: Option<Value>,
    record: Option<Value>,
    errors: Vec<Value>,
    mesh: Option<Arc<Vec<u8>>>,
    mesh_version: u64,
    /// The build's progress (a `progress` event's body) while building.
    progress: Option<Value>,
    /// Files the last build read, with their times.
    files: BTreeMap<String, Option<SystemTime>>,
}

impl Slot {
    fn new(path: &str) -> Self {
        let name = path
            .rsplit(['/', ':'])
            .next()
            .unwrap_or(path)
            .trim_end_matches(".kdl")
            .to_owned();
        Self {
            path: path.to_owned(),
            name,
            status: "idle",
            plan: None,
            record: None,
            errors: Vec::new(),
            mesh: None,
            mesh_version: 0,
            progress: None,
            files: BTreeMap::new(),
        }
    }

    fn summary(&self, index: usize) -> Value {
        json!({
            "index": index,
            "path": self.path,
            "name": self.name,
            "status": self.status,
            "plan": self.plan,
            "record": self.record,
            "errors": self.errors,
            "progress": self.progress,
            "mesh_version": self.mesh.as_ref().map(|_| self.mesh_version),
            "mesh_bytes": self.mesh.as_ref().map(|m| m.len()),
        })
    }

    fn changed(&self) -> bool {
        self.status != "idle" && self.files.iter().any(|(f, t)| mtime(f) != *t)
    }
}

/// What the server and the worker share.
pub struct Shared {
    system: String,
    overhang_deg: f64,
    tiles: Mutex<Vec<Slot>>,
    clients: Mutex<Vec<Sender<String>>>,
    wanted: Mutex<VecDeque<usize>>,
    /// Where measured stage durations are kept between builds.
    cache: Option<std::path::PathBuf>,
    /// A palette document the paint is read from at each build.
    palette: Option<std::path::PathBuf>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Shared {
    /// Serve `paths` for connector `system`.
    #[must_use]
    pub fn new(paths: &[String], system: &str, overhang_deg: f64) -> Self {
        Self {
            system: system.to_owned(),
            overhang_deg,
            tiles: Mutex::new(paths.iter().map(|p| Slot::new(p)).collect()),
            clients: Mutex::new(Vec::new()),
            wanted: Mutex::new(VecDeque::new()),
            cache: None,
            palette: None,
        }
    }

    /// Read the paint from this palette document at each build, and
    /// rebuild when it is saved.
    #[must_use]
    pub fn with_palette(mut self, file: std::path::PathBuf) -> Self {
        self.palette = Some(file);
        self
    }

    fn with_palette_opt(self, f: Option<std::path::PathBuf>) -> Self {
        match f {
            Some(f) => self.with_palette(f),
            None => self,
        }
    }

    /// The paint now: the palette document if there is a good one, else
    /// the default.
    fn palette(&self) -> dt_kit::Palette {
        let Some(f) = &self.palette else {
            return dt_kit::Palette::default();
        };
        match std::fs::read_to_string(f)
            .map_err(|e| e.to_string())
            .and_then(|t| dt_kit::Palette::parse(&t))
        {
            Ok(p) => p,
            Err(e) => {
                tell!("palette {}: {e}; using the default paint", f.display());
                dt_kit::Palette::default()
            }
        }
    }

    /// Keep stage timings in `file`, so the next build of a tile has an ETA.
    #[must_use]
    pub fn with_cache(mut self, file: std::path::PathBuf) -> Self {
        self.cache = Some(file);
        self
    }

    /// Tell every listening browser `msg`.
    fn broadcast(&self, msg: &Value) {
        let msg = msg.to_string();
        lock(&self.clients).retain(|tx| tx.send(msg.clone()).is_ok());
    }

    /// The callback a tracker for tile `i` reports through.
    fn sink(self: &Arc<Self>, i: usize) -> impl Fn(&Status) + Send + Sync + use<> {
        let me = Arc::clone(self);
        move |s| me.progress(i, s)
    }

    /// A tile's progress: remembered for a browser that connects later,
    /// and sent now.  The tracker calls this about ten times a second.
    fn progress(&self, i: usize, status: &Status) {
        let body = json!({
            "fraction": status.fraction, "label": status.label, "key": status.key,
            "count": status.count, "elapsed_s": status.elapsed_s,
            "eta_s": status.eta_s, "estimate_s": status.estimate_s,
        });
        if let Some(slot) = lock(&self.tiles).get_mut(i) {
            slot.progress = Some(body.clone());
        }
        self.broadcast(&json!({"type": "progress", "index": i, "progress": body}));
    }

    /// Every tile's state, as the `/api/tiles` body and the first event.
    fn snapshot(&self) -> Value {
        let tiles = lock(&self.tiles);
        json!({
            "type": "snapshot",
            "system": self.system,
            "overhang_deg": self.overhang_deg,
            "tiles": tiles.iter().enumerate().map(|(i, t)| t.summary(i)).collect::<Vec<_>>(),
        })
    }

    /// Change tile `i` and tell every listening browser.
    fn update(&self, i: usize, f: impl FnOnce(&mut Slot)) {
        let summary = {
            let mut tiles = lock(&self.tiles);
            let Some(slot) = tiles.get_mut(i) else { return };
            f(slot);
            slot.summary(i)
        };
        self.broadcast(&json!({"type": "tile", "tile": summary}));
    }

    /// A new listener, whose first message is the snapshot.  The clients
    /// lock is held across both steps so no update falls between them.
    fn subscribe(&self) -> std::sync::mpsc::Receiver<String> {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut clients = lock(&self.clients);
        let _ = tx.send(self.snapshot().to_string());
        clients.push(tx);
        rx
    }

    /// Ask for tile `i` to be built next (a no-op once it has been built).
    fn want(&self, i: usize) -> bool {
        let idle = lock(&self.tiles).get(i).map(|t| t.status == "idle");
        if idle == Some(true) {
            let mut w = lock(&self.wanted);
            if !w.contains(&i) {
                w.push_back(i);
            }
        }
        idle.is_some()
    }

    fn mesh(&self, i: usize) -> Option<Arc<Vec<u8>>> {
        lock(&self.tiles).get(i)?.mesh.clone()
    }

    /// The next tile to build: one asked for, else (when `eager`) the first
    /// not built yet, else one whose files changed.
    fn pick(&self, eager: bool) -> Option<usize> {
        if let Some(i) = lock(&self.wanted).pop_front() {
            return Some(i);
        }
        let tiles = lock(&self.tiles);
        if eager && let Some(i) = tiles.iter().position(|t| t.status == "idle") {
            return Some(i);
        }
        tiles.iter().position(Slot::changed)
    }
}

/// Build tile `i`: plan pass, then geometry, pushing each result.
fn rebuild(shared: &Arc<Shared>, i: usize, opts: &Options) {
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
            "stack": t.stack.iter().map(|e| e.what.value.clone()).collect::<Vec<_>>(),
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
            "the build panicked (a bug in a look); see the terminal".to_owned(),
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
fn rebuild_stl(shared: &Shared, i: usize, opts: &Options, path: &str) {
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
        "square_mm": sq, "size_mm": footprint, "datum_z": Value::Null, "stack": ["stl file"],
    });
    let record = json!({
        "record": {"seed": Value::Null, "system": opts.system},
        "audit": {
            "triangles": a.triangles, "watertight": a.watertight, "overhang_mm2": a.overhang_mm2,
            "band_mm2": a.band_mm2, "band_budget_mm2": a.band_budget_mm2, "dropped": a.dropped.len(),
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
        match shared.pick(eager) {
            Some(i) => rebuild(shared, i, opts),
            None => std::thread::sleep(POLL),
        }
    }
}

/// Open `url` in the default browser.
fn open_browser(url: &str) {
    let (cmd, args): (&str, Vec<&str>) = if cfg!(target_os = "macos") {
        ("open", vec![url])
    } else if cfg!(target_os = "windows") {
        ("cmd", vec!["/c", "start", "", url])
    } else {
        ("xdg-open", vec![url])
    };
    if std::process::Command::new(cmd).args(args).spawn().is_err() {
        tell!("could not open a browser; open {url} yourself");
    }
}

/// Bind 127.0.0.1 on `port`, or, with none given, the first free port from
/// [`FIRST_PORT`].
fn bind(port: Option<u16>) -> Result<TcpListener, String> {
    match port {
        Some(p) => TcpListener::bind(("127.0.0.1", p))
            .map_err(|e| format!("cannot listen on 127.0.0.1:{p}: {e}")),
        None => (FIRST_PORT..FIRST_PORT + 20)
            .find_map(|p| TcpListener::bind(("127.0.0.1", p)).ok())
            .ok_or_else(|| format!("no free port from {FIRST_PORT}; give one with --port")),
    }
}

/// `dharmatiles preview`.  Returns an exit code (it runs until Ctrl-C).
pub fn run(
    paths: &[String],
    opts: &Options,
    port: Option<u16>,
    open: bool,
    palette: Option<std::path::PathBuf>,
) -> u8 {
    let listener = match bind(port) {
        Ok(l) => l,
        Err(e) => {
            tell!("error: {e}");
            return crate::ERROR;
        }
    };
    let addr = listener
        .local_addr()
        .map_or_else(|_| "127.0.0.1".to_owned(), |a| a.to_string());
    // no paths: offer the library, building a tile when it is chosen
    let (paths, eager) = if paths.is_empty() {
        (crate::library_tiles(), false)
    } else {
        (paths.to_vec(), true)
    };
    let cache = dt_eval::discover(&std::env::current_dir().unwrap_or_default()).out;
    let shared = Arc::new(
        Shared::new(&paths, &opts.system, opts.printer.overhang_deg)
            .with_cache(Timings::file_in(&cache))
            .with_palette_opt(palette),
    );
    let url = format!("http://{addr}/");
    tell!("preview: {url}  ({} tile(s); Ctrl-C to stop)", paths.len());
    server::spawn(listener, Arc::clone(&shared));
    if open {
        open_browser(&url);
    }
    work(&shared, opts, eager, &AtomicBool::new(false));
    crate::OK
}

#[cfg(test)]
mod tests;
