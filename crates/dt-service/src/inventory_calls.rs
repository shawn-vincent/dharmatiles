//! The inventory in the editor: `inventory.dinv` in My tiles, read and added to through the host's files.
//!
//! The document and its counting are `dt_doc::inventory`.  The names a log uses for a tile are `dt_print::inventory`'s
//! (pure: `canonical`, `readable`).  This is what needs files: where the log is read and written (the host's
//! [`Docs`], so a write is a revision like any document's, and a store that is memory serves as well as the disk),
//! whether a tile changed since it was printed, and the calls of the page and of `dharmatiles inventory`:
//!
//! - the **snapshot** carries the inventory (what each piece adds up to), so the folder rows' pills, the tile level's
//!   "N printed" line and the plates' "N in inventory" all come from one place and change together;
//! - opening the **document** shows the pieces grouped by tile, with its text for Source; a file with mistakes is a
//!   failed build whose errors are at their text ([`rebuild`]);
//! - the **calls** append lines to the log: "I have this" (`inventoryHave`), the sheet after a print
//!   (`inventoryRecord`), `inventoryGone`, a re-link (`inventoryRelink`), and a reprint into a print list
//!   (`inventoryReprint`).  Nothing here edits a line away; every call is one or more lines added at the end, and a file
//!   that is not understood is never added to.
//!
//! One reader and one writer of the format ([`load`], [`append`]) serve the program's editor, the command line and the
//! web worker.  The page's side is `js/editor/inventory.js` and the pieces in `plates.js`, `view/places-panel.js` and
//! `levels.js`.  Docs: docs/inventory.html.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use dt_doc::inventory::{self as doc, Colour, Identity, Inventory};
use dt_doc::map::Mode;
use dt_eval::{Diag, Source};
use dt_print::inventory as names;
use serde::{Deserialize, Serialize};

use crate::call::{Refusal, Status, Verb};
use crate::docs::{DocFail, Docs};
pub use crate::inventory::InventoryWrote;
use crate::inventory::{ColourWord, InventoryRow, InventoryState};
use crate::list_calls::{is_inventory, is_plates};
use crate::message::{Kind, Message, MessageData, Scope, Severity};
use crate::patch::Patch;
use crate::plan::{InventoryDoc, InventoryRead, PlatesPanel, TilePlan};
use crate::plates_calls::PlatesHost;
use crate::scene_read::SceneHost;
use crate::tile::tile_id;

// ---- the calls' types ------------------------------------------------------------------

/// `inventoryHave`: one good print (or `count`) of the tile, at the tile's own seed unless one is given.
#[derive(Deserialize, Serialize, Debug, ts_rs::TS)]
#[ts(optional_fields = nullable)]
#[serde(deny_unknown_fields)]
pub struct InventoryHaveReq {
    pub tile: String,
    pub colour: ColourWord,
    pub system: Option<dt_lang::System>,
    pub seed: Option<u64>,
    pub count: Option<u64>,
    /// The date, like 2026-11-09 (today when left out).
    pub on: Option<String>,
}

/// `inventoryRecord`: the sheet after a print.
#[derive(Deserialize, Serialize, Debug, ts_rs::TS)]
#[ts(optional_fields = nullable)]
#[serde(deny_unknown_fields)]
pub struct InventoryRecordReq {
    pub list: String,
    /// The plate, from 1.
    pub plate: usize,
    /// The pieces that failed, by their place in the plate's list.
    pub failed: Option<Vec<usize>>,
    pub on: Option<String>,
}

/// `inventoryGone`: one of the piece is lost or broken.
#[derive(Deserialize, Serialize, Debug, ts_rs::TS)]
#[ts(optional_fields = nullable)]
#[serde(deny_unknown_fields)]
pub struct InventoryGoneReq {
    pub tile: String,
    pub seed: u64,
    pub system: dt_lang::System,
    pub colour: ColourWord,
    pub note: Option<String>,
    pub on: Option<String>,
}

/// `inventoryRelink`: a tile has another path now.
#[derive(Deserialize, Serialize, Debug, ts_rs::TS)]
#[ts(optional_fields = nullable)]
#[serde(deny_unknown_fields)]
pub struct InventoryRelinkReq {
    /// The old path, as the log has it.
    pub from: String,
    /// The id of the listed tile it is now.
    pub to: String,
    pub on: Option<String>,
}

/// `inventoryReprint`: a piece put in a print list again.
#[derive(Deserialize, Serialize, Debug, ts_rs::TS)]
#[ts(optional_fields = nullable)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct InventoryReprintReq {
    pub tile: String,
    pub seed: u64,
    pub system: dt_lang::System,
    pub colour: ColourWord,
    /// The print list's id; none makes a new list.
    pub list: Option<String>,
    /// The revision of the list the caller last saw, when it has the list open.
    pub doc_rev: Option<u64>,
}

/// A piece put in a print list again.
#[derive(Serialize, Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct InventoryReprinted {
    /// The list's path.
    pub list: String,
    pub id: String,
    pub made: bool,
    pub warning: Option<String>,
    pub always: bool,
    pub before: Option<String>,
    pub text: String,
    pub doc_rev: u64,
    pub note: Option<String>,
}

pub const INVENTORY_HAVE: Verb<InventoryHaveReq, InventoryWrote> = Verb::new("inventoryHave");
pub const INVENTORY_RECORD: Verb<InventoryRecordReq, InventoryWrote> = Verb::new("inventoryRecord");
pub const INVENTORY_GONE: Verb<InventoryGoneReq, InventoryWrote> = Verb::new("inventoryGone");
pub const INVENTORY_RELINK: Verb<InventoryRelinkReq, InventoryWrote> = Verb::new("inventoryRelink");
pub const INVENTORY_REPRINT: Verb<InventoryReprintReq, InventoryReprinted> =
    Verb::new("inventoryReprint");

/// What the inventory's calls ask of whoever runs them, beyond what a list is planned with ([`PlatesHost`]).
pub trait InventoryHost: PlatesHost {
    /// Today's date as the log writes it, `2026-11-12`, in UTC (the editor sends its own local day).
    fn today(&self) -> String;

    /// One message with nothing to clear (a note of the server's or a call's), said now.
    fn say(&self, m: Message);

    /// The plan of tile or list `id` as the owner holds it.
    fn plan_of(&self, id: &str) -> Option<TilePlan>;
}

// ---- the file --------------------------------------------------------------------------

/// Where the inventory is: `inventory.dinv` in `home`.
#[must_use]
pub fn path_in(home: &Path) -> PathBuf {
    names::path_in(home)
}

/// An inventory read from the host's files, with its text.
#[derive(Debug, Clone)]
pub struct Loaded {
    /// The file.
    pub path: PathBuf,
    /// Its text.
    pub text: String,
    /// What it holds.
    pub inventory: Inventory,
}

fn plain(message: impl Into<String>) -> Vec<Diag> {
    vec![Diag::plain(dt_lang::codes::INVENTORY, message)]
}

/// Read the inventory in `home`.  `Ok(None)` when there is no file: no inventory yet, and nothing is made until the
/// first write.
///
/// # Errors
/// The file is there and cannot be read, or has mistakes: each at its text.
pub fn load(docs: &Docs, home: &Path) -> Result<Option<Loaded>, Vec<Diag>> {
    let path = path_in(home);
    let text = match docs.read(&path.to_string_lossy()) {
        Ok((t, _)) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(plain(format!("cannot read {}: {e}", path.display()))),
    };
    let inventory = doc::decode(&path.display().to_string(), &text)
        .map_err(|e| e.iter().map(Diag::from).collect::<Vec<_>>())?;
    Ok(Some(Loaded {
        path,
        text,
        inventory,
    }))
}

/// Add `lines` to the end of the log in `home`, making the file (with its header, `version` stamped) when this is the
/// first write.  Written whole and never half a line.
///
/// # Errors
/// The file has mistakes (nothing is added to a file that is not understood, and the mistakes are the answer), or
/// cannot be written.
pub fn append(
    docs: &Docs,
    version: &str,
    home: &Path,
    lines: &[String],
) -> Result<PathBuf, Vec<Diag>> {
    let path = path_in(home);
    let key = path.to_string_lossy();
    let exists = match docs.exists(&key) {
        Ok(e) => e,
        Err(e) => return Err(plain(format!("cannot read {}: {e}", path.display()))),
    };
    if !exists {
        let text = doc::new_text(&dt_doc::header::stamp(version), lines);
        docs.create(&key, &text)
            .map_err(|e| plain(format!("cannot write {}: {e}", path.display())))?;
        return Ok(path);
    }
    docs.edit(&key, None, |text| {
        match dt_doc::ops::apply(
            dt_doc::ops::Kind::Inventory,
            text,
            &dt_doc::ops::Op::Append(lines),
        ) {
            Ok(d) => Ok((d.text, ())),
            Err(dt_doc::ops::EditError::Unread(e)) => {
                Err(e.iter().map(Diag::from).collect::<Vec<_>>())
            }
            Err(e) => Err(plain(e.to_string())),
        }
    })
    .map_err(|e| match e {
        DocFail::Caller(diags) => diags,
        DocFail::Service(r) => plain(r.message),
    })?;
    Ok(path)
}

/// The hash of the tile `canon` as it is now, as the log writes it; `None` when the tile cannot be read (it is not there:
/// renamed, moved or deleted).
#[expect(
    clippy::disallowed_methods,
    reason = "permitted silence: a best-effort probe (None is the meaning: the tile is renamed, moved or deleted)"
)]
#[must_use]
pub fn tile_hash(source: &dyn Source, canon: &str, home: Option<&Path>) -> Option<String> {
    let path = names::readable(canon, home)?;
    let text = source.read(&path).ok()?;
    Some(names::short_hash(dt_eval::doc::text_hash(&text)))
}

/// Is the tile different from when it was printed?  False when the print recorded no hash or the tile cannot be read (the
/// latter is its own question: a path that is gone).
#[must_use]
pub fn edited_since(
    source: &dyn Source,
    canon: &str,
    recorded: Option<&str>,
    home: Option<&Path>,
) -> bool {
    match (recorded, tile_hash(source, canon, home)) {
        (Some(r), Some(now)) => !now.starts_with(r) && !r.starts_with(&now),
        _ => false,
    }
}

/// A tile named for the log: how the log names it, its seed and its hash.
#[derive(Debug, Clone)]
pub struct Named {
    /// `lib:tiles/ground/1x1-grass`, `home:meadows/glade`.
    pub canon: String,
    /// The seed (`seed`, else the tile's own).
    pub seed: u64,
    /// The first eight hex digits of the document's hash.
    pub hash: String,
}

/// Read a tile the way a print list does (`lib:`, `home:`, or a path), check it for `system` and say how the log names it.
///
/// # Errors
/// The path does not resolve, the tile is not there or does not check.
pub fn name_tile(
    source: &dyn Source,
    options: &dt_eval::Options,
    tile: &str,
    dir: Option<&Path>,
    home: Option<&Path>,
    system: dt_lang::System,
    seed: Option<u64>,
) -> Result<Named, Vec<Diag>> {
    let path = dt_print::plates::tile_path(tile, dir, home).map_err(plain)?;
    let text = source.read(&path).map_err(plain)?;
    let opts = dt_eval::Options {
        system,
        seed,
        ..options.clone()
    };
    let checked = dt_eval::check(source, &path, &opts)?;
    if checked.tile.is_none() {
        return Err(plain(format!("`{tile}` is not a tile")));
    }
    Ok(Named {
        canon: names::canonical(&path, home),
        seed: checked.seed,
        hash: names::short_hash(dt_eval::doc::text_hash(&text)),
    })
}

/// The colour mode last used in the log (what `have` assumes), else single.
#[must_use]
pub fn last_colour(inv: &Inventory) -> Colour {
    inv.entries
        .iter()
        .rev()
        .find_map(|e| match e {
            doc::Entry::Print { id, .. } | doc::Entry::Gone { id, .. } => Some(id.colour),
            doc::Entry::Renamed { .. } => None,
        })
        .unwrap_or(Colour::Single)
}

/// The inventory's rows as the page and `inventory list --json` read them: every piece with what is held, and whether its
/// tile changed or is gone.
#[must_use]
pub fn pieces_of(inv: &Inventory, source: &dyn Source, home: &Path) -> Vec<InventoryRow> {
    inv.pieces()
        .into_iter()
        .map(|p| InventoryRow {
            edited: p.have > 0 && edited_since(source, &p.id.tile, p.hash.as_deref(), Some(home)),
            missing: tile_hash(source, &p.id.tile, Some(home)).is_none(),
            colour: p.id.colour.into(),
            tile: p.id.tile,
            seed: p.id.seed,
            system: p.id.system,
            have: p.have,
            printed: p.printed,
            failed: p.failed,
            gone: p.gone,
            hash: p.hash,
            last: p.last,
            lines: p.lines,
        })
        .collect()
}

/// The inventory in `home`, as its state: absent until the first line is written, or read.  A file that does not read is
/// the diagnostics it made.
///
/// # Errors
/// The file is there and cannot be read, or has mistakes.
pub fn state_of(
    docs: &Docs,
    source: &dyn Source,
    home: &Path,
) -> Result<InventoryState, Vec<Diag>> {
    let path = path_in(home);
    let path_text = path.to_string_lossy().into_owned();
    let id = tile_id(&path_text);
    let home_text = home.to_string_lossy().into_owned();
    Ok(match load(docs, home)? {
        None => InventoryState::Absent {
            home: home_text,
            path: path_text,
            id,
        },
        Some(l) => InventoryState::Read {
            home: home_text,
            path: path_text,
            id,
            pieces: pieces_of(&l.inventory, source, home),
            last_colour: last_colour(&l.inventory).into(),
        },
    })
}

/// The inventory in `home` as the page reads it, in every snapshot: where it is, the pieces it adds up to (with whether
/// their tile changed or is gone), the colour mode last used, and what is wrong with it if it does not read.
#[must_use]
pub fn snapshot_in(docs: &Docs, source: &dyn Source, home: Option<&Path>) -> InventoryState {
    let Some(home) = home else {
        return InventoryState::Unavailable;
    };
    match state_of(docs, source, home) {
        Ok(state) => state,
        Err(errs) => {
            let path = path_in(home);
            InventoryState::Broken {
                home: home.to_string_lossy().into_owned(),
                id: tile_id(&path.to_string_lossy()),
                path: path.to_string_lossy().into_owned(),
                problems: errs.iter().map(|d| d.message.clone()).collect(),
            }
        }
    }
}

/// The inventory as a host's snapshot holds it ([`snapshot_in`] over the host's home, files and tiles).
#[must_use]
pub fn snapshot(host: &impl SceneHost) -> InventoryState {
    snapshot_in(
        host.docs(),
        host.tile_source(),
        host.tiles_home().as_deref(),
    )
}

/// What the user owns, for a host's [`SceneHost::inventory`]: none where there is no home or no file yet.
///
/// # Errors
/// The inventory is there and cannot be read, or has mistakes (each at its text).
pub fn read_inventory(host: &impl SceneHost) -> Result<Option<Inventory>, Vec<Diag>> {
    match host.tiles_home() {
        Some(h) => Ok(load(host.docs(), &h)?.map(|l| l.inventory)),
        None => Ok(None),
    }
}

// ---- the document ---------------------------------------------------------------------------

/// Open or rebuild an inventory document for the page: its pieces and its text, or its mistakes at their text.
pub fn rebuild(host: &impl InventoryHost, id: &str, path: &str) {
    let files = BTreeMap::from([(path.to_owned(), host.modified(path))]);
    let writable = crate::tile_host::writable(host.storage(), path);
    let skeleton = move |inventory: InventoryRead| {
        TilePlan::Inventory(InventoryDoc {
            writable,
            inventory,
        })
    };
    let fail_with = |errs: Vec<Diag>, text: &str| {
        host.report(id, Scope::Check, |name| {
            crate::tile_calls::of_errors(&errs, Scope::Check, id, name, &[])
        });
        let (files, text) = (files.clone(), text.to_owned());
        host.update(
            id,
            Patch::InventoryFailed {
                errors: dt_eval::mistakes(&errs),
                files,
                plan: skeleton(InventoryRead {
                    text,
                    pieces: Vec::new(),
                    problems: dt_eval::mistakes(&errs),
                }),
            },
        );
    };
    let text = match host.docs().read(path) {
        Ok((t, _)) => t,
        Err(e) => {
            return fail_with(plain(format!("cannot read {path}: {e}")), "");
        }
    };
    let inv = match doc::decode(path, &text) {
        Ok(i) => i,
        Err(e) => return fail_with(e.iter().map(Diag::from).collect(), &text),
    };
    let pieces = pieces_of(
        &inv,
        host.tile_source(),
        host.tiles_home()
            .as_deref()
            .unwrap_or_else(|| Path::new(".")),
    );
    let held = inv.pieces().iter().filter(|p| p.have > 0).count();
    host.report(id, Scope::Check, |name| {
        vec![Message::new(
            Kind::Check,
            Severity::Info,
            Scope::Check,
            id,
            name,
            format!(
                "Read {name}: {} line{}, {held} piece{} on the shelf",
                inv.entries.len(),
                if inv.entries.len() == 1 { "" } else { "s" },
                if held == 1 { "" } else { "s" }
            ),
        )]
    });
    host.update(
        id,
        Patch::InventoryReady {
            files,
            plan: skeleton(InventoryRead {
                text,
                pieces,
                problems: Vec::new(),
            }),
        },
    );
}

// ---- the calls ------------------------------------------------------------------------------

fn bad(msg: impl Into<String>) -> Refusal {
    Refusal::refused(Status::BadRequest, msg)
}

fn home_of(host: &impl SceneHost) -> Result<PathBuf, Refusal> {
    host.tiles_home().ok_or_else(|| {
        Refusal::failure(
            Status::Internal,
            "there is no home folder for the inventory ($DT_HOME, else ~/dharmatiles)",
        )
    })
}

fn date_of(host: &impl InventoryHost, on: Option<&str>) -> Result<String, Refusal> {
    match on {
        None => Ok(host.today()),
        Some(d) => {
            let probe = format!(
                "dharmatiles version=1\ninventory {{\n gone \"x\" seed=1 system=db colour=single on={}\n}}\n",
                doc::quote(d)
            );
            doc::decode("on", &probe)
                .map(|_| d.to_owned())
                .map_err(|_| bad("the date is like 2026-11-09"))
        }
    }
}

fn system_of(host: &impl InventoryHost, system: Option<dt_lang::System>) -> dt_lang::System {
    system.unwrap_or_else(|| host.options().system)
}

/// Tell every listener the inventory (and so the list) changed.
fn announce(host: &impl InventoryHost) {
    // the file may have been made now: it joins My tiles' list, and the snapshot has the new log
    host.rescan();
    host.told();
}

/// Add `lines` to the log, tell the page, and say what happened in the stream.  `about` is the tile the message is filed
/// under.
fn write(
    host: &impl InventoryHost,
    about: &str,
    lines: &[String],
    said: &str,
) -> Result<InventoryWrote, Refusal> {
    let home = home_of(host)?;
    match append(host.docs(), &host.version(), &home, lines) {
        Ok(path) => {
            announce(host);
            let to = host.summary(about).map_or_else(
                || {
                    (
                        path_in(&home).to_string_lossy().into_owned(),
                        doc::FILE.to_owned(),
                    )
                },
                |s| (about.to_owned(), s.name),
            );
            let file = path.display().to_string();
            host.say(
                Message::new(
                    Kind::Export,
                    Severity::Info,
                    Scope::Export,
                    &to.0,
                    &to.1,
                    said,
                )
                .data(MessageData::Inventory(InventoryWrote {
                    lines: lines.to_vec(),
                    file: file.clone(),
                })),
            );
            Ok(InventoryWrote {
                lines: lines.to_vec(),
                file,
            })
        }
        Err(errs) => {
            host.say(
                Message::new(
                    Kind::Error,
                    Severity::Error,
                    Scope::Export,
                    about,
                    "inventory",
                    format!(
                        "Could not add to the inventory: {}",
                        errs.first().map_or("", |d| d.message.as_str())
                    ),
                )
                .code(dt_lang::codes::INVENTORY),
            );
            Err(Refusal::of_mistakes(&errs))
        }
    }
}

fn plural(n: usize, one: &str) -> String {
    format!("{n} {one}{}", if n == 1 { "" } else { "s" })
}

/// `inventoryHave`: one good print (or `count`) of the tile, at the tile's own seed unless one is given.  Works without any
/// export.
///
/// # Errors
/// No such tile; the tile does not check; the inventory does not read or cannot be written.
pub fn have(host: &impl InventoryHost, v: InventoryHaveReq) -> Result<InventoryWrote, Refusal> {
    let id = v.tile.as_str();
    let Some(path) = host
        .path_of(id)
        .filter(|p| !is_inventory(p) && !p.ends_with(".dmap") && !p.ends_with(".stl"))
    else {
        return Err(Refusal::refused(Status::NotFound, "no such tile"));
    };
    let system = system_of(host, v.system);
    let colour: Colour = v.colour.into();
    let on = date_of(host, v.on.as_deref())?;
    let count = usize::try_from(v.count.unwrap_or(1).clamp(1, 100))
        .expect("a count of at most 100 fits a usize");
    let home = host.tiles_home();
    let named = name_tile(
        host.tile_source(),
        &host.options(),
        &path,
        None,
        home.as_deref(),
        system,
        v.seed,
    )
    .map_err(|e| Refusal::of_mistakes(&e))?;
    let ident = Identity {
        tile: named.canon.clone(),
        seed: named.seed,
        system,
        colour,
    };
    let line = doc::print_line(&ident, true, &on, Some(&named.hash), None, None);
    let lines = vec![line; count];
    let name = host.summary(id).map(|s| s.name).unwrap_or_default();
    write(
        host,
        id,
        &lines,
        &format!(
            "Added {} {name} (seed {}, {}, {}) to your inventory",
            if count == 1 {
                "one".to_owned()
            } else {
                count.to_string()
            },
            named.seed,
            system.title(),
            if colour == Colour::Multi {
                "multicolour"
            } else {
                "single colour"
            }
        ),
    )
}

/// `inventoryRecord`: the sheet after a print.  Every piece on the plate (as the plates' panel showed it) is added as `ok`
/// except the failed ones, by their place in the plate's list, which are added as `failed`.  The tile, seed, system and
/// colour mode are the plan's, the date is the page's, the hash is the tile's document now, and `from` names the list and
/// plate.
///
/// # Errors
/// No such list, the list does not plan, no such plate, a failed piece not on the plate; or the log cannot be added to.
pub fn record(host: &impl InventoryHost, v: InventoryRecordReq) -> Result<InventoryWrote, Refusal> {
    let id = v.list.as_str();
    let Some(path) = host.path_of(id).filter(|p| is_plates(p)) else {
        return Err(Refusal::refused(Status::NotFound, "no such print list"));
    };
    let unplanned = || bad("this list does not plan yet, so there are no plates to record");
    let Some(TilePlan::Plates(doc)) = host.plan_of(id) else {
        return Err(unplanned());
    };
    let PlatesPanel::Planned(p) = doc.plates else {
        return Err(unplanned());
    };
    if v.plate < 1 {
        return Err(bad("give the plate (from 1)"));
    }
    let plate = v.plate;
    let Some(pieces) = p.layout.get(plate - 1).map(|pl| &pl.pieces) else {
        return Err(bad("the list has no such plate"));
    };
    let failed: Vec<usize> = v.failed.unwrap_or_default();
    if failed.iter().any(|n| *n >= pieces.len()) {
        return Err(bad("a failed piece is not on the plate"));
    }
    let on = date_of(host, v.on.as_deref())?;
    let colour = if p.settings.mode == Mode::Multicolour.word() {
        Colour::Multi
    } else {
        Colour::Single
    };
    let title = Path::new(&path)
        .file_stem()
        .map_or_else(|| "plates".to_owned(), |s| s.to_string_lossy().into_owned());
    let from = format!("{title}/plate-{plate}");
    let home = host.tiles_home();
    let mut lines = Vec::new();
    for (i, piece) in pieces.iter().enumerate() {
        let Some(k) = p.kinds.get(piece.kind) else {
            return Err(bad("a piece names no tile"));
        };
        let tile = k.planned.canon.as_str();
        let ident = Identity {
            tile: tile.to_owned(),
            seed: k.planned.seed_used,
            system: k.planned.system,
            colour,
        };
        let hash = tile_hash(host.tile_source(), tile, home.as_deref());
        lines.push(doc::print_line(
            &ident,
            !failed.contains(&i),
            &on,
            hash.as_deref(),
            Some(&from),
            None,
        ));
    }
    let bad_n = failed.len();
    let good = lines.len() - bad_n;
    let said = if bad_n == 0 {
        format!(
            "Added {} from plate {plate} to your inventory",
            plural(good, "piece")
        )
    } else {
        format!(
            "Added {} from plate {plate} to your inventory, and {} as failed (kept, not counted)",
            plural(good, "piece"),
            bad_n
        )
    };
    write(host, id, &lines, &said)
}

/// `inventoryGone`: one of the piece is lost or broken.
///
/// # Errors
/// None of that piece is on the shelf; the inventory does not read or cannot be written.
pub fn gone(host: &impl InventoryHost, v: InventoryGoneReq) -> Result<InventoryWrote, Refusal> {
    let on = date_of(host, v.on.as_deref())?;
    let home = home_of(host)?;
    let ident = Identity {
        tile: v.tile.clone(),
        seed: v.seed,
        system: v.system,
        colour: v.colour.into(),
    };
    match load(host.docs(), &home) {
        Ok(Some(l)) if l.inventory.have(&ident) > 0 => {}
        Ok(_) => {
            return Err(Refusal::refused(
                Status::Conflict,
                "you have none of that piece on the shelf to take away",
            ));
        }
        Err(e) => return Err(Refusal::of_mistakes(&e)),
    }
    let line = doc::gone_line(
        &ident,
        &on,
        v.note.as_deref().filter(|n| !n.trim().is_empty()),
    );
    let about = tile_id(&path_in(&home).to_string_lossy());
    write(
        host,
        &about,
        &[line],
        &format!(
            "Marked one {} gone",
            v.tile.rsplit('/').next().unwrap_or(&v.tile)
        ),
    )
}

/// `inventoryRelink`: a tile has another path now.  `to` is the id of a listed tile.
///
/// # Errors
/// No such tile; the inventory has nothing for the old path; the inventory does not read or cannot be written.
pub fn relink(host: &impl InventoryHost, v: InventoryRelinkReq) -> Result<InventoryWrote, Refusal> {
    let from = v.from.as_str();
    let on = date_of(host, v.on.as_deref())?;
    let Some(path) = host.path_of(&v.to) else {
        return Err(Refusal::refused(Status::NotFound, "no such tile"));
    };
    let to = names::canonical(&path, host.tiles_home().as_deref());
    let home = home_of(host)?;
    match load(host.docs(), &home) {
        Ok(Some(l)) if l.inventory.pieces().iter().any(|p| p.id.tile == from) => {}
        Ok(_) => {
            return Err(Refusal::refused(
                Status::Conflict,
                "the inventory has nothing for that path",
            ));
        }
        Err(e) => return Err(Refusal::of_mistakes(&e)),
    }
    let about = tile_id(&path_in(&home).to_string_lossy());
    write(
        host,
        &about,
        &[doc::renamed_line(from, &to, &on)],
        &format!("Re-linked {from} to {to}"),
    )
}

/// Note a rename or a move of a tile: if the log knows the old path, a `renamed` line makes the counts follow it.  Said
/// loudly where it fails: the file is already moved, and the log would otherwise lose the tile.
pub fn note_rename(host: &impl InventoryHost, from: &Path, to: &Path) {
    let Some(home) = host.tiles_home() else {
        return;
    };
    let old = names::canonical(&from.to_string_lossy(), Some(&home));
    let new = names::canonical(&to.to_string_lossy(), Some(&home));
    if old == new {
        return;
    }
    let known = match load(host.docs(), &home) {
        Ok(None) => return,
        Ok(Some(l)) => l.inventory.pieces().iter().any(|p| p.id.tile == old),
        Err(e) => {
            say_problem(
                host,
                &format!(
                    "Could not note that {old} is now {new}: the inventory does not read ({})",
                    e.first().map_or("", |d| d.message.as_str())
                ),
            );
            return;
        }
    };
    if !known {
        return;
    }
    match append(
        host.docs(),
        &host.version(),
        &home,
        &[doc::renamed_line(&old, &new, &host.today())],
    ) {
        Ok(_) => announce(host),
        Err(e) => say_problem(
            host,
            &format!(
                "Could not note that {old} is now {new} in the inventory: {}",
                e.first().map_or("", |d| d.message.as_str())
            ),
        ),
    }
}

/// Every tile under `dir` that the log knows, as (old, new) canonical paths for a folder renamed from `dir` to `to`.
pub fn note_folder_rename(host: &impl InventoryHost, dir: &Path, to: &Path) {
    let Some(home) = host.tiles_home() else {
        return;
    };
    let prefix_old = names::canonical(&dir.to_string_lossy(), Some(&home));
    let prefix_new = names::canonical(&to.to_string_lossy(), Some(&home));
    let inv = match load(host.docs(), &home) {
        Ok(None) => return,
        Ok(Some(l)) => l.inventory,
        Err(e) => {
            say_problem(
                host,
                &format!(
                    "Could not follow the folder {prefix_old} to {prefix_new} in the inventory: it does not read ({})",
                    e.first().map_or("", |d| d.message.as_str())
                ),
            );
            return;
        }
    };
    let under = format!("{prefix_old}/");
    let mut tiles: Vec<String> = inv
        .pieces()
        .iter()
        .filter(|p| p.id.tile.starts_with(&under))
        .map(|p| p.id.tile.clone())
        .collect();
    tiles.sort();
    tiles.dedup();
    if tiles.is_empty() {
        return;
    }
    let today = host.today();
    let lines: Vec<String> = tiles
        .iter()
        .map(|t| doc::renamed_line(t, &format!("{prefix_new}/{}", &t[under.len()..]), &today))
        .collect();
    match append(host.docs(), &host.version(), &home, &lines) {
        Ok(_) => announce(host),
        Err(e) => say_problem(
            host,
            &format!(
                "Could not follow the folder {prefix_old} to {prefix_new} in the inventory: {}",
                e.first().map_or("", |d| d.message.as_str())
            ),
        ),
    }
}

fn say_problem(host: &impl InventoryHost, text: &str) {
    host.say(
        Message::new(
            Kind::Server,
            Severity::Error,
            Scope::Server,
            "",
            "inventory",
            text,
        )
        .code(dt_lang::codes::INVENTORY)
        .help(Some(
            "mend inventory.dinv, then `dharmatiles inventory relink <old> <new>`",
        )),
    );
}

/// `inventoryReprint`: the piece, with its recorded seed, goes into the print list `list` (a listed `.dmap`) or into a new
/// one beside your tiles.  A piece you still have is added with `print=always`; one you do not is a plain piece, which
/// prints because the inventory lacks it.
///
/// # Errors
/// The inventory does not read; the list is not there, does not read or cannot be written.
#[expect(
    clippy::disallowed_methods,
    reason = "permitted silence: a best-effort probe for a warning only (a list that cannot be read has no mode to compare; the put below reads it and refuses in words)"
)]
pub fn reprint(
    host: &impl InventoryHost,
    v: InventoryReprintReq,
) -> Result<InventoryReprinted, Refusal> {
    let (tile, seed, system) = (v.tile.as_str(), v.seed, v.system);
    let colour: Colour = v.colour.into();
    let home = home_of(host)?;
    let have = match load(host.docs(), &home) {
        Ok(Some(l)) => l.inventory.have(&Identity {
            tile: tile.to_owned(),
            seed,
            system,
            colour,
        }),
        Ok(None) => 0,
        Err(e) => return Err(Refusal::of_mistakes(&e)),
    };
    let mode = match colour {
        Colour::Single => Mode::Single,
        Colour::Multi => Mode::Multicolour,
    };
    // a list laid out in another colour mode is warned about, not refused
    let list = v.list.as_deref();
    let warning = list
        .and_then(|id| host.path_of(id))
        .and_then(|path| host.storage().read_text(Path::new(&path)).ok().map(|t| (path, t)))
        .and_then(|(path, t)| dt_doc::map::decode(&path, &t).ok())
        .and_then(|l| l.plates.mode.as_ref().filter(|m| m.value != mode).map(|m| {
            format!(
                "this list prints in {}, and the piece you have is {}: the inventory counts the colour mode, so it will not see it as the one you have",
                m.value.word(),
                if colour == Colour::Multi { "multicolour" } else { "single colour" }
            )
        }));
    let want = dt_doc::map::Want {
        tile,
        seed: Some(seed),
        system: Some(system),
        always: have > 0,
    };
    let put = crate::list_calls::put(host, &want, list, v.doc_rev, Some(mode))?;
    announce(host);
    let id = tile_id(&put.path.to_string_lossy());
    if put.made {
        host.want(&id)?;
    }
    Ok(InventoryReprinted {
        list: put.path.display().to_string(),
        id,
        made: put.made,
        warning,
        always: have > 0,
        before: put.before,
        text: put.after,
        doc_rev: put.rev,
        note: put.note.map(str::to_owned),
    })
}
