//! The disk cache of builds: a finished [`Built`] kept in a cache folder
//! under a key that is a hash of everything the build depended on, so a
//! build made once is not made again until something it read changes.
//!
//! **The key** (a SHA-256, see [`Store::key`]) covers, in order: the cache
//! format number ([`FORMAT`]), the engine identity the front end gives the
//! store (the version and a hash of the engine's source, so a code change
//! in a dev checkout cannot serve old geometry), the resolution tier
//! ([`Tier`]), the build target's system and printer (its name and every
//! number of its profile), the seed override, the document's path, and the
//! path and full text of every file the build read (the tile and every
//! part it uses, `lib:` ones too).  The engine label a build record carries
//! (`Options::engine`) is not part of it: it is a label, and a hit has it
//! rewritten to the one asked for.
//!
//! **The value** is the whole [`Built`]: the mesh and where each triangle
//! came from, the gate's verdict and per-face flags, the stages, the
//! record, the stack, the painted-surface data and the clouds.  A build
//! read back writes the same STL byte for byte and paints the same
//! ([`codec`] keeps every float's bits).
//!
//! **The folder** holds one directory per *slot* (a document, a target and a
//! tier) and one file per key in it, `v<FORMAT>/<slot>/<key>.dtb`: the
//! entries of one tile's older builds are found together, to show while its
//! new build runs ([`Store::previous`]).  A file is written to a temporary
//! name and renamed into place, so a reader sees a whole entry or none, and
//! two processes writing one key write the same bytes.  Its time is its last
//! use; past the size limit the least recently used entries go first.
//!
//! **Failures are never silent.**  A damaged entry is an error in words
//! ([`Store::get`]), it is removed, and the build that replaces it is real; a
//! write that failed is told too.  The cache never changes what a build
//! makes: a hit is the bytes a build made.

mod codec;

use dt_value::LockExt as _;
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use dt_kit::debug::Collect;
use sha2::{Digest, Sha256};

use dt_eval::canon::Canon;
use dt_eval::{
    Built, Cache, CacheState, CacheUse, Diag, Options, Progress, Source, build_cached,
    build_checked, check, hash, tiers,
};

/// The cache format.  Bump it by hand when the bytes an entry holds, how a
/// key is made, or what a [`Built`] means changes without the engine's
/// source changing (the source hash covers every code change): old entries
/// are then never read (a new `v<FORMAT>` folder) and are cleared as they
/// age out.  2: keys are made of the canonical form ([`crate::canon`]), not
/// of `Debug` text.
pub const FORMAT: u32 = 2;

/// The SHA-256 of `bytes`, as lower-case hex.
fn hex_of(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// The default size limit: 4 GiB.
pub const DEFAULT_LIMIT: u64 = 4 << 30;

const MAGIC: &[u8; 4] = b"DTBC";

pub use dt_eval::Tier;

/// What a build depends on, hashed: where its entry is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Key {
    /// The 128-bit content hash, 32 hex digits.
    id: String,
    /// The slot: a document at a target and a tier, 16 hex digits.
    slot: String,
    path: String,
    target: String,
    tier: Tier,
}

impl Key {
    /// The key's hex digits.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The slot's hex digits.
    #[must_use]
    pub fn slot(&self) -> &str {
        &self.slot
    }
}

/// The key of building `path` with `opts` at `tier`, by an engine named `engine`, given every file the build read
/// (see [`Tracked`]).  The one maker of a build's key: the disk cache ([`Store::key`]) calls it, and so does the
/// browser's worker (dt-web's `stored_key`, compiled into the wasm) to key what it keeps between visits, so a key is
/// made one way wherever it is made.  Pure: it reads no disk and no clock.
#[must_use]
pub fn make_key(engine: &str, reads: &Reads, path: &str, opts: &Options, tier: Tier) -> Key {
    let target = opts.target();
    let mut c = Canon::new();
    c.field("format", &FORMAT)
        .field("engine", &engine)
        .field("tier", tier.name())
        .field("system", opts.system.word())
        .field("printer", &opts.printer.name)
        .field("printer-values", &opts.printer)
        .field("seed", &opts.seed)
        .field("cells", &opts.cells)
        .field("path", path);
    for (p, t) in &reads.0 {
        c.field("file", p).field("text", t);
    }
    let id = hex_of(c.bytes())[..32].to_owned();
    let mut s = Canon::new();
    s.put(path).put(&target).put(tier.name());
    Key {
        id,
        slot: hex_of(s.bytes())[..16].to_owned(),
        path: path.to_owned(),
        target,
        tier,
    }
}

/// Every file a build read, by path, with its text.
#[derive(Clone, Debug, Default)]
pub struct Reads(Vec<(String, String)>);

/// A [`Source`] that remembers every file read through it, with its text:
/// what a build read is what its key covers.
pub struct Tracked<'a> {
    inner: &'a dyn Source,
    seen: Mutex<BTreeMap<String, String>>,
}

impl<'a> Tracked<'a> {
    /// Track reads from `inner`.
    #[must_use]
    pub fn new(inner: &'a dyn Source) -> Self {
        Self {
            inner,
            seen: Mutex::new(BTreeMap::new()),
        }
    }

    /// Every file read so far, by path.
    #[must_use]
    pub fn reads(&self) -> Reads {
        Reads(
            self.seen
                .lock_unpoisoned()
                .iter()
                .map(|(p, t)| (p.clone(), t.clone()))
                .collect(),
        )
    }
}

impl Source for Tracked<'_> {
    fn read(&self, path: &str) -> Result<String, String> {
        let text = self.inner.read(path)?;
        self.seen
            .lock_unpoisoned()
            .insert(path.to_owned(), text.clone());
        Ok(text)
    }
}

/// An older build of the same slot, for showing while the new one runs.
pub struct Previous {
    /// The build.
    pub built: Built,
    /// The engine that made it.
    pub made_by: String,
    /// When it was made.
    pub made_at: SystemTime,
    /// Its tier.
    pub tier: Tier,
}

/// One entry, as `dharmatiles cache` lists it.
#[derive(Clone, Debug)]
pub struct EntryInfo {
    /// Where it is.
    pub file: PathBuf,
    /// Its size in bytes.
    pub bytes: u64,
    /// When it was last used.
    pub used: SystemTime,
    /// The document it is a build of.
    pub path: String,
    /// Its target (`db`, `ol-fine`).
    pub target: String,
    /// Its tier.
    pub tier: String,
    /// The engine that made it.
    pub engine: String,
    /// Its triangles.
    pub triangles: u64,
}

/// What is in the folder.
#[derive(Clone, Debug, Default)]
pub struct Stats {
    /// How many entries.
    pub entries: usize,
    /// Their size, bytes.
    pub bytes: u64,
    /// Files that are not whole entries (a leftover temporary, a damaged
    /// header): counted and sized apart, and removed by a clear or an
    /// eviction.
    pub strays: usize,
}

/// The cache folder.
#[derive(Clone, Debug)]
pub struct Store {
    dir: PathBuf,
    engine: String,
    limit: u64,
}

/// Where the cache lives when `DT_CACHE` does not say: `~/Library/Caches/dharmatiles`
/// on macOS, `%LOCALAPPDATA%\dharmatiles\cache` on Windows, else
/// `$XDG_CACHE_HOME/dharmatiles` or `~/.cache/dharmatiles`.
///
/// # Errors
/// There is no home folder to put it in.
pub fn default_dir() -> Result<PathBuf, String> {
    let home = || {
        std::env::var_os("HOME")
            .filter(|h| !h.is_empty())
            .map(PathBuf::from)
            .ok_or_else(|| {
                "there is no home folder for the cache: set DT_CACHE to a folder".to_owned()
            })
    };
    if cfg!(target_os = "macos") {
        return Ok(home()?.join("Library/Caches/dharmatiles"));
    }
    if cfg!(windows) {
        return std::env::var_os("LOCALAPPDATA")
            .filter(|h| !h.is_empty())
            .map(|d| PathBuf::from(d).join("dharmatiles").join("cache"))
            .ok_or_else(|| "there is no LOCALAPPDATA for the cache: set DT_CACHE".to_owned());
    }
    match std::env::var_os("XDG_CACHE_HOME").filter(|h| !h.is_empty()) {
        Some(x) => Ok(PathBuf::from(x).join("dharmatiles")),
        None => Ok(home()?.join(".cache/dharmatiles")),
    }
}

/// A size such as `4GB`, `500MB`, `65536` (bytes), for `DT_CACHE_LIMIT`.
///
/// # Errors
/// What is wrong with it, in words.
pub fn parse_size(text: &str) -> Result<u64, String> {
    let t = text.trim();
    let split = t.find(|c: char| !c.is_ascii_digit()).unwrap_or(t.len());
    let (num, unit) = t.split_at(split);
    let n: u64 = num
        .parse()
        .map_err(|_| format!("`{text}` is not a size (try 4GB or 500MB)"))?;
    let scale: u64 = match unit.trim().to_ascii_lowercase().as_str() {
        "" | "b" => 1,
        "k" | "kb" | "kib" => 1 << 10,
        "m" | "mb" | "mib" => 1 << 20,
        "g" | "gb" | "gib" => 1 << 30,
        "t" | "tb" | "tib" => 1 << 40,
        other => {
            return Err(format!(
                "`{text}`: no size unit `{other}` (try 4GB or 500MB)"
            ));
        }
    };
    n.checked_mul(scale)
        .ok_or_else(|| format!("`{text}` is too large"))
}

static TEMPS: AtomicU64 = AtomicU64::new(0);

impl Store {
    /// A store in `dir` for the engine `engine` (see [`Store::key`]), at the
    /// default size limit.  Nothing is read or made until it is used.
    #[must_use]
    pub fn new(dir: impl Into<PathBuf>, engine: impl Into<String>) -> Self {
        Self {
            dir: dir.into(),
            engine: engine.into(),
            limit: DEFAULT_LIMIT,
        }
    }

    /// The same store with its size limit set, bytes.
    #[must_use]
    pub fn with_limit(mut self, bytes: u64) -> Self {
        self.limit = bytes;
        self
    }

    /// The store the environment names: `DT_CACHE` (a folder, or `off` for
    /// none) else the platform's cache folder ([`default_dir`]), at the size
    /// `DT_CACHE_LIMIT` says (else [`DEFAULT_LIMIT`]).  `None` when it is
    /// off.
    ///
    /// # Errors
    /// No folder can be found, or the limit is not a size.
    pub fn from_env(engine: &str) -> Result<Option<Self>, String> {
        let dir = match std::env::var_os("DT_CACHE").filter(|d| !d.is_empty()) {
            Some(d) if d == "off" => return Ok(None),
            Some(d) => PathBuf::from(d),
            None => default_dir()?,
        };
        let mut store = Self::new(dir, engine);
        if let Some(l) = std::env::var_os("DT_CACHE_LIMIT").filter(|d| !d.is_empty()) {
            let l = l.to_string_lossy();
            store.limit = parse_size(&l).map_err(|e| format!("DT_CACHE_LIMIT: {e}"))?;
        }
        Ok(Some(store))
    }

    /// The cache folder.
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The size limit, bytes.
    #[must_use]
    pub fn limit(&self) -> u64 {
        self.limit
    }

    /// The engine identity the key covers.
    #[must_use]
    pub fn engine(&self) -> &str {
        &self.engine
    }

    fn root(&self) -> PathBuf {
        self.dir.join(format!("v{FORMAT}"))
    }

    /// Every format's folder (`v1`, `v2`, ...): older ones are still counted,
    /// evicted and cleared, though never read.
    fn roots(&self) -> Result<Vec<PathBuf>, String> {
        let Some(rd) = read_dir_opt(&self.dir)? else {
            return Ok(Vec::new());
        };
        let mut v = Vec::new();
        for e in rd {
            let p = e
                .map_err(|e| format!("cannot read {}: {e}", self.dir.display()))?
                .path();
            let versioned = p
                .file_name()
                .and_then(|n| n.to_str())
                .and_then(|n| n.strip_prefix('v'))
                .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()));
            if versioned && p.is_dir() {
                v.push(p);
            }
        }
        Ok(v)
    }

    fn file_of(&self, key: &Key) -> PathBuf {
        self.root().join(&key.slot).join(format!("{}.dtb", key.id))
    }

    /// The key of building `path` with `opts`, given every file the build
    /// read (see [`Tracked`]).
    #[must_use]
    pub fn key(&self, reads: &Reads, path: &str, opts: &Options) -> Key {
        self.key_for(reads, path, opts, opts.tier)
    }

    fn key_for(&self, reads: &Reads, path: &str, opts: &Options, tier: Tier) -> Key {
        make_key(&self.engine, reads, path, opts, tier)
    }

    /// The key of the same build at `tier`.  (Entries of different tiers
    /// never answer each other; this names the other one.)
    #[must_use]
    pub fn key_at(&self, reads: &Reads, path: &str, opts: &Options, tier: Tier) -> Key {
        let mut o = opts.clone();
        o.tier = tier;
        self.key_for(reads, path, &o, tier)
    }

    /// Whether an entry is kept under `key`.
    #[must_use]
    pub fn contains(&self, key: &Key) -> bool {
        self.file_of(key).is_file()
    }

    /// The build kept under `key`, when it holds at least what `want` asks
    /// for (the clouds; the debug marks).  `Ok(None)`: nothing usable is kept.
    /// Its use is noted (it is the last used, for eviction).
    ///
    /// # Errors
    /// The entry is damaged or cannot be read: said in words, and the entry
    /// removed (so the next build makes it again); the build that follows is
    /// real.
    pub fn get(&self, key: &Key, want: Collect) -> Result<Option<Built>, String> {
        let file = self.file_of(key);
        let Some((head, built)) = self.read_entry(&file)? else {
            return Ok(None);
        };
        if head.key != key.id {
            return self.damaged(&file, "it holds another key's build");
        }
        if !collected(&built).covers(want) {
            return Ok(None);
        }
        self.touch(&file);
        let mut built = trim(built, want);
        built.report.cache.made_by = Some(head.engine);
        Ok(Some(built))
    }

    /// The newest build of `key`'s slot that is not `key` itself: an older
    /// build of the same document at the same target and tier, to show while
    /// the new one runs.  `Ok(None)` when there is none (or none holds what
    /// `want` asks for).
    ///
    /// # Errors
    /// As [`Store::get`].
    pub fn previous(&self, key: &Key, want: Collect) -> Result<Option<Previous>, String> {
        let dir = self.root().join(&key.slot);
        let Some(rd) = read_dir_opt(&dir)? else {
            return Ok(None);
        };
        let mut found: Vec<(SystemTime, PathBuf)> = Vec::new();
        for e in rd {
            let e = e.map_err(|e| format!("cannot read {}: {e}", dir.display()))?;
            let p = e.path();
            if p.extension().is_none_or(|x| x != "dtb")
                || p.file_stem().is_some_and(|s| s == key.id.as_str())
            {
                continue;
            }
            if let Ok(m) = e.metadata().and_then(|m| m.modified()) {
                found.push((m, p));
            }
        }
        found.sort_by_key(|a| std::cmp::Reverse(a.0));
        for (_, file) in found {
            let Some((head, built)) = self.read_entry(&file)? else {
                continue;
            };
            if collected(&built).covers(want) {
                return Ok(Some(Previous {
                    built: trim(built, want),
                    made_by: head.engine,
                    made_at: head.created,
                    tier: key.tier,
                }));
            }
        }
        Ok(None)
    }

    /// Keep `built` under `key`: written to a temporary name and renamed, so
    /// no reader sees half of it, then the folder is brought under its size
    /// limit (the entry just written is never the one that goes).
    ///
    /// # Errors
    /// The folder or file cannot be written, in words.
    pub fn put(&self, key: &Key, built: &Built) -> Result<(), String> {
        let file = self.file_of(key);
        let dir = file
            .parent()
            .ok_or_else(|| format!("{} has no folder", file.display()))?;
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("cannot create the cache folder {}: {e}", dir.display()))?;
        let out = entry_bytes(&self.engine, key, built, now_secs())?;
        let tmp = dir.join(format!(
            ".tmp-{}-{}",
            std::process::id(),
            TEMPS.fetch_add(1, Ordering::Relaxed)
        ));
        let written = std::fs::File::create(&tmp)
            .and_then(|mut f| f.write_all(&out).and_then(|()| f.sync_all()))
            .and_then(|()| std::fs::rename(&tmp, &file));
        if let Err(e) = written {
            let cleaned = match std::fs::remove_file(&tmp) {
                Ok(()) => String::new(),
                Err(c) if c.kind() == std::io::ErrorKind::NotFound => String::new(),
                Err(c) => format!(" (and {} could not be removed: {c})", tmp.display()),
            };
            return Err(format!(
                "cannot write the cache entry {}: {e}{cleaned}",
                file.display()
            ));
        }
        self.evict(Some(&file))
    }

    /// Bring the folder under its size limit by removing the least recently
    /// used entries (and any stray temporary an hour old), never `keep`.
    ///
    /// # Errors
    /// The folder cannot be read, or an entry cannot be removed, in words.
    pub fn evict(&self, keep: Option<&Path>) -> Result<(), String> {
        let mut all = self.files()?;
        // leftover temporaries (an hour old) go whatever the size
        for f in all.iter().filter(|f| !f.entry) {
            remove_entry(&f.file)?;
        }
        all.retain(|f| f.entry);
        let mut total: u64 = all.iter().map(|f| f.bytes).sum();
        // oldest use first
        all.sort_by_key(|f| f.used);
        for f in &all {
            if total <= self.limit {
                break;
            }
            if Some(f.file.as_path()) == keep {
                continue;
            }
            remove_entry(&f.file)?;
            total = total.saturating_sub(f.bytes);
        }
        Ok(())
    }

    fn touch(&self, file: &Path) {
        // Allowed: noting a use is best effort; a file that cannot be
        // touched (read-only folder) keeps its older time and may go sooner.
        let _ = std::fs::File::options()
            .write(true)
            .open(file)
            .and_then(|f| f.set_modified(SystemTime::now()));
    }

    /// Every file under the cache: size and last use.
    #[expect(
        clippy::disallowed_methods,
        reason = "permitted silence: a best-effort filesystem probe (a missing or unreadable file means none yet, read again on the next call); permitted silence: a clock before 1970 or a missing mtime reads as zero (a stamp, never data)"
    )]
    fn files(&self) -> Result<Vec<Stray>, String> {
        let mut out = Vec::new();
        for root in self.roots()? {
            let Some(slots) = read_dir_opt(&root)? else {
                continue;
            };
            for slot in slots {
                let slot = slot.map_err(|e| format!("cannot read {}: {e}", root.display()))?;
                let sp = slot.path();
                if !sp.is_dir() {
                    continue;
                }
                let Some(rd) = read_dir_opt(&sp)? else {
                    continue;
                };
                for e in rd {
                    let e = e.map_err(|e| format!("cannot read {}: {e}", sp.display()))?;
                    let Ok(m) = e.metadata() else {
                        // gone between the listing and now: another process
                        // evicted it
                        continue;
                    };
                    if !m.is_file() {
                        continue;
                    }
                    let name = e.file_name().to_string_lossy().into_owned();
                    let is_entry = name.ends_with(".dtb");
                    let used = m.modified().unwrap_or(UNIX_EPOCH);
                    // a temporary is another writer's work in progress unless it is old
                    if !is_entry
                        && SystemTime::now()
                            .duration_since(used)
                            .is_ok_and(|age| age < Duration::from_secs(3600))
                    {
                        continue;
                    }
                    out.push(Stray {
                        file: e.path(),
                        bytes: m.len(),
                        used,
                        entry: is_entry,
                    });
                }
            }
        }
        Ok(out)
    }

    /// What is in the folder.
    ///
    /// # Errors
    /// The folder cannot be read.
    pub fn stats(&self) -> Result<Stats, String> {
        let mut s = Stats::default();
        for f in self.files()? {
            s.bytes += f.bytes;
            if f.entry {
                s.entries += 1;
            } else {
                s.strays += 1;
            }
        }
        Ok(s)
    }

    /// Every entry, newest use first.
    ///
    /// # Errors
    /// The folder cannot be read, or an entry's header is damaged.
    pub fn list(&self) -> Result<Vec<EntryInfo>, String> {
        let mut out = Vec::new();
        for f in self.files()?.into_iter().filter(|f| f.entry) {
            let bytes = match read_head(&f.file) {
                Ok(Some(b)) => b,
                // removed by another process since the listing
                Ok(None) => continue,
                Err(e) => return Err(e),
            };
            let head = parse_head(&bytes).map_err(|e| format!("{}: {e}", f.file.display()))?;
            out.push(EntryInfo {
                file: f.file,
                bytes: f.bytes,
                used: f.used,
                path: head.path,
                target: head.target,
                tier: head.tier,
                engine: head.engine,
                triangles: head.triangles,
            });
        }
        out.sort_by_key(|a| std::cmp::Reverse(a.used));
        Ok(out)
    }

    /// Remove everything the cache holds (every format, every stray).
    /// Returns how many entries and bytes went.
    ///
    /// # Errors
    /// A file cannot be removed, in words.
    pub fn clear(&self) -> Result<(usize, u64), String> {
        let mut entries = 0;
        let mut bytes = 0;
        for f in self.files()? {
            remove_entry(&f.file)?;
            bytes += f.bytes;
            entries += usize::from(f.entry);
        }
        // the slot folders, now empty; one that is not empty is left, and
        // said by the error
        for root in self.roots()? {
            for e in read_dir_opt(&root)?.into_iter().flatten() {
                let p = e
                    .map_err(|e| format!("cannot read {}: {e}", root.display()))?
                    .path();
                if p.is_dir() {
                    std::fs::remove_dir(&p)
                        .map_err(|e| format!("cannot remove {}: {e}", p.display()))?;
                }
            }
            std::fs::remove_dir(&root)
                .map_err(|e| format!("cannot remove {}: {e}", root.display()))?;
        }
        Ok((entries, bytes))
    }

    /// An entry that cannot be used: removed, and said.
    fn damaged<T>(&self, file: &Path, why: &str) -> Result<T, String> {
        let removed = match std::fs::remove_file(file) {
            Ok(()) => "it was removed, so this build is made again".to_owned(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                "it is already gone, so this build is made again".to_owned()
            }
            Err(e) => format!("and it could not be removed: {e}"),
        };
        Err(format!(
            "the cache entry {} is damaged ({why}); {removed}",
            file.display()
        ))
    }

    /// An entry, read and checked.  `Ok(None)` when there is no such file.
    fn read_entry(&self, file: &Path) -> Result<Option<(Head, Built)>, String> {
        let bytes = match std::fs::read(file) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => {
                return Err(format!(
                    "cannot read the cache entry {}: {e}",
                    file.display()
                ));
            }
        };
        let (head, body) = match split(&bytes) {
            Ok(v) => v,
            Err(why) => return self.damaged(file, &why),
        };
        match codec::built_in(body) {
            Ok(b) => Ok(Some((head, b))),
            Err(why) => self.damaged(file, &why),
        }
    }
}

/// What a build holds beside its geometry, as a [`Collect`].
fn collected(b: &Built) -> Collect {
    Collect {
        clouds: b.report.clouds.is_some(),
        marks: b.report.debug.is_some(),
    }
}

/// `built` with only what `want` asks of the clouds and the marks: a build
/// read back is what a fresh one with those options would have made.
fn trim(mut built: Built, want: Collect) -> Built {
    if !want.clouds {
        built.report.clouds = None;
    }
    if !want.marks {
        built.report.debug = None;
    }
    built
}

struct Stray {
    file: PathBuf,
    bytes: u64,
    used: SystemTime,
    entry: bool,
}

struct Head {
    key: String,
    path: String,
    target: String,
    tier: String,
    engine: String,
    created: SystemTime,
    triangles: u64,
}

fn read_dir_opt(dir: &Path) -> Result<Option<std::fs::ReadDir>, String> {
    match std::fs::read_dir(dir) {
        Ok(rd) => Ok(Some(rd)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("cannot read {}: {e}", dir.display())),
    }
}

fn remove_entry(file: &Path) -> Result<(), String> {
    match std::fs::remove_file(file) {
        Ok(()) => Ok(()),
        // another process removed it first
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("cannot remove {}: {e}", file.display())),
    }
}

/// A fast checksum, to notice a damaged file: words folded as the
/// evaluator's other hash does.
fn checksum(bytes: &[u8]) -> u64 {
    let mut h = hash::OFFSET;
    let (words, rest) = bytes.as_chunks::<8>();
    for w in words {
        h = hash::step(h, u64::from_le_bytes(*w));
    }
    for &b in rest {
        h = hash::step(h, u64::from(b));
    }
    hash::step(h, bytes.len() as u64)
}

/// The first bytes of an entry, enough for its header.
fn read_head(file: &Path) -> Result<Option<Vec<u8>>, String> {
    use std::io::Read;
    let mut f = match std::fs::File::open(file) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("cannot read {}: {e}", file.display())),
    };
    let mut fixed = [0u8; 12];
    f.read_exact(&mut fixed)
        .map_err(|e| format!("{} is damaged (no header: {e})", file.display()))?;
    let n = u32::from_le_bytes([fixed[8], fixed[9], fixed[10], fixed[11]]) as usize;
    if n > 1 << 20 {
        return Err(format!(
            "{} is damaged (a header of {n} bytes)",
            file.display()
        ));
    }
    let mut all = fixed.to_vec();
    all.resize(12 + n, 0);
    f.read_exact(&mut all[12..])
        .map_err(|e| format!("{} is damaged (a short header: {e})", file.display()))?;
    Ok(Some(all))
}

fn parse_head(bytes: &[u8]) -> Result<Head, String> {
    if bytes.len() < 12 || &bytes[..4] != MAGIC {
        return Err("not a cache entry".to_owned());
    }
    let format = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    if format != FORMAT {
        return Err(format!("format {format}, this program reads {FORMAT}"));
    }
    let n = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]) as usize;
    let h = bytes
        .get(12..12 + n)
        .ok_or_else(|| "a short header".to_owned())?;
    let mut r = codec::Rd::new(h);
    let head = Head {
        key: r.str()?,
        path: r.str()?,
        target: r.str()?,
        tier: r.str()?,
        engine: r.str()?,
        created: UNIX_EPOCH + Duration::from_secs(r.u64()?),
        triangles: r.u64()?,
    };
    r.done()?;
    Ok(head)
}

/// A whole entry as it is kept (the disk's file, or a memory store's bytes): magic, format, header, body, checksum.
#[expect(
    clippy::disallowed_methods,
    reason = "permitted silence: a clock before 1970 or a missing mtime reads as zero (a stamp, never data)"
)]
fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// `stamp` is when it was made, in seconds since 1970 (the caller's clock: a browser has no `SystemTime`).
fn entry_bytes(engine: &str, key: &Key, built: &Built, stamp: u64) -> Result<Vec<u8>, String> {
    let body = codec::built_out(built);
    let mut head = codec::Wr(Vec::new());
    head.str(&key.id);
    head.str(&key.path);
    head.str(&key.target);
    head.str(key.tier.name());
    head.str(engine);
    head.u64(stamp);
    head.u64(built.audit.triangles as u64);
    let head = head.0;
    let mut out = Vec::with_capacity(head.len() + body.len() + 24);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&FORMAT.to_le_bytes());
    out.extend_from_slice(
        &u32::try_from(head.len())
            .map_err(|_| "a header too long")?
            .to_le_bytes(),
    );
    out.extend_from_slice(&head);
    out.extend_from_slice(&(body.len() as u64).to_le_bytes());
    out.extend_from_slice(&body);
    let sum = checksum(&out);
    out.extend_from_slice(&sum.to_le_bytes());
    Ok(out)
}

/// A whole entry's header and build, once its checksum holds ([`entry_bytes`] backwards).
fn entry_read(bytes: &[u8]) -> Result<(Head, Built), String> {
    let (head, body) = split(bytes)?;
    Ok((head, codec::built_in(body)?))
}

/// A whole entry's header and body, once its checksum holds.
fn split(bytes: &[u8]) -> Result<(Head, &[u8]), String> {
    let head = parse_head(bytes)?;
    let n = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]) as usize;
    let at = 12 + n;
    let len_bytes = bytes
        .get(at..at + 8)
        .ok_or_else(|| "it ends in its header".to_owned())?;
    let mut l = [0u8; 8];
    l.copy_from_slice(len_bytes);
    let body_len =
        usize::try_from(u64::from_le_bytes(l)).map_err(|_| "a body too long".to_owned())?;
    let body_at = at + 8;
    let end = body_at
        .checked_add(body_len)
        .filter(|&e| e + 8 == bytes.len())
        .ok_or_else(|| {
            format!(
                "its length is wrong (a body of {body_len} bytes in a file of {})",
                bytes.len()
            )
        })?;
    let mut s = [0u8; 8];
    s.copy_from_slice(&bytes[end..]);
    if u64::from_le_bytes(s) != checksum(&bytes[..end]) {
        return Err("its checksum does not hold".to_owned());
    }
    Ok((head, &bytes[body_at..end]))
}

/// Where finished builds are kept: [`build_stored`] reads and writes through this, so the program's disk cache
/// ([`Store`]) and a browser worker's memory ([`MemStore`]) are served by the one function.  An implementation
/// keeps a [`Built`] under a [`Key`] ([`make_key`] with [`Kept::engine`]) and gives back what a build with `want`
/// would have made.
pub trait Kept: Sync {
    /// The engine identity the keys cover.
    fn engine(&self) -> &str;

    /// The build kept under `key` when it holds at least what `want` asks for; `Ok(None)`: nothing usable is kept.
    ///
    /// # Errors
    /// A damaged or unreadable entry, in words (the entry is dropped, so the next build is real).
    fn get(&self, key: &Key, want: Collect) -> Result<Option<Built>, String>;

    /// Keep `built` under `key`.
    ///
    /// # Errors
    /// The entry cannot be kept, in words.
    fn put(&self, key: &Key, built: &Built) -> Result<(), String>;
}

impl Kept for Store {
    fn engine(&self) -> &str {
        &self.engine
    }

    fn get(&self, key: &Key, want: Collect) -> Result<Option<Built>, String> {
        Store::get(self, key, want)
    }

    fn put(&self, key: &Key, built: &Built) -> Result<(), String> {
        Store::put(self, key, built)
    }
}

impl Store {
    /// This store as what [`build_stored`] takes (for a caller that holds an `Option<&Store>`).
    #[must_use]
    pub fn kept(&self) -> &dyn Kept {
        self
    }
}

/// Builds kept in memory, as the same bytes the disk keeps: what a browser's worker holds between calls (a second
/// build of an unchanged tile is read, not made), bounded by `limit` bytes with the least recently used going first.
/// It reads no clock and no disk, so it builds for `wasm32-unknown-unknown`.
pub struct MemStore {
    engine: String,
    limit: usize,
    held: Mutex<MemHeld>,
}

#[derive(Default)]
struct MemHeld {
    /// key id to (bytes, last use)
    entries: BTreeMap<String, (Vec<u8>, u64)>,
    tick: u64,
    bytes: usize,
}

impl MemHeld {
    fn drop_entry(&mut self, id: &str) {
        if let Some((b, _)) = self.entries.remove(id) {
            self.bytes -= b.len();
        }
    }
}

impl MemStore {
    /// A memory store for the engine `engine`, keeping at most `limit` bytes.
    #[must_use]
    pub fn new(engine: impl Into<String>, limit: usize) -> Self {
        Self {
            engine: engine.into(),
            limit,
            held: Mutex::new(MemHeld::default()),
        }
    }

    /// How many builds are kept.
    #[must_use]
    pub fn len(&self) -> usize {
        self.held.lock_unpoisoned().entries.len()
    }

    /// Whether nothing is kept.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Kept for MemStore {
    fn engine(&self) -> &str {
        &self.engine
    }

    fn get(&self, key: &Key, want: Collect) -> Result<Option<Built>, String> {
        let mut h = self.held.lock_unpoisoned();
        h.tick += 1;
        let tick = h.tick;
        let Some((bytes, used)) = h.entries.get_mut(&key.id) else {
            return Ok(None);
        };
        *used = tick;
        let read = entry_read(bytes);
        let (head, built) = match read {
            Ok(v) => v,
            Err(why) => {
                h.drop_entry(&key.id);
                return Err(format!("the kept build is damaged ({why}); it is dropped"));
            }
        };
        if head.key != key.id {
            h.drop_entry(&key.id);
            return Err("a kept build holds another key's build; it is dropped".to_owned());
        }
        if !collected(&built).covers(want) {
            return Ok(None);
        }
        let mut built = trim(built, want);
        built.report.cache.made_by = Some(head.engine);
        Ok(Some(built))
    }

    fn put(&self, key: &Key, built: &Built) -> Result<(), String> {
        let bytes = entry_bytes(&self.engine, key, built, 0)?;
        let mut h = self.held.lock_unpoisoned();
        h.tick += 1;
        let tick = h.tick;
        h.drop_entry(&key.id);
        h.bytes += bytes.len();
        h.entries.insert(key.id.clone(), (bytes, tick));
        // past the limit the least recently used go first, never the one just kept
        while h.bytes > self.limit {
            let Some(oldest) = h
                .entries
                .iter()
                .filter(|(id, _)| **id != key.id)
                .min_by_key(|(_, (_, used))| *used)
                .map(|(id, _)| id.clone())
            else {
                break;
            };
            h.drop_entry(&oldest);
        }
        Ok(())
    }
}

/// The key of building `path` with `opts` at `tier` in `store`: [`make_key`] with the store's engine.
fn key_in(store: &dyn Kept, reads: &Reads, path: &str, opts: &Options, tier: Tier) -> Key {
    let mut o = opts.clone();
    o.tier = tier;
    make_key(store.engine(), reads, path, &o, tier)
}

/// [`build_cached`] and a store (the disk cache, or a worker's memory): a build whose key (see [`Store::key`])
/// is kept is read, not made, and a build that is made is kept.  The result
/// is the same, byte for byte, as without the store; [`Built::cache`] says
/// which it was and what went wrong with the cache, if anything (a damaged
/// entry is removed and the build made again; an entry that cannot be
/// written is told, and the build stands).
///
/// A stored build always carries the clouds (as the editor's builds do), so
/// one made for the command line serves the editor.  What `opts` did not ask
/// for (clouds, debug marks) is left off the result.
///
/// # Errors
/// As [`build`].
fn stored_build(
    sources: &dyn Source,
    path: &str,
    opts: &Options,
    progress: &Progress,
    cache: Option<&Cache>,
    store: Option<&dyn Kept>,
) -> Result<Built, Vec<Diag>> {
    let Some(store) = store else {
        return build_cached(sources, path, opts, progress, cache);
    };
    let tracked = Tracked::new(sources);
    let text = tracked
        .read(path)
        .map_err(|e| vec![Diag::plain(dt_lang::codes::SOURCE, e)])?;
    let stored = Options {
        clouds: true,
        ..opts.clone()
    };
    let c = check(&tracked, path, &stored)?;
    let key = key_in(store, &tracked.reads(), path, opts, opts.tier);
    let mut problems = Vec::new();
    let want = Collect {
        clouds: opts.clouds,
        marks: opts.debug,
    };
    match store.get(&key, want) {
        Ok(Some(mut b)) => {
            // the engine label is not part of the key: it is the one asked for
            b.record.engine.clone_from(&opts.engine);
            b.report.cache.state = CacheState::Hit;
            return Ok(b);
        }
        Ok(None) => {}
        Err(e) => problems.push(e),
    }
    let mut b = build_checked(&c, path, &text, &stored, progress, cache)?;
    if let Err(e) = store.put(&key, &b) {
        problems.push(e);
    }
    if !opts.clouds {
        b.report.clouds = None;
    }
    b.report.cache = CacheUse {
        state: CacheState::Miss,
        made_by: None,
        problems,
    };
    Ok(b)
}

/// [`build_stored`]: what a map or a browser asks for by distance, at the tier `opts` names ([`Options::tier`]).
///
/// - [`Tier::Full`]: the finished build, through the union and the gate.
/// - [`Tier::Low`]: the same places drawn with less detail, the parts as they
///   are, each face flagged by its angle alone.  **Not yet checked**
///   ([`Built::checked`]).
/// - [`Tier::SuperLow`]: a low build cut down to
///   [`crate::tiers::SUPER_LOW_BUDGET`] triangles.  It is made from the low
///   build (made, not kept) and kept as its own entry.
/// - [`Tier::Draft`]: the editor's draft (coarse cells); the program passes no store for it.
///
/// Each tier is its own entry in the store.  Only a full build may be
/// written out: see [`Built::verdict`].
///
/// # Errors
/// As [`build`].
pub fn build_stored(
    sources: &dyn Source,
    path: &str,
    opts: &Options,
    progress: &Progress,
    cache: Option<&Cache>,
    store: Option<&dyn Kept>,
) -> Result<Built, Vec<Diag>> {
    let tier = opts.tier;
    let at = |t: Tier| Options {
        tier: t,
        ..opts.clone()
    };
    if tier != Tier::SuperLow {
        let o = at(tier);
        return stored_build(sources, path, &o, progress, cache, store);
    }
    let Some(store) = store else {
        let low = build_cached(sources, path, &at(Tier::Low), progress, cache)?;
        return Ok(tiers::super_low(low));
    };
    let tracked = Tracked::new(sources);
    let o = at(Tier::Low);
    check(&tracked, path, &o)?;
    let key = key_in(store, &tracked.reads(), path, &o, Tier::SuperLow);
    let want = Collect::default();
    let mut problems = Vec::new();
    match store.get(&key, want) {
        Ok(Some(mut b)) => {
            b.record.engine.clone_from(&opts.engine);
            b.report.cache.state = CacheState::Hit;
            return Ok(b);
        }
        Ok(None) => {}
        Err(e) => problems.push(e),
    }
    // the low build it is cut from is not kept: only the small one is
    // no clouds: a preview is for looking at, and they are most of its size
    let low = build_cached(&tracked, path, &o, progress, cache)?;
    let mut b = tiers::super_low(low);
    if let Err(e) = store.put(&key, &b) {
        problems.push(e);
    }
    b.report.cache = CacheUse {
        state: CacheState::Miss,
        made_by: None,
        problems,
    };
    Ok(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_digest_is_sha256() {
        // the standard test vectors
        assert_eq!(
            hex_of(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            hex_of(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            hex_of(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn a_key_is_the_one_a_known_input_always_made() {
        // a change of this key is a change of how keys are made: bump
        // [`FORMAT`] with it, so old entries are ignored and not misread
        let store = Store::new("unused", "pinned-engine");
        let reads = Reads(vec![("mem/t.dtile".to_owned(), "tile".to_owned())]);
        let key = store.key(&reads, "mem/t.dtile", &Options::default());
        assert_eq!(key.id(), "5db5be104b958dd6e40cfb628093e1ea");
    }

    #[test]
    #[expect(
        clippy::disallowed_methods,
        reason = "permitted silence: parsing where None is the meaning (a probe of a written word or an optional parameter)"
    )]
    fn sizes_parse() {
        assert_eq!(parse_size("4GB").unwrap_or(0), 4 << 30);
        assert_eq!(parse_size("500 MB").unwrap_or(0), 500 << 20);
        assert_eq!(parse_size("65536").unwrap_or(0), 65536);
        assert!(parse_size("lots").is_err());
        assert!(parse_size("4 parsecs").is_err());
    }

    #[test]
    fn a_default_dir_is_found_or_said() {
        // not asserted to a place (it depends on the platform and HOME): it
        // is either a folder named for the program or an error in words
        match default_dir() {
            Ok(d) => assert!(
                d.to_string_lossy()
                    .to_ascii_lowercase()
                    .contains("dharmatiles")
            ),
            Err(e) => assert!(e.contains("cache")),
        }
    }
}
