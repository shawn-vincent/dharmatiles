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
//! The tiles come from roots: the files named on the command line (built
//! at once), folders (every tile document under them, built when chosen)
//! and the library.  With no paths the roots are the personal home
//! ([`personal_home`]) and the library.  Folders and the on-disk library
//! are rescanned every second: new tiles join the list, deleted ones leave
//! it.
//!
//! A worker thread builds the tiles one at a time and, as `watch` does,
//! polls the modification time of every file a build read, rebuilding the
//! tile when one changes.  Each build pushes the plan pass first (the
//! tile's name, size and stack, from `check`, before any geometry), then
//! the finished mesh.  Errors are pushed with their file, line and column.

mod assets;
pub mod payload;
mod server;
mod stack;

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
/// How often folders are rescanned for new and deleted tiles.
const RESCAN: Duration = Duration::from_secs(1);
/// Folders a scan never enters (besides hidden ones): build output.
const SKIP_DIRS: [&str; 3] = ["out", "target", "node_modules"];

/// Where the tiles come from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RootKind {
    /// Files named on the command line: built at once, never rescanned.
    Files,
    /// A folder: every tile document under it.
    Dir(std::path::PathBuf),
    /// The library's tiles under `lib:` + this prefix (`tiles/`, or
    /// `tiles/water/` for one group).
    Library(String),
}

/// One place the list shows, with its name in the switcher.
#[derive(Clone, Debug)]
pub struct Root {
    label: String,
    kind: RootKind,
}

impl Root {
    #[must_use]
    pub fn new(label: &str, kind: RootKind) -> Self {
        Self {
            label: label.to_owned(),
            kind,
        }
    }

    fn json(&self, index: usize) -> Value {
        let (kind, path) = match &self.kind {
            RootKind::Files => ("files", Value::Null),
            RootKind::Dir(d) => ("dir", json!(d.to_string_lossy())),
            RootKind::Library(p) => ("library", json!(format!("lib:{p}"))),
        };
        json!({"index": index, "label": self.label, "kind": kind, "path": path})
    }

    /// The tiles in this root now, sorted; `None` for the named files.
    fn scan(&self) -> Option<Vec<String>> {
        match &self.kind {
            RootKind::Files => None,
            RootKind::Dir(d) => {
                let mut v = Vec::new();
                tile_files(d, 0, &mut v);
                v.sort();
                Some(v)
            }
            RootKind::Library(prefix) => {
                let want = format!("lib:{prefix}");
                Some(
                    crate::all_library_tiles()
                        .into_iter()
                        .filter(|p| p.starts_with(&want))
                        .collect(),
                )
            }
        }
    }

    /// The group a tile is listed under: its folder below the root (the
    /// library's `ground`, `water`, …), none at the top.
    fn group(&self, path: &str) -> Option<String> {
        match &self.kind {
            RootKind::Dir(d) => {
                let rel = std::path::Path::new(path).strip_prefix(d).ok()?;
                let parent = rel.parent()?.to_string_lossy().replace('\\', "/");
                (!parent.is_empty()).then_some(parent)
            }
            RootKind::Files | RootKind::Library(_) => lib_group(path),
        }
    }
}

/// `lib:tiles/water/x.dtile` → `water`.
fn lib_group(path: &str) -> Option<String> {
    let rest = path.strip_prefix("lib:tiles/")?;
    let (g, _) = rest.split_once('/')?;
    Some(g.to_owned())
}

/// Is this `.dtile` a tile (a `tile` node at the top), not a shared part,
/// palette, printer or project file?  A file that can't be read yet counts.
fn is_tile_document(f: &std::path::Path) -> bool {
    std::fs::read_to_string(f).map_or(true, |t| {
        t.lines()
            .any(|l| l.starts_with("tile ") || l.starts_with("tile{") || l == "tile")
    })
}

/// Every tile document (`.dtile` with a `tile` node) and `.stl` under
/// `dir`, skipping hidden folders and build output.
fn tile_files(dir: &std::path::Path, depth: usize, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let p = e.path();
        if p.is_dir() {
            if depth < 8 && !SKIP_DIRS.contains(&name.as_str()) {
                tile_files(&p, depth + 1, out);
            }
            continue;
        }
        let lower = name.to_ascii_lowercase();
        let tile =
            (lower.ends_with(".dtile") && lower != "dharmatiles.dtile" && is_tile_document(&p))
                || lower.ends_with(".stl");
        if tile {
            out.push(p.to_string_lossy().into_owned());
        }
    }
}

/// The personal home: `$DT_HOME`, else `~/dharmatiles`.  It need not exist
/// yet; the preview lists its tiles once it does.
#[must_use]
pub fn personal_home() -> std::path::PathBuf {
    if let Some(d) = std::env::var_os("DT_HOME").filter(|d| !d.is_empty()) {
        return d.into();
    }
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .unwrap_or_default();
    std::path::PathBuf::from(home).join("dharmatiles")
}

/// One served tile.
pub struct Slot {
    path: String,
    name: String,
    /// The root it is listed under, and its group there.
    root: usize,
    group: Option<String>,
    /// Its file has gone from its folder: off the list, never built.  The
    /// slot stays so every other tile keeps its index.
    gone: bool,
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
    #[cfg(test)]
    fn new(path: &str) -> Self {
        Self::in_root(path, 0, lib_group(path))
    }

    fn in_root(path: &str, root: usize, group: Option<String>) -> Self {
        let name = path
            .rsplit(['/', ':'])
            .next()
            .unwrap_or(path)
            .trim_end_matches(".dtile")
            .to_owned();
        Self {
            path: path.to_owned(),
            name,
            root,
            group,
            gone: false,
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
            "root": self.root,
            "group": self.group,
            "gone": self.gone,
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
        !self.gone && self.status != "idle" && self.files.iter().any(|(f, t)| mtime(f) != *t)
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
    /// Where the tiles come from; a slot's `root` indexes this.
    roots: Vec<Root>,
    /// When the folders were last rescanned.
    scanned: Mutex<Option<Instant>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Shared {
    /// Serve `paths` for connector `system` (the tests' server).
    #[cfg(test)]
    #[must_use]
    pub fn new(paths: &[String], system: &str, overhang_deg: f64) -> Self {
        let roots = if paths.is_empty() {
            Vec::new()
        } else {
            vec![Root::new("Files", RootKind::Files)]
        };
        Self::with_roots(roots, paths, system, overhang_deg)
    }

    /// Serve `roots` for connector `system`: `files` (listed under the
    /// first [`RootKind::Files`] root) and every tile in the others.
    #[must_use]
    pub fn with_roots(roots: Vec<Root>, files: &[String], system: &str, overhang_deg: f64) -> Self {
        let files_root = roots.iter().position(|r| r.kind == RootKind::Files);
        let tiles = files_root.map_or_else(Vec::new, |r| {
            files
                .iter()
                .map(|p| Slot::in_root(p, r, roots[r].group(p)))
                .collect()
        });
        let me = Self {
            system: system.to_owned(),
            overhang_deg,
            tiles: Mutex::new(tiles),
            clients: Mutex::new(Vec::new()),
            wanted: Mutex::new(VecDeque::new()),
            cache: None,
            palette: None,
            roots,
            scanned: Mutex::new(None),
        };
        me.rescan();
        me
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
            "roots": self.roots.iter().enumerate().map(|(i, r)| r.json(i)).collect::<Vec<_>>(),
            "tiles": tiles.iter().enumerate().map(|(i, t)| t.summary(i)).collect::<Vec<_>>(),
        })
    }

    /// Rescan the folders and the library once [`RESCAN`] has passed:
    /// new tiles join the list, deleted ones leave it (and come back if
    /// the file does).  Every browser is told the new list.
    fn rescan_due(&self) {
        let due = {
            let mut last = lock(&self.scanned);
            let due = last.is_none_or(|t| t.elapsed() >= RESCAN);
            if due {
                *last = Some(Instant::now());
            }
            due
        };
        if due && self.rescan() {
            self.broadcast(&self.snapshot());
        }
    }

    /// Rescan every root now.  True when the list changed.
    fn rescan(&self) -> bool {
        let found: Vec<(usize, Vec<String>)> = self
            .roots
            .iter()
            .enumerate()
            .filter_map(|(r, root)| Some((r, root.scan()?)))
            .collect();
        let mut tiles = lock(&self.tiles);
        let mut changed = false;
        for (r, paths) in found {
            let have: std::collections::BTreeSet<&str> = paths.iter().map(String::as_str).collect();
            for t in tiles.iter_mut().filter(|t| t.root == r) {
                let gone = !have.contains(t.path.as_str());
                changed |= t.gone != gone;
                t.gone = gone;
            }
            for p in &paths {
                if !tiles.iter().any(|t| t.root == r && &t.path == p) {
                    tiles.push(Slot::in_root(p, r, self.roots[r].group(p)));
                    changed = true;
                }
            }
        }
        changed
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
        let idle = lock(&self.tiles)
            .get(i)
            .filter(|t| !t.gone)
            .map(|t| t.status == "idle");
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
    /// named file not built yet, else one whose files changed.  Folder and
    /// library tiles are built only when asked for.
    fn pick(&self, eager: bool) -> Option<usize> {
        if let Some(i) = lock(&self.wanted).pop_front() {
            return Some(i);
        }
        let tiles = lock(&self.tiles);
        let named = |t: &Slot| {
            self.roots
                .get(t.root)
                .is_none_or(|r| r.kind == RootKind::Files)
        };
        if eager
            && let Some(i) = tiles
                .iter()
                .position(|t| t.status == "idle" && !t.gone && named(t))
        {
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

/// What the command line asks for: the roots, and the files named.
/// A folder is a root of its own; `lib:tiles/water` (no `.dtile`) is the
/// library's water group; anything else is a file.  No paths: the personal
/// home and the library.
#[must_use]
pub fn roots_for(paths: &[String]) -> (Vec<Root>, Vec<String>) {
    let lib = Root::new("Library", RootKind::Library("tiles/".to_owned()));
    if paths.is_empty() {
        let home = Root::new("My tiles", RootKind::Dir(personal_home()));
        return (vec![home, lib], Vec::new());
    }
    let mut roots = Vec::new();
    let mut files = Vec::new();
    for p in paths {
        let lower = p.to_ascii_lowercase();
        if let Some(rest) = p.strip_prefix("lib:")
            && !lower.ends_with(".dtile")
            && !lower.ends_with(".stl")
        {
            let prefix = format!("{}/", rest.trim_end_matches('/'));
            let label = format!(
                "Library: {}",
                prefix.trim_start_matches("tiles/").trim_end_matches('/')
            );
            let label = if prefix == "tiles/" {
                "Library".to_owned()
            } else {
                label
            };
            roots.push(Root::new(&label, RootKind::Library(prefix)));
        } else if !p.starts_with("lib:") && std::path::Path::new(p).is_dir() {
            roots.push(Root::new(p, RootKind::Dir(p.into())));
        } else {
            files.push(p.clone());
        }
    }
    if !files.is_empty() {
        roots.insert(0, Root::new("Files", RootKind::Files));
    }
    (roots, files)
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
    // files named are built at once; folder and library tiles when chosen
    let (roots, files) = roots_for(paths);
    let cache = dt_eval::discover(&std::env::current_dir().unwrap_or_default()).out;
    let shared = Arc::new(
        Shared::with_roots(roots, &files, &opts.system, opts.printer.overhang_deg)
            .with_cache(Timings::file_in(&cache))
            .with_palette_opt(palette),
    );
    let url = format!("http://{addr}/");
    tell!("preview: {url}  (Ctrl-C to stop)");
    for (i, r) in shared.roots.iter().enumerate() {
        let n = lock(&shared.tiles)
            .iter()
            .filter(|t| !t.gone && t.root == i)
            .count();
        match &r.kind {
            RootKind::Dir(d) if !d.is_dir() => tell!(
                "{}: {} (not there yet; tiles saved there will show up)",
                r.label,
                d.display()
            ),
            RootKind::Dir(d) => tell!("{}: {} ({n} tile(s))", r.label, d.display()),
            RootKind::Files => tell!("{}: {n} file(s)", r.label),
            RootKind::Library(_) => tell!("{}: {n} tile(s)", r.label),
        }
    }
    match dt_eval::source::library_dir() {
        Some(d) => tell!(
            "library: {} (read from disk; edits and new tiles show up live)",
            d.display()
        ),
        None => tell!("library: the copy built into this program"),
    }
    server::spawn(listener, Arc::clone(&shared));
    if open {
        open_browser(&url);
    }
    work(&shared, opts, true, &AtomicBool::new(false));
    crate::OK
}

#[cfg(test)]
mod tests;
