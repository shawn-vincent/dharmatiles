//! `dharmatiles cache`: the disk cache of builds ([`dt_store::Store`]) where it
//! is, what is in it, emptying it, and `cache previews`, which builds the
//! library's previews into it (or into a folder an install copies in).

use std::path::PathBuf;
use std::time::{Instant, SystemTime};

use crate::cli_wire::{
    CacheCleared, CacheEntryRow, CacheOff, CacheShown, CacheStatus, Failed, PreviewKept,
    PreviewTile, PreviewsReport,
};
use dt_eval::{Files, Options, Progress};
use dt_store::Store;

use crate::cli::CacheAction;
use crate::{BUG, ERROR, OK};

/// The engine identity the cache keys on: the program's version and a hash
/// of the source that decides what a build makes (see the build script), so
/// a code change, committed or not, never serves geometry the old code made.
pub(crate) fn engine_id() -> String {
    format!("{}+{}", env!("CARGO_PKG_VERSION"), env!("DT_SOURCE_HASH"))
}

/// The cache the environment names (`DT_CACHE`, `DT_CACHE_LIMIT`), or none
/// when it is switched off (`DT_CACHE=off`).
///
/// # Errors
/// No cache folder can be found, or the limit is not a size.
pub(crate) fn open() -> Result<Option<Store>, String> {
    Store::from_env(&engine_id())
}

/// `1.4 GB`, `312 MB`, `87 KB`.
pub(crate) fn size_text(bytes: u64) -> String {
    let b = bytes as f64;
    if b >= 1e9 {
        format!("{:.2} GB", b / 1e9)
    } else if b >= 1e6 {
        format!("{:.1} MB", b / 1e6)
    } else if b >= 1e3 {
        format!("{:.0} KB", b / 1e3)
    } else {
        format!("{bytes} B")
    }
}

#[expect(
    clippy::disallowed_methods,
    reason = "permitted silence: a clock before 1970 or a missing mtime reads as zero (a stamp, never data)"
)]
pub(crate) fn age_text(t: SystemTime) -> String {
    let s = SystemTime::now()
        .duration_since(t)
        .map_or(0, |d| d.as_secs());
    match s {
        0..=89 => format!("{s} s ago"),
        90..=5399 => format!("{} min ago", s / 60),
        5400..=129_599 => format!("{} h ago", s / 3600),
        _ => format!("{} days ago", s / 86_400),
    }
}

/// `dharmatiles cache [clear | list | previews]`.
pub(crate) fn run(action: Option<CacheAction>, json: bool) -> u8 {
    let store = match open() {
        Ok(Some(s)) => s,
        Ok(None) => {
            if json {
                crate::say_json(&CacheStatus::Off(CacheOff { enabled: false }));
            } else {
                say!("the build cache is off (DT_CACHE=off)");
            }
            return OK;
        }
        Err(e) => {
            tell!("error: {e}");
            return ERROR;
        }
    };
    match action {
        None => show(&store, json),
        Some(CacheAction::List) => list(&store, json),
        Some(CacheAction::Clear) => clear(&store, json),
        Some(CacheAction::Previews {
            system,
            printer,
            into,
        }) => previews(&store, system, &printer, into, json),
    }
}

fn show(store: &Store, json: bool) -> u8 {
    let stats = match store.stats() {
        Ok(s) => s,
        Err(e) => {
            tell!("error: {e}");
            return ERROR;
        }
    };
    if json {
        crate::say_json(&CacheStatus::Shown(CacheShown {
            enabled: true,
            folder: store.dir().display().to_string(),
            engine: store.engine().to_owned(),
            entries: stats.entries,
            bytes: stats.bytes,
            strays: stats.strays,
            limit_bytes: store.limit(),
        }));
        return OK;
    }
    say!("folder   {}", store.dir().display());
    say!("entries  {} ({})", stats.entries, size_text(stats.bytes));
    say!("limit    {} (DT_CACHE_LIMIT)", size_text(store.limit()));
    say!("engine   {}", store.engine());
    if stats.strays > 0 {
        say!(
            "strays   {} leftover file(s) (removed by the next eviction or a clear)",
            stats.strays
        );
    }
    OK
}

fn list(store: &Store, json: bool) -> u8 {
    let entries = match store.list() {
        Ok(e) => e,
        Err(e) => {
            tell!("error: {e}");
            return ERROR;
        }
    };
    if json {
        crate::say_json(
            &entries
                .iter()
                .map(|e| CacheEntryRow {
                    path: e.path.clone(),
                    target: e.target.clone(),
                    tier: e.tier.clone(),
                    engine: e.engine.clone(),
                    triangles: e.triangles,
                    bytes: e.bytes,
                    file: e.file.display().to_string(),
                })
                .collect::<Vec<_>>(),
        );
        return OK;
    }
    for e in &entries {
        say!(
            "{:>9}  {:<6} {:<10} {}  {} triangles, used {}{}",
            size_text(e.bytes),
            e.tier,
            e.target,
            e.path,
            e.triangles,
            age_text(e.used),
            if e.engine == store.engine() {
                String::new()
            } else {
                format!(", by {} (not this program's)", e.engine)
            }
        );
    }
    say!("{} entries", entries.len());
    OK
}

fn clear(store: &Store, json: bool) -> u8 {
    match store.clear() {
        Ok((n, bytes)) => {
            if json {
                crate::say_json(&CacheCleared { removed: n, bytes });
            } else {
                say!(
                    "cleared the build cache: {n} entries, {} ({})",
                    size_text(bytes),
                    store.dir().display()
                );
            }
            OK
        }
        Err(e) => {
            tell!("error: {e}");
            ERROR
        }
    }
}

/// `cache previews`: the super-low tier ([`dt_store::Tier::SuperLow`]) of every
/// library tile, kept as the library's previews: the cache, or with `--into`
/// a folder (laid out as the cache is, so an install copies it into the
/// cache).  A low build cut down to about 12 000 triangles, a fraction of
/// a megabyte a tile; the map draws it from far and the preview shows it
/// while a tile builds and nothing earlier of it is kept.
fn previews(
    store: &Store,
    system: dt_lang::System,
    printer: &str,
    into: Option<PathBuf>,
    json: bool,
) -> u8 {
    let opts = match crate::library::options(system, printer, None) {
        Ok(o) => o,
        Err(e) => {
            tell!("error: {e}");
            return ERROR;
        }
    };
    let tiles = match crate::library::library_tiles() {
        Ok(t) => t,
        Err(e) => {
            tell!("error: cache previews: {e}");
            return ERROR;
        }
    };
    let target = match into {
        Some(dir) => Store::new(dir, store.engine()).with_limit(u64::MAX),
        None => store.clone(),
    };
    let t0 = Instant::now();
    let mut code = OK;
    let mut report = Vec::new();
    for (n, p) in tiles.iter().enumerate() {
        let (c, r) = preview_one(&target, p, &opts, (n + 1, tiles.len()), json);
        code = crate::build::worse(code, c);
        report.push(r);
    }
    let total = t0.elapsed().as_secs_f64();
    if json {
        crate::say_json(&PreviewsReport {
            ok: code == OK,
            folder: target.dir().display().to_string(),
            tiles: report,
            total_s: total,
        });
    } else {
        tell!(
            "previews of {} tiles for {} in {} ({total:.1} s)",
            tiles.len(),
            opts.target(),
            target.dir().display()
        );
    }
    code
}

/// The preview of `path` kept in `store`: found there, or built (the
/// super-low tier) and kept.  Returns it, and what went wrong with the cache
/// on the way.
fn stored_preview(
    store: &Store,
    path: &str,
    opts: &Options,
) -> Result<(dt_eval::Built, Vec<String>), Vec<dt_eval::Diag>> {
    let mut b = dt_store::build_stored(
        &Files,
        path,
        &Options {
            tier: dt_store::Tier::SuperLow,
            ..opts.clone()
        },
        &Progress::none(),
        None,
        Some(store),
    )?;
    let problems = std::mem::take(&mut b.report.cache.problems);
    Ok((b, problems))
}

fn preview_one(
    store: &Store,
    path: &str,
    opts: &Options,
    (n, of): (usize, usize),
    json: bool,
) -> (u8, PreviewTile) {
    let t0 = Instant::now();
    let built = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        stored_preview(store, path, opts)
    }));
    match built {
        Ok(Ok((b, problems))) => {
            for problem in &problems {
                tell!("warning: {path}: {problem}");
            }
            let how = if b.report.cache.state == dt_eval::CacheState::Hit {
                "kept already"
            } else {
                "built"
            }
            .to_owned();
            if !json {
                tell!(
                    "[{n}/{of}] {path}: {how}, {} triangles, {:.2} s",
                    b.audit.triangles,
                    t0.elapsed().as_secs_f64()
                );
            }
            (
                OK,
                PreviewTile::Kept(PreviewKept {
                    path: path.to_owned(),
                    ok: true,
                    how,
                    triangles: b.audit.triangles,
                    problems,
                }),
            )
        }
        Ok(Err(errs)) => {
            if !json {
                crate::report(&errs);
            }
            (
                ERROR,
                PreviewTile::Failed(Failed::of(path, dt_eval::mistakes(&errs))),
            )
        }
        Err(panic) => {
            let msg = crate::preview::panic_message(panic.as_ref());
            tell!(
                "error: {path}: the build panicked (a bug in dharmatiles, not in the tile): {msg}"
            );
            (
                BUG,
                PreviewTile::Failed(Failed::of(
                    path,
                    dt_eval::mistakes(&[dt_eval::Diag::plain(dt_lang::codes::PANIC, msg)]),
                )),
            )
        }
    }
}
