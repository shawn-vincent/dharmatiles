//! The inventory in the editor: `inventory.dinv` in My tiles, as the page
//! uses it.
//!
//! - The **snapshot** carries the inventory (what each piece adds up to), so
//!   the folder rows' pills, the tile level's "N printed" line and the plates'
//!   "N in inventory" all come from one place and change together.
//! - Opening the **document** shows the pieces grouped by tile, with its text
//!   for Source; a file with mistakes is a failed build whose errors are at
//!   their text.
//! - The **calls** append lines to the log: "I have this", the sheet after a
//!   print, `gone`, a re-link, and a reprint into a print list.  Nothing here
//!   edits a line away; every call is one or more lines added at the end, and
//!   a file that is not understood is never added to.
//!
//! The page's side is `js/editor/inventory.js` (the view) and the pieces in
//! `plates.js`, `view/places-panel.js` and `levels.js`.  Docs: docs/inventory.html.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use dt_doc::inventory::{self as doc, Colour, Identity};
use dt_eval::Diag;
use dt_print::inventory as disk;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::api::{Out, Reply};
use super::messages::Scope;
use super::{Shared, State, fail, lock, messages};
use crate::errors_json;
use crate::message::{Kind, Message, Severity};

// ---- the calls' types ------------------------------------------------------------------

/// The colour mode a print was made in.
#[derive(Deserialize, Clone, Copy)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(optional_fields = nullable))]
#[serde(rename_all = "lowercase")]
pub enum ColourWord {
    Single,
    Multi,
}

impl From<ColourWord> for Colour {
    fn from(c: ColourWord) -> Self {
        match c {
            ColourWord::Single => Self::Single,
            ColourWord::Multi => Self::Multi,
        }
    }
}

/// The base system a tile was built for.
#[derive(Deserialize, Clone, Copy)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(optional_fields = nullable))]
#[serde(rename_all = "lowercase")]
pub enum SystemWord {
    Db,
    Ol,
}

impl SystemWord {
    fn word(self) -> &'static str {
        match self {
            Self::Db => "db",
            Self::Ol => "ol",
        }
    }
}

#[derive(Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(optional_fields = nullable))]
#[serde(deny_unknown_fields)]
pub struct InventoryHaveReq {
    pub tile: String,
    pub colour: ColourWord,
    pub system: Option<SystemWord>,
    pub seed: Option<u64>,
    pub count: Option<u64>,
    /// The date, like 2026-11-09 (today when left out).
    pub on: Option<String>,
}

#[derive(Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(optional_fields = nullable))]
#[serde(deny_unknown_fields)]
pub struct InventoryRecordReq {
    pub list: String,
    /// The plate, from 1.
    pub plate: usize,
    /// The pieces that failed, by their place in the plate's list.
    pub failed: Option<Vec<usize>>,
    pub on: Option<String>,
}

#[derive(Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(optional_fields = nullable))]
#[serde(deny_unknown_fields)]
pub struct InventoryGoneReq {
    pub tile: String,
    pub seed: u64,
    pub system: SystemWord,
    pub colour: ColourWord,
    pub note: Option<String>,
    pub on: Option<String>,
}

#[derive(Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(optional_fields = nullable))]
#[serde(deny_unknown_fields)]
pub struct InventoryRelinkReq {
    /// The old path, as the log has it.
    pub from: String,
    /// The id of the listed tile it is now.
    pub to: String,
    pub on: Option<String>,
}

#[derive(Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(optional_fields = nullable))]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct InventoryReprintReq {
    pub tile: String,
    pub seed: u64,
    pub system: SystemWord,
    pub colour: ColourWord,
    /// The print list's id; none makes a new list.
    pub list: Option<String>,
    /// The revision of the list the caller last saw, when it has the list open.
    pub doc_rev: Option<u64>,
}

/// Lines added to the inventory.
#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct InventoryWrote {
    pub lines: Vec<String>,
    pub file: String,
}

/// A piece put in a print list again.
#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
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

/// Is this document an inventory?
pub(super) fn is_inventory(path: &str) -> bool {
    path.to_ascii_lowercase().ends_with(".dinv")
}

/// The inventory as the page reads it, in every snapshot: where it is, the
/// pieces it adds up to (with whether their tile changed or is gone), the
/// colour mode last used, and what is wrong with it if it does not read.
pub(super) fn snapshot(home: Option<&Path>) -> Value {
    let Some(home) = home else {
        return json!({"available": false, "pieces": []});
    };
    let path = disk::path_in(home);
    let base = json!({
        "available": true, "home": home.to_string_lossy(),
        "path": path.to_string_lossy(), "id": super::tile_id(&path.to_string_lossy()),
    });
    let mut out = base;
    match disk::load(home) {
        Ok(None) => {
            out["exists"] = json!(false);
            out["pieces"] = json!([]);
        }
        Ok(Some(l)) => {
            out["exists"] = json!(true);
            out["pieces"] = Value::Array(crate::inventory::pieces_json(&l.inventory, home));
            out["last_colour"] = json!(last_colour(&l.inventory).word());
        }
        Err(e) => {
            out["exists"] = json!(true);
            out["pieces"] = json!([]);
            out["problems"] = Value::Array(errors_json(&e));
        }
    }
    out
}

fn last_colour(inv: &doc::Inventory) -> Colour {
    inv.entries
        .iter()
        .rev()
        .find_map(|e| match e {
            doc::Entry::Print { id, .. } | doc::Entry::Gone { id, .. } => Some(id.colour),
            doc::Entry::Renamed { .. } => None,
        })
        .unwrap_or(Colour::Single)
}

// ---- the document ---------------------------------------------------------------------------

/// Open or rebuild an inventory document for the page.
pub(super) fn rebuild(shared: &Arc<Shared>, id: &str, path: &str) {
    let files = BTreeMap::from([(path.to_owned(), crate::watch::mtime(path))]);
    let writable = std::fs::metadata(path).is_ok_and(|m| !m.permissions().readonly());
    let skeleton = |inv: Value| {
        json!({
            "kind": "inventory", "tile": "inventory", "cols": 1, "rows": 1, "square_mm": 35.0,
            "size_mm": [0, 0], "datum_z": 0.0, "stack": [], "writable": writable,
            "inventory": inv,
        })
    };
    let fail_with = |errs: Vec<Diag>, text: &str| {
        shared.report(id, Scope::Check, |name| {
            messages::of_errors(&errs, Scope::Check, id, name, &[])
        });
        shared.update(id, |t| {
            t.status = State::Failed;
            t.progress = None;
            t.shown = None;
            t.errors = super::api::mistakes(&errs);
            t.files = files.clone();
            t.mesh = None;
            t.plan = Some(skeleton(
                json!({"text": text, "pieces": [], "problems": errors_json(&errs)}),
            ));
        });
    };
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            let d = Diag::plain(
                dt_lang::codes::INVENTORY,
                format!("cannot read {path}: {e}"),
            );
            return fail_with(vec![d], "");
        }
    };
    let inv = match doc::decode(path, &text) {
        Ok(i) => i,
        Err(e) => return fail_with(e.iter().map(Diag::from).collect(), &text),
    };
    let pieces = crate::inventory::pieces_json(
        &inv,
        shared.home.as_deref().unwrap_or_else(|| Path::new(".")),
    );
    let held = inv.pieces().iter().filter(|p| p.have > 0).count();
    shared.report(id, Scope::Check, |name| {
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
    shared.update(id, |t| {
        t.status = State::Ready;
        t.progress = None;
        t.shown = None;
        t.errors.clear();
        t.failure = None;
        t.files = files.clone();
        t.mesh = None;
        t.plan = Some(skeleton(json!({"text": text, "pieces": pieces})));
    });
}

// ---- the calls ------------------------------------------------------------------------------

fn bad(msg: &str) -> Reply {
    fail::refused("400 Bad Request", msg)
}

fn home_of(shared: &Shared) -> Result<&Path, Reply> {
    shared.home.as_deref().ok_or_else(|| {
        fail::failure(
            "500 Internal Server Error",
            "there is no home folder for the inventory ($DT_HOME, else ~/dharmatiles)",
        )
    })
}

fn date_of(on: Option<&str>) -> Result<String, Reply> {
    match on {
        None => Ok(disk::today()),
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

fn system_of(shared: &Shared, system: Option<SystemWord>) -> String {
    system.map_or_else(|| shared.options().system, |s| s.word().to_owned())
}

/// Tell every listener the inventory (and so the list) changed.
fn announce(shared: &Shared) {
    // the file may have been made now: it joins My tiles' list, and the snapshot has the new log
    shared.rescan();
    super::places::announce(shared);
}

/// Add `lines` to the log, tell the page, and say what happened in the
/// stream.  `about` is the tile the message is filed under.
fn write(shared: &Shared, about: &str, lines: &[String], said: &str) -> Out<InventoryWrote> {
    let home = home_of(shared)?;
    match disk::append(home, lines) {
        Ok(path) => {
            announce(shared);
            let to = shared.name_of(about).map_or_else(
                || {
                    (
                        disk::path_in(home).to_string_lossy().into_owned(),
                        doc::FILE.to_owned(),
                    )
                },
                |n| (about.to_owned(), n),
            );
            shared.say(
                Message::new(
                    Kind::Export,
                    Severity::Info,
                    Scope::Export,
                    &to.0,
                    &to.1,
                    said,
                )
                .data(json!({"lines": lines, "file": path.display().to_string()})),
            );
            Ok(InventoryWrote {
                lines: lines.to_vec(),
                file: path.display().to_string(),
            })
        }
        Err(errs) => {
            shared.say(
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
            Err(fail::mistakes(&errs))
        }
    }
}

fn slot_path(shared: &Shared, id: &str) -> Option<String> {
    lock(&shared.tiles)
        .iter()
        .find(|t| t.id == id)
        .map(|t| t.path.clone())
}

fn plural(n: usize, one: &str) -> String {
    format!("{n} {one}{}", if n == 1 { "" } else { "s" })
}

/// `/api/inventory/have` `{tile, colour, system?, seed?, count?, on?}`: one
/// good print (or `count`) of the tile, at the tile's own seed unless one is
/// given.  Works without any export.
pub(super) fn have(shared: &Shared, v: InventoryHaveReq) -> Out<InventoryWrote> {
    let id = v.tile.as_str();
    let Some(path) = slot_path(shared, id)
        .filter(|p| !is_inventory(p) && !p.ends_with(".dmap") && !p.ends_with(".stl"))
    else {
        return Err(fail::refused("404 Not Found", "no such tile"));
    };
    let system = system_of(shared, v.system);
    let colour: Colour = v.colour.into();
    let on = date_of(v.on.as_deref())?;
    let seed = v.seed;
    let count = v.count.unwrap_or(1).clamp(1, 100) as usize;
    let named = disk::name_tile(&path, None, shared.home.as_deref(), &system, seed)
        .map_err(|e| fail::mistakes(&e))?;
    let ident = Identity {
        tile: named.canon.clone(),
        seed: named.seed,
        system: system.clone(),
        colour,
    };
    let line = doc::print_line(&ident, true, &on, Some(&named.hash), None, None);
    let lines = vec![line; count];
    let name = shared.name_of(id).unwrap_or_default();
    write(
        shared,
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
            if system == "db" {
                "DungeonBlocks"
            } else {
                "OpenLOCK"
            },
            if colour == Colour::Multi {
                "multicolour"
            } else {
                "single colour"
            }
        ),
    )
}

/// `/api/inventory/record` `{list, plate, failed: [n…], on?}`: the sheet after
/// a print.  Every piece on the plate (as the plates' panel showed it) is added
/// as `ok` except the failed ones, by their place in the plate's list, which
/// are added as `failed`.  The tile, seed, system and colour mode are the
/// plan's, the date is the page's, the hash is the tile's document now, and
/// `from` names the list and plate.
pub(super) fn record(shared: &Shared, v: InventoryRecordReq) -> Out<InventoryWrote> {
    let id = v.list.as_str();
    let (path, plan) = {
        let tiles = lock(&shared.tiles);
        let Some(t) = tiles
            .iter()
            .find(|t| t.id == id && super::plates::is_plates(&t.path))
        else {
            return Err(fail::refused("404 Not Found", "no such print list"));
        };
        (t.path.clone(), t.plan.clone())
    };
    let Some(p) = plan
        .as_ref()
        .map(|p| &p["plates"])
        .filter(|p| p["planned"] == json!(true))
    else {
        return Err(bad(
            "this list does not plan yet, so there are no plates to record",
        ));
    };
    if v.plate < 1 {
        return Err(bad("give the plate (from 1)"));
    }
    let plate = v.plate;
    let Some(pieces) = p["layout"]
        .get(plate - 1)
        .and_then(|pl| pl["pieces"].as_array())
    else {
        return Err(bad("the list has no such plate"));
    };
    let failed: Vec<usize> = v.failed.unwrap_or_default();
    if failed.iter().any(|n| *n >= pieces.len()) {
        return Err(bad("a failed piece is not on the plate"));
    }
    let on = date_of(v.on.as_deref())?;
    let colour = if p["mode"] == json!("multicolour") {
        Colour::Multi
    } else {
        Colour::Single
    };
    let from = format!(
        "{}/plate-{plate}",
        crate::plates::title_of(Path::new(&path))
    );
    let mut lines = Vec::new();
    for (i, piece) in pieces.iter().enumerate() {
        let Some(k) = piece["kind"]
            .as_u64()
            .and_then(|k| p["kinds"].get(usize::try_from(k).unwrap_or(usize::MAX)))
        else {
            return Err(bad("a piece names no tile"));
        };
        let (Some(tile), Some(seed), Some(system)) = (
            k["canon"].as_str(),
            k["seed_used"].as_u64(),
            k["system"].as_str(),
        ) else {
            return Err(bad("the plan is missing a piece's tile, seed or system"));
        };
        let ident = Identity {
            tile: tile.to_owned(),
            seed,
            system: system.to_owned(),
            colour,
        };
        let hash = disk::tile_hash(tile, shared.home.as_deref());
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
    write(shared, id, &lines, &said)
}

/// `/api/inventory/gone` `{tile, seed, system, colour, note?, on?}`: one of
/// the piece is lost or broken.
pub(super) fn gone(shared: &Shared, v: InventoryGoneReq) -> Out<InventoryWrote> {
    let on = date_of(v.on.as_deref())?;
    let home = home_of(shared)?;
    let ident = Identity {
        tile: v.tile.clone(),
        seed: v.seed,
        system: v.system.word().to_owned(),
        colour: v.colour.into(),
    };
    match disk::load(home) {
        Ok(Some(l)) if l.inventory.have(&ident) > 0 => {}
        Ok(_) => {
            return Err(fail::refused(
                "409 Conflict",
                "you have none of that piece on the shelf to take away",
            ));
        }
        Err(e) => return Err(fail::mistakes(&e)),
    }
    let line = doc::gone_line(
        &ident,
        &on,
        v.note.as_deref().filter(|n| !n.trim().is_empty()),
    );
    let about = super::tile_id(&disk::path_in(home).to_string_lossy());
    write(
        shared,
        &about,
        &[line],
        &format!(
            "Marked one {} gone",
            v.tile.rsplit('/').next().unwrap_or(&v.tile)
        ),
    )
}

/// `/api/inventory/relink` `{from, to, on?}`: a tile has another path now.
/// `to` is the id of a listed tile.
pub(super) fn relink(shared: &Shared, v: InventoryRelinkReq) -> Out<InventoryWrote> {
    let from = v.from.as_str();
    let on = date_of(v.on.as_deref())?;
    let Some(path) = slot_path(shared, &v.to) else {
        return Err(fail::refused("404 Not Found", "no such tile"));
    };
    let to = disk::canonical(&path, shared.home.as_deref());
    let home = home_of(shared)?;
    match disk::load(home) {
        Ok(Some(l)) if l.inventory.pieces().iter().any(|p| p.id.tile == from) => {}
        Ok(_) => {
            return Err(fail::refused(
                "409 Conflict",
                "the inventory has nothing for that path",
            ));
        }
        Err(e) => return Err(fail::mistakes(&e)),
    }
    let about = super::tile_id(&disk::path_in(home).to_string_lossy());
    write(
        shared,
        &about,
        &[doc::renamed_line(from, &to, &on)],
        &format!("Re-linked {from} to {to}"),
    )
}

/// Note a rename or a move of a tile: if the log knows the old path, a
/// `renamed` line makes the counts follow it.  Said loudly where it fails:
/// the file is already moved, and the log would otherwise lose the tile.
pub(super) fn note_rename(shared: &Shared, from: &Path, to: &Path) {
    let Some(home) = shared.home.as_deref() else {
        return;
    };
    let old = disk::canonical(&from.to_string_lossy(), Some(home));
    let new = disk::canonical(&to.to_string_lossy(), Some(home));
    if old == new {
        return;
    }
    let known = match disk::load(home) {
        Ok(None) => return,
        Ok(Some(l)) => l.inventory.pieces().iter().any(|p| p.id.tile == old),
        Err(e) => {
            say_problem(
                shared,
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
    match disk::append(home, &[doc::renamed_line(&old, &new, &disk::today())]) {
        Ok(_) => announce(shared),
        Err(e) => say_problem(
            shared,
            &format!(
                "Could not note that {old} is now {new} in the inventory: {}",
                e.first().map_or("", |d| d.message.as_str())
            ),
        ),
    }
}

/// Every tile under `dir` that the log knows, as (old, new) canonical paths
/// for a folder renamed from `dir` to `to`.
pub(super) fn note_folder_rename(shared: &Shared, dir: &Path, to: &Path) {
    let Some(home) = shared.home.as_deref() else {
        return;
    };
    let prefix_old = disk::canonical(&dir.to_string_lossy(), Some(home));
    let prefix_new = disk::canonical(&to.to_string_lossy(), Some(home));
    let inv = match disk::load(home) {
        Ok(None) => return,
        Ok(Some(l)) => l.inventory,
        Err(e) => {
            say_problem(
                shared,
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
    let today = disk::today();
    let lines: Vec<String> = tiles
        .iter()
        .map(|t| doc::renamed_line(t, &format!("{prefix_new}/{}", &t[under.len()..]), &today))
        .collect();
    match disk::append(home, &lines) {
        Ok(_) => announce(shared),
        Err(e) => say_problem(
            shared,
            &format!(
                "Could not follow the folder {prefix_old} to {prefix_new} in the inventory: {}",
                e.first().map_or("", |d| d.message.as_str())
            ),
        ),
    }
}

fn say_problem(shared: &Shared, text: &str) {
    shared.say(
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

/// `/api/inventory/reprint` `{tile, seed, system, colour, list?, docRev?}`: the piece,
/// with its recorded seed, goes into the print list `list` (a listed `.dmap`)
/// or into a new one beside your tiles.  A piece you still have is added with
/// `print=always`; one you do not is a plain piece, which prints because the
/// inventory lacks it.
pub(super) fn reprint(shared: &Shared, v: InventoryReprintReq) -> Out<InventoryReprinted> {
    let (tile, seed, system) = (v.tile.as_str(), v.seed, v.system.word());
    let colour: Colour = v.colour.into();
    let home = home_of(shared)?;
    let have = match disk::load(home) {
        Ok(Some(l)) => l.inventory.have(&Identity {
            tile: tile.to_owned(),
            seed,
            system: system.to_owned(),
            colour,
        }),
        Ok(None) => 0,
        Err(e) => return Err(fail::mistakes(&e)),
    };
    let mode = match colour {
        Colour::Single => dt_doc::map::Mode::Single,
        Colour::Multi => dt_doc::map::Mode::Multicolour,
    };
    // a list laid out in another colour mode is warned about, not refused
    let list = v.list.as_deref();
    let warning = list
        .and_then(|id| slot_path(shared, id))
        .and_then(|path| std::fs::read_to_string(&path).ok().map(|t| (path, t)))
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
    let put = super::lists::put(shared, &want, list, v.doc_rev, Some(mode))?;
    shared.rescan();
    super::places::announce(shared);
    let id = super::tile_id(&put.path.to_string_lossy());
    if put.made {
        shared.want(&id);
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
