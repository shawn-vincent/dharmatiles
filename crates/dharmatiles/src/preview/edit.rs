//! The editor's calls (`POST`, with this run's token; see `server`):
//!
//! - `/api/edit` `{text, at, name, value}`: set a control on the node whose
//!   name starts at byte `at`, in whichever form it is written, keeping
//!   everything else byte for byte; `value: null` takes it out.  With
//!   `op: insert` `{place: stack|tile, node}` it writes a new node (at the
//!   end of the stack, or before it) and says at which byte; with
//!   `place: before` and `at` it writes the node on a line of its own above
//!   the node at `at` (an entry between two others); with `op: move`
//!   `{at, before?}` it moves the node at `at` whole (its comments and
//!   children) to stand before its sibling at byte `before`, or last, and
//!   says at which byte it now starts; with
//!   `op: remove` it takes out the node at `at`; with `place: in` and `at`
//!   it writes `node` as the last child of the node at `at` (a clause of a
//!   region), and `op: remove-children` `{at, names}` takes out that node's
//!   children called one of `names` (a region's `boundedBy` and `fill`).  Nothing is written to
//!   disk: the page saves the result.
//! - `/api/tiles/ID/offers` `{text, at, file?}`: what the Add menu may offer
//!   at place `at` of the stack, each with the node it would write and
//!   whether the plan lets it stand there (see `offers`).
//! - `/api/names` `{text}`: the definitions, regions, lines and points the
//!   text makes, read by the parser (`{ok, defs, regions, lines, points}`),
//!   for completion between checks.
//! - `/api/check-value` `{element, control, typed}`: what the reader would
//!   refuse in `typed` for that control (`{message, label, help}`, or
//!   `{message: null}`), by the same check as a written value, so the sheet
//!   draws the program's answer and checks nothing itself.
//! - `/api/tiles/ID/check` `{text, file?}`: read and plan the tile with
//!   `text` in place of its document (or of `file`, one it uses): the
//!   errors, the stack and the map (see `map`), under a second, for the
//!   text pane and the overlay as you type.
//! - `/api/tiles/ID/save` `{text, base, file?}`: write the document (or a
//!   file it uses) when what is on disk is still `base`; else `409` with
//!   what is there.  Library files are never written.  The worker sees the
//!   new file and rebuilds.
//! - `/api/tiles/ID/mine`: copy a library tile into My tiles, its `use`s
//!   pointing back at the library, and return the copy's id and state.
//! - `/api/tiles/ID/delete`: delete a tile's file from a folder the preview
//!   lists (never the library): the editor's Undo of the copy it just made.
//! - `/api/tiles/ID/rename` `{name}`: rename the tile's file in its folder
//!   (a tile's file name is its name), never over another file, and
//!   return the new path, id and state (a new path is a new id).  Library
//!   tiles are not renamed.
//! - `/api/new` `{name, size, root?, dir?}`: make a flat, printable tile
//!   called `name` in My tiles (or in folder `dir` of place `root`), never
//!   over another file.
//! - the places and what is done in them: add and remove a place, new, rename and
//!   trash a folder, move, duplicate and trash a tile, undo a trash (see `places`).
//! - `/api/tiles/ID/draft` `{text, file?}`: build a draft of the tile from
//!   `text` (see `dt_eval::Options::draft`) ahead of any full build, and
//!   answer with its mesh: `200` and the binary buffer, `422` when the text
//!   has a mistake, `409` when a newer draft of the tile took its place.
//!   Nothing is written.
//! - `/api/tiles/ID/step` `{edit, file?, debug?}`: one step of a drag or a
//!   scrub: the edit (as `/api/edit` makes it, `edit.read` and all) and the
//!   draft of the text it made, in one request.  The answer is the head (the
//!   new text, its revision, what became of the draft) as JSON after its
//!   length in four bytes, then the mesh.  Nothing is written.

use std::collections::BTreeMap;
use std::hash::{BuildHasher, Hasher};
use std::path::{Path, PathBuf};

use dt_eval::Source;
use serde::{Deserialize, Serialize};

use super::api::{Out, Reply, TileRef, TileRefReply};
use super::places::{Listed, Nothing};
use super::roots::personal_home;
use super::*;
pub use dt_service::api::DraftTier;
pub use dt_service::draft_calls::{DraftReq, StepReq};
#[cfg(test)]
pub use dt_service::draft_calls::{StepDraft, StepHead};
#[cfg(test)]
pub(super) use dt_service::tile_calls::text_rev;
pub use dt_service::tile_calls::{CheckRes, TextReq};

// ---- the calls' types ------------------------------------------------------------------
// The edit family (`edit`, `save`) is the service's: its types and its handlers are in `dt_service::edit_calls`.

#[cfg(test)]
pub use dt_service::edit_calls::{EditOp, InsertPlace, PointSlot, Scalar};
pub use dt_service::edit_calls::{EditReq, EditRes, SaveReq, SaveRes};

pub use dt_service::folder_calls::{Deleted, RenameReq};

pub use dt_service::place_calls::NewReq;

// The reading calls' types (names, tokens, checkValue, printed, printedSet) are the service's: `dt_service::read_calls`.
pub use dt_service::read_calls::{
    CheckValueReq, CheckValueRes, NamesReq, NamesRes, PrintedReq, PrintedRes, PrintedSetReq,
    PrintedSetRes, TokensReq, TokensRes,
};
#[cfg(test)]
pub use dt_service::read_calls::{PrintedItem, PrintedSize};

pub use dt_service::target_calls::{TargetReq, TargetRes};

#[derive(Deserialize, Serialize, Debug)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(optional_fields = nullable))]
#[serde(deny_unknown_fields)]
pub struct RevealReq {
    pub path: String,
}

pub use dt_service::read_calls::Done;

/// `/api/messages`: the page reports a failure of its own.
#[derive(Deserialize, Serialize, Debug)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(optional_fields = nullable))]
#[serde(deny_unknown_fields)]
pub struct PageMessageReq {
    pub tile: Option<String>,
    pub what: Option<String>,
    pub text: String,
    pub help: Option<String>,
    pub causes: Option<Vec<String>>,
    pub backtrace: Option<String>,
}

pub use dt_service::export_calls::{ExportBuilt, ExportReq, ExportRes};

/// 128 random bits, hex: the run's token.
#[expect(
    clippy::disallowed_methods,
    reason = "permitted silence: a clock before 1970 or a missing mtime reads as zero (a stamp, never data)"
)]
pub(super) fn token() -> String {
    let word = |salt: u64| {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u64(salt);
        h.write_u128(
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos()),
        );
        h.write_u32(std::process::id());
        h.finish()
    };
    format!("{:016x}{:016x}", word(1), word(2))
}

/// The text of a file, overlaid on the disk: the document being edited.
pub(super) struct Overlay(pub BTreeMap<String, String>);

impl Source for Overlay {
    fn read(&self, path: &str) -> Result<String, String> {
        self.0
            .get(path)
            .cloned()
            .map_or_else(|| dt_eval::Files.read(path), Ok)
    }
}

pub(super) fn bad(msg: impl Into<String>) -> Reply {
    fail::refused("400 Bad Request", msg.into())
}

/// Route one editor call: see [`super::calls`].
#[cfg(test)]
pub(super) fn post(shared: &Shared, path: &str, body: &[u8]) -> super::route::Reply {
    super::calls::dispatch(shared, path, body)
}

/// `/api/messages` `{tile?, what, text, help?, causes?, backtrace?}`: the page
/// reports a failure of its own, so it joins the stream (kind `server`, origin
/// `page`) beside the program's, where an assistant reads it too.
pub(super) fn page_message(shared: &Shared, v: PageMessageReq) -> Out<Done> {
    let tile = v.tile.as_deref().unwrap_or("");
    let name = shared.name_of(tile).unwrap_or_default();
    let error = api::ErrorDetail {
        kind: "bug".to_owned(),
        message: v.text,
        help: v.help,
        causes: v.causes.unwrap_or_default(),
        backtrace: v.backtrace.unwrap_or_default(),
        context: api::ErrorContext {
            route: v.what,
            tile: None,
            version: env!("DT_ENGINE_VERSION").to_owned(),
            at_ms: 0,
        },
    };
    shared.say(messages::of_server_error(
        &error,
        (tile, &name),
        dt_service::message::Origin::Page,
    ));
    Ok(Done {})
}

/// `/api/target` (see [`dt_service::target_calls::set_target`]).
pub(super) fn target(shared: &Shared, v: TargetReq) -> Out<TargetRes> {
    super::api::out(dt_service::target_calls::set_target(shared, v))
}

pub(super) fn listed(shared: &Shared, dest: &Path) -> Listed {
    shared.rescan();
    let path = dest.to_string_lossy().into_owned();
    let id = tile_id(&path);
    let tile = shared.summary(&id);
    if tile.is_some() {
        shared.want(&id);
    }
    Listed {
        path,
        id: tile.as_ref().map(|_| id),
        tile,
    }
}

pub(super) fn home(shared: &Shared) -> Result<PathBuf, (&'static str, super::api::Body)> {
    shared.home_dir().map_err(super::api::refusal_reply)
}

impl Shared {
    /// My tiles: the first folder place (else the personal home), made when it is not there yet.
    pub(super) fn home_dir(&self) -> Result<PathBuf, dt_service::call::Refusal> {
        use dt_service::call::{Refusal, Status};
        let home = self
            .roots()
            .iter()
            .find_map(|r| match &r.kind {
                RootKind::Dir(d) => Some(d.clone()),
                _ => None,
            })
            .map_or_else(
                || personal_home().map_err(|e| Refusal::failure(Status::Internal, e)),
                Ok,
            )?;
        self.storage.create_dir_all(&home).map_err(|e| {
            Refusal::failure(
                Status::Internal,
                format!("cannot make {}: {e}", home.display()),
            )
            .caused_by(&e)
        })?;
        Ok(home)
    }
}

/// `/api/tiles/ID/rename` (see [`dt_service::folder_calls::rename`]).
pub(super) fn rename(shared: &Shared, tile: &TileRef, v: RenameReq) -> Out<Listed> {
    super::api::out(dt_service::folder_calls::rename(shared, tile, v))
}

/// `/api/tiles/ID/delete` (see [`dt_service::folder_calls::delete_tile`]).
pub(super) fn delete(shared: &Shared, tile: &TileRef, n: Nothing) -> Out<Deleted> {
    super::api::out(dt_service::folder_calls::delete_tile(shared, tile, n))
}

/// Make a small, valid tile in My tiles (or in a folder of a place) and return its slot (see
/// [`dt_service::place_calls::new_tile`]).
pub(super) fn new_tile(shared: &Shared, v: NewReq) -> Out<Listed> {
    // in a folder of a place (the page's folder level), else in My tiles
    let into = super::api::out(dt_service::folder_calls::new_tile_dir(
        shared,
        v.root,
        v.dir.as_deref(),
    ))?;
    super::api::out(dt_service::place_calls::new_tile(shared, &v, into))
}

/// `/api/names` (see [`dt_service::read_calls::names`]).
pub(super) fn names(_: &Shared, v: NamesReq) -> Out<NamesRes> {
    Ok(dt_service::read_calls::names(&v))
}

/// `/api/tokens` (see [`dt_service::read_calls::tokens`]).
pub(super) fn tokens(_: &Shared, v: TokensReq) -> Out<TokensRes> {
    super::api::out(dt_service::read_calls::tokens(&v))
}

/// `/api/check-value` (see [`dt_service::read_calls::check_value`]).
pub(super) fn check_value(shared: &Shared, v: CheckValueReq) -> Out<CheckValueRes> {
    Ok(dt_service::read_calls::check_value(shared, &v))
}

/// `/api/tiles/ID/printed` (see [`dt_service::read_calls::printed`]), reading the tile's text from the request.
pub(super) fn printed(shared: &Shared, t: &TileRef, v: PrintedReq) -> Out<PrintedRes> {
    let src = Overlay(BTreeMap::from([(t.path.clone(), v.text.clone())]));
    Ok(dt_service::read_calls::printed(shared, &src, &t.path, &v))
}

/// `/api/printed-set` (see [`dt_service::read_calls::printed_set`]).
pub(super) fn printed_set(shared: &Shared, v: PrintedSetReq) -> Out<PrintedSetRes> {
    super::api::out(dt_service::read_calls::printed_set(shared, &v))
}

/// `/api/edit`: [`dt_service::edit::apply`] with this program's reader, and its failures as the replies the page reads.
pub(super) fn edit(shared: &Shared, req: EditReq) -> Out<EditRes> {
    super::api::out(dt_service::edit_calls::edit(shared, req))
}

pub(super) fn check(shared: &Shared, t: &TileRef, v: TextReq) -> Out<CheckRes> {
    let file = t.file(v.file.as_deref())?;
    let (tile, file, text) = (t.path.as_str(), file.as_str(), v.text);
    // a print list or the inventory is read by its own decoder: its mistakes, at their text
    if file == tile && (super::plates::is_plates(tile) || super::inventory::is_inventory(tile)) {
        return Ok(super::lists::check_text(shared, tile, &text));
    }
    let src = Overlay(BTreeMap::from([(file.to_owned(), text)]));
    dt_service::tile_calls::check(shared, &src, tile)
        .map_err(|e| fail::failure("500 Internal Server Error", e.0))
}

/// `/api/tiles/ID/save` `{text, docRev, file?}` (see [`dt_service::edit_calls::save`]).
pub(super) fn save(shared: &Shared, t: &TileRef, v: SaveReq) -> Out<SaveRes> {
    super::api::out(dt_service::edit_calls::save(shared, t, v))
}

pub(super) fn mine(shared: &Shared, t: &TileRef, n: Nothing) -> Out<Listed> {
    super::api::out(dt_service::place_calls::mine(shared, t, n))
}

/// Export: the same build as `dharmatiles build` (the saved file, through
/// the gate), written to [`export_dir`].  With `where`, only says where.
/// It builds for the target the page chose (the title bar's chip);
/// `threemf` adds the painted 3MF.
pub(super) fn export(shared: &Shared, t: &TileRef, v: ExportReq) -> Out<ExportRes> {
    // asking where is no build; a build waits its turn on the export lane
    if v.ask_where == Some(true) {
        return export_now(shared, t, &v);
    }
    let (tile, rev) = (t.clone(), shared.rev_of(&t.path));
    lane_out(shared.on_lane(
        queue::Lane::Export,
        format!("export|{}", t.id),
        rev,
        &t.id,
        move |reply| queue::SideJob::ExportTile {
            tile,
            req: v,
            reply,
        },
    ))
}

/// What a request gets for an export that ran on its lane (see [`export`]): its answer; a 409 when a newer one took its
/// place; a panic raised again.
pub(super) fn lane_out<T>(r: Result<Out<T>, queue::Interrupted>) -> Out<T> {
    match r {
        Ok(out) => out,
        Err(queue::Interrupted::Superseded) => Err(fail::refused(
            "409 Conflict",
            "a newer request for this took its place",
        )),
        Err(queue::Interrupted::Panicked(p)) => std::panic::resume_unwind(p),
    }
}

#[expect(
    clippy::disallowed_methods,
    reason = "permitted silence: a path outside its root stays as written (display only)"
)]
pub(super) fn export_now(shared: &Shared, t: &TileRef, v: &ExportReq) -> Out<ExportRes> {
    let tile = t.path.as_str();
    let threemf = v.threemf.unwrap_or(false);
    let template = if threemf {
        crate::bambu::template(None).map_err(|e| fail::failure("500 Internal Server Error", e))?
    } else {
        None
    };
    let out = export_dir(tile).map_err(|e| fail::failure("500 Internal Server Error", e))?;
    if v.ask_where == Some(true) {
        // shown with ~ for the home folder
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let shown = home
            .as_deref()
            .and_then(|h| out.strip_prefix(h).ok())
            .map_or_else(
                || out.display().to_string(),
                |r| format!("~/{}", r.display()),
            );
        return Ok(ExportRes::Where {
            out: out.display().to_string(),
            shown,
        });
    }
    let palette = shared
        .palette_checked()
        .map_err(|e| fail::failure("500 Internal Server Error", e))?;
    let extras = crate::build::Extras {
        progress: false,
        tile_no: None,
        png: false,
        threemf,
        template: template.as_deref(),
        palette: &palette,
        cache: None,
        store: shared.store.as_deref(),
    };
    let opts = dt_eval::Options {
        tier: dt_store::Tier::Full,
        ..shared.options()
    };
    let target = opts.target();
    let t0 = std::time::Instant::now();
    let mut b = match dt_store::build_stored(
        &dt_eval::Files,
        tile,
        &opts,
        &dt_eval::Progress::none(),
        None,
        shared.store.as_deref().map(dt_store::Store::kept),
    ) {
        Ok(b) => b,
        Err(errs) => {
            let id = tile_id(tile);
            shared.report(&id, messages::Scope::Export, |name| {
                messages::of_errors(&errs, messages::Scope::Export, &id, name, &[])
            });
            return Err(fail::mistakes(&errs));
        }
    };
    // a file of yours goes to out/<target>/<name>-<target>.stl, not under
    // the folders it sits in (a library tile mirrors the library, as build does)
    dt_service::export_calls::place_out(&mut b, tile, &target);
    let built_s = t0.elapsed().as_secs_f64();
    let record = dt_eval::TileRecord::of(&b, &opts.printer, built_s);
    let files = crate::build::write(
        &b,
        &b.verdict(&opts.printer),
        &record,
        &out,
        extras,
        opts.tier,
    )
    .map_err(|e| fail::failure("500 Internal Server Error", e))?;
    let written: Vec<String> = files.iter().map(|f| f.display().to_string()).collect();
    let id = tile_id(tile);
    shared.report(&id, messages::Scope::Export, |name| {
        vec![messages::of_export(&record, &written, &id, name)]
    });
    Ok(ExportRes::Built(ExportBuilt {
        path: tile.to_owned(),
        files: written,
        downloads: Vec::new(),
        build: Box::new(record),
    }))
}

/// Where `tile` exports to: never the library.  A library tile (`lib:`,
/// or a file inside the library folder) goes to `out/` in My tiles; any
/// other file to its project's `out/` (beside the nearest
/// `dharmatiles.dtile` above it), else `out/` beside it.
///
/// # Errors
/// When the library folder or the tile's folder cannot be resolved to compare
/// them, or there is no personal home: a guess could write into the library.
pub(super) fn export_dir(tile: &str) -> Result<PathBuf, String> {
    let in_library = if tile.starts_with("lib:") {
        true
    } else if let Some(lib) = dt_eval::source::library_dir() {
        let l = lib
            .canonicalize()
            .map_err(|e| format!("cannot resolve the library folder {}: {e}", lib.display()))?;
        let folder = Path::new(tile)
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let f = folder
            .canonicalize()
            .map_err(|e| format!("cannot resolve the folder of {tile}: {e}"))?;
        f.starts_with(l)
    } else {
        false
    };
    if in_library {
        return Ok(personal_home()?.join("out"));
    }
    let dir = Path::new(tile).parent().unwrap_or_else(|| Path::new("."));
    let project = dt_eval::discover(dir);
    if let Some(why) = &project.problem {
        super::tell_once(&format!(
            "note: {why}; the project file is ignored, so exports go to out/"
        ));
    }
    Ok(project.out)
}

/// Show a written file in the Finder (macOS `open -R`; elsewhere the
/// folder is opened).  Only an STL or 3MF that exists.
pub(super) fn reveal(_: &Shared, v: RevealReq) -> Out<Done> {
    let p = PathBuf::from(&v.path);
    let kind = p.extension().and_then(|e| e.to_str()).unwrap_or("");
    if !matches!(kind, "stl" | "3mf") || !p.is_file() {
        return Err(bad("only a written STL or 3MF"));
    }
    let r = if cfg!(target_os = "macos") {
        std::process::Command::new("open")
            .arg("-R")
            .arg(&p)
            .status()
    } else {
        let dir = p.parent().unwrap_or_else(|| Path::new("."));
        std::process::Command::new(if cfg!(windows) {
            "explorer"
        } else {
            "xdg-open"
        })
        .arg(dir)
        .status()
    };
    r.map_err(|e| fail::failure("500 Internal Server Error", e.to_string()))?;
    Ok(Done {})
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_named(shared: &Shared, name: &str) -> Listed {
        new_tile(
            shared,
            NewReq {
                name: Some(name.to_owned()),
                size: None,
                root: None,
                dir: None,
            },
        )
        .unwrap()
    }

    fn tile_of(path: &str) -> TileRef {
        TileRef {
            id: tile_id(path),
            path: path.to_owned(),
            files: BTreeMap::new(),
        }
    }

    fn status(r: Out<impl Sized>) -> &'static str {
        match r {
            Ok(_) => "200 OK",
            Err(e) => e.0,
        }
    }

    #[test]
    fn a_new_tile_is_safe_named_valid_and_listed() {
        let dir = std::env::temp_dir().join(format!("dt-new-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let roots = vec![Root::new("My tiles", RootKind::Dir(dir.clone()))];
        let shared = Shared::with_roots(roots, &[], &dt_eval::Options::default());
        let made = new_tile(
            &shared,
            NewReq {
                name: Some("Shore / first try".to_owned()),
                size: Some("2x1".to_owned()),
                root: None,
                dir: None,
            },
        )
        .unwrap();
        let path = made.path.as_str();
        assert!(path.ends_with("Shore - first try.dtile"), "{path}");
        let text = std::fs::read_to_string(path).unwrap();
        assert!(text.contains("tile size=\"2x1\""));
        assert!(text.contains("base\n        dirt\n"));
        dt_eval::check(&dt_eval::Files, path, &Options::default()).unwrap();
        assert!(made.id.is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_delete_takes_out_a_tile_of_a_listed_folder_and_nothing_else() {
        let dir = std::env::temp_dir().join(format!("dt-delete-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let roots = vec![Root::new("My tiles", RootKind::Dir(dir.clone()))];
        let shared = Shared::with_roots(roots, &[], &dt_eval::Options::default());
        let pond = new_named(&shared, "Pond").path;
        let r = delete(
            &shared,
            &tile_of("lib:tests/ground/1x1-flat.dtile"),
            Nothing {},
        );
        assert_eq!(status(r), "400 Bad Request");
        let outside = std::env::temp_dir().join(format!("dt-outside-{}.dtile", std::process::id()));
        std::fs::write(&outside, "x").unwrap();
        let r = delete(&shared, &tile_of(&outside.to_string_lossy()), Nothing {});
        assert_eq!(status(r), "400 Bad Request");
        assert!(outside.exists());
        let _ = std::fs::remove_file(&outside);
        let r = delete(&shared, &tile_of(&pond), Nothing {});
        assert_eq!(status(r), "200 OK");
        assert!(!Path::new(&pond).exists());
        assert!(
            shared.summary(&tile_id(&pond)).is_none(),
            "the list drops it at once"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_rename_moves_the_file_and_never_clobbers() {
        let dir = std::env::temp_dir().join(format!("dt-rename-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let roots = vec![Root::new("My tiles", RootKind::Dir(dir.clone()))];
        let shared = Shared::with_roots(roots, &[], &dt_eval::Options::default());
        let pond = new_named(&shared, "Pond").path;
        let marsh = new_named(&shared, "Marsh").path;
        let name = |n: &str| RenameReq { name: n.to_owned() };

        let moved = rename(&shared, &tile_of(&pond), name("Still lake")).unwrap();
        let lake = moved.path.clone();
        assert!(lake.ends_with("Still lake.dtile"), "{lake}");
        assert!(!Path::new(&pond).exists());
        assert!(moved.id.is_some());

        let r = rename(&shared, &tile_of(&marsh), name("Still lake"));
        assert_eq!(status(r), "409 Conflict");
        assert!(Path::new(&marsh).exists());

        let r = rename(
            &shared,
            &tile_of("lib:tests/ground/1x1-flat.dtile"),
            name("x"),
        );
        assert_eq!(status(r), "400 Bad Request");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
