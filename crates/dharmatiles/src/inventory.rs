//! `dharmatiles inventory`: what you have printed.
//!
//! `list` adds the log up (by tile, system and colour mode, with the seeds you
//! have); `have` adds a good print of each tile named (or a failed one with
//! `--failed`); `gone` marks one lost; `relink` says a tile has another path
//! now.  The file is `inventory.dinv` in My tiles; nothing is made until the
//! first line is written.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use dt_doc::inventory::{self as doc, Colour, Identity};
use dt_eval::Diag;
use dt_print::inventory as disk;

use crate::cli::InventoryAction;
use crate::inventory_wire::{InventoryRow, InventoryState};
use crate::{ERROR, OK, report};

/// The command's choices, as the words the command line gives them.
pub(crate) struct Choice {
    pub(crate) system: Option<String>,
    pub(crate) colour: Option<String>,
    pub(crate) seed: Option<u64>,
    pub(crate) on: Option<String>,
    pub(crate) note: Option<String>,
    pub(crate) json: bool,
}

fn colour_of(word: Option<&str>) -> Result<Option<Colour>, String> {
    match word {
        None => Ok(None),
        Some(w) => Colour::from_word(w)
            .map(Some)
            .ok_or_else(|| format!("`{w}` is not a colour mode: use `single` or `multi`")),
    }
}

fn system_of(word: Option<&str>) -> Result<Option<dt_lang::System>, String> {
    match word {
        None | Some("all") => Ok(None),
        Some(s) => s
            .parse()
            .map(Some)
            .map_err(|e: String| format!("{e} (or `all`)")),
    }
}

fn date_of(on: Option<&str>) -> Result<String, String> {
    match on {
        None => Ok(disk::today()),
        Some(d) => {
            let probe = format!(
                "dharmatiles version=1\ninventory {{\n gone \"x\" seed=1 system=db colour=single on={}\n}}\n",
                doc::quote(d)
            );
            doc::decode("--on", &probe)
                .map(|_| d.to_owned())
                .map_err(|_| format!("`{d}` is not a date like 2026-11-09"))
        }
    }
}

/// The colour mode last used in the log (what `have` assumes), else single.
pub(crate) fn last_colour(inv: &doc::Inventory) -> Colour {
    inv.entries
        .iter()
        .rev()
        .find_map(|e| match e {
            doc::Entry::Print { id, .. } | doc::Entry::Gone { id, .. } => Some(id.colour),
            doc::Entry::Renamed { .. } => None,
        })
        .unwrap_or(Colour::Single)
}

fn home() -> Result<PathBuf, String> {
    crate::preview::roots::personal_home()
}

fn fail(errs: &[Diag], json: bool) -> u8 {
    if json {
        crate::say_json(&crate::cli_wire::Refused::of(dt_eval::mistakes(errs)));
    } else {
        report(errs);
    }
    ERROR
}

fn read(home: &Path) -> Result<(doc::Inventory, Option<PathBuf>), Vec<Diag>> {
    Ok(match disk::load(home)? {
        Some(l) => (l.inventory, Some(l.path)),
        None => (doc::Inventory::empty(), None),
    })
}

/// One line of the table `list` prints: a tile, system and colour mode, the pieces of its seeds added up.
struct Row {
    tile: String,
    system: dt_lang::System,
    colour: Colour,
    have: u32,
    seeds: Vec<(u64, u32)>,
    failed: u32,
    gone: u32,
    edited: bool,
    missing: bool,
}

/// The table's lines from the pieces (the same rows the page reads, grouped by tile, system and colour mode).
fn rows(pieces: &[InventoryRow]) -> Vec<Row> {
    let mut by: BTreeMap<(String, dt_lang::System, Colour), Row> = BTreeMap::new();
    for p in pieces {
        let colour = Colour::from(p.colour);
        let r = by
            .entry((p.tile.clone(), p.system, colour))
            .or_insert_with(|| Row {
                tile: p.tile.clone(),
                system: p.system,
                colour,
                have: 0,
                seeds: Vec::new(),
                failed: 0,
                gone: 0,
                edited: false,
                missing: p.missing,
            });
        r.have += p.have;
        r.failed += p.failed;
        r.gone += p.gone;
        if p.have > 0 {
            r.seeds.push((p.seed, p.have));
            r.edited |= p.edited;
        }
    }
    by.into_values().collect()
}

fn seeds_text(seeds: &[(u64, u32)]) -> String {
    seeds
        .iter()
        .map(|(s, n)| {
            if *n > 1 {
                format!("{s} x{n}")
            } else {
                s.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn list(home: &Path, c: &Choice, tile: Option<&str>, failures: bool) -> u8 {
    let system = match system_of(c.system.as_deref().or(Some(dt_lang::System::Db.word()))) {
        Ok(s) => s,
        Err(e) => {
            tell!("error: {e}");
            return ERROR;
        }
    };
    let colour = match colour_of(c.colour.as_deref()) {
        Ok(x) => x,
        Err(e) => {
            tell!("error: {e}");
            return ERROR;
        }
    };
    let state = match state_of(home) {
        Ok(x) => x,
        Err(e) => return fail(&e, c.json),
    };
    let all = state.pieces();
    let others = all
        .iter()
        .filter(|p| system.as_ref().is_some_and(|s| &p.system != s))
        .count();
    let shown: Vec<InventoryRow> = all
        .iter()
        .filter(|p| system.as_ref().is_none_or(|s| &p.system == s))
        .filter(|p| colour.is_none_or(|k| Colour::from(p.colour) == k))
        .filter(|p| tile.is_none_or(|t| p.tile.contains(t)))
        .filter(|p| !failures || p.failed > 0)
        .cloned()
        .collect();
    if c.json {
        // the page's own type, with the pieces the filters leave
        let out = match state {
            InventoryState::Read {
                home,
                path,
                id,
                last_colour,
                ..
            } => InventoryState::Read {
                home,
                path,
                id,
                pieces: shown,
                last_colour,
            },
            other => other,
        };
        match serde_json::to_string(&out) {
            Ok(text) => say!("{text}"),
            Err(e) => {
                tell!("error: the inventory did not write as JSON: {e}");
                return ERROR;
            }
        }
        return OK;
    }
    let table = rows(&shown);
    let path = match &state {
        InventoryState::Read { path, .. } => path,
        InventoryState::Absent { path, .. } => {
            tell!(
                "no inventory yet: nothing is made until the first line is written ({path} will be it); `dharmatiles inventory have <tile>` adds one"
            );
            return OK;
        }
        other => {
            tell!("error: the inventory cannot be listed: {other:?}");
            return ERROR;
        }
    };
    tell!(
        "{path} ({}): {}",
        system.map_or("every system", dt_lang::System::title),
        match table.len() {
            0 => "nothing matches".to_owned(),
            n => format!("{n} piece group{}", if n == 1 { "" } else { "s" }),
        }
    );
    for r in &table {
        let mut line = format!(
            "{:<44} {:<2} {:<6} have {}",
            r.tile,
            r.system,
            r.colour.word(),
            r.have
        );
        if !r.seeds.is_empty() {
            line.push_str(&format!(" (seeds {})", seeds_text(&r.seeds)));
        }
        if r.failed > 0 {
            line.push_str(&format!(", {} failed", r.failed));
        }
        if r.gone > 0 {
            line.push_str(&format!(", {} gone", r.gone));
        }
        if r.edited {
            line.push_str(", edited since printing");
        }
        if r.missing {
            line.push_str(", the tile is not at this path any more (dharmatiles inventory relink)");
        }
        tell!("{line}");
    }
    if others > 0 {
        tell!(
            "note: {others} piece group{} in the other system not shown: --system all lists every system",
            if others == 1 { "" } else { "s" }
        );
    }
    OK
}

/// A line for each tile named, for `have` and `gone`.
#[expect(
    clippy::disallowed_methods,
    reason = "permitted silence: an environment variable or directory that is not set or not readable is the meaning (the default applies)"
)]
fn write_lines(
    home: &Path,
    c: &Choice,
    tiles: &[String],
    make: &dyn Fn(&Identity, &disk::Named, &str) -> Vec<String>,
    verb: &str,
) -> u8 {
    let system = match system_of(c.system.as_deref().or(Some(dt_lang::System::Db.word()))) {
        Ok(Some(s)) => s,
        Ok(None) => {
            tell!("error: name one system, `db` or `ol`");
            return ERROR;
        }
        Err(e) => {
            tell!("error: {e}");
            return ERROR;
        }
    };
    let on = match date_of(c.on.as_deref()) {
        Ok(d) => d,
        Err(e) => {
            tell!("error: {e}");
            return ERROR;
        }
    };
    let colour = match colour_of(c.colour.as_deref()) {
        Ok(x) => x,
        Err(e) => {
            tell!("error: {e}");
            return ERROR;
        }
    };
    let (inv, _) = match read(home) {
        Ok(x) => x,
        Err(e) => return fail(&e, c.json),
    };
    let colour = colour.unwrap_or_else(|| last_colour(&inv));
    let cwd = std::env::current_dir().ok();
    let mut lines = Vec::new();
    for t in tiles {
        match disk::name_tile(t, cwd.as_deref(), Some(home), system, c.seed) {
            Ok(n) => {
                let id = Identity {
                    tile: n.canon.clone(),
                    seed: n.seed,
                    system,
                    colour,
                };
                if verb == "gone" && inv.have(&id) == 0 {
                    tell!(
                        "error: you have no {} seed {} for {} in {} to mark gone (`dharmatiles inventory list --tile {}` shows what you have)",
                        colour.word(),
                        n.seed,
                        n.canon,
                        system,
                        n.canon.rsplit('/').next().unwrap_or(&n.canon)
                    );
                    return ERROR;
                }
                lines.extend(make(&id, &n, &on));
            }
            Err(e) => return fail(&e, c.json),
        }
    }
    match disk::append(home, &lines) {
        Ok(p) => {
            if c.json {
                crate::say_json(&crate::cli_wire::InventoryAdded {
                    ok: true,
                    file: p.display().to_string(),
                    lines: lines.clone(),
                });
            } else {
                for l in &lines {
                    tell!("added: {l}");
                }
                tell!("{}", p.display());
            }
            OK
        }
        Err(e) => fail(&e, c.json),
    }
}

/// `dharmatiles inventory …`.
#[allow(clippy::too_many_lines)]
#[expect(
    clippy::disallowed_methods,
    reason = "permitted silence: an environment variable or directory that is not set or not readable is the meaning (the default applies)"
)]
pub(crate) fn run(action: Option<InventoryAction>, json: bool) -> u8 {
    let home = match home() {
        Ok(h) => h,
        Err(e) => {
            tell!("error: {e}");
            return ERROR;
        }
    };
    let action = action.unwrap_or(InventoryAction::List {
        system: None,
        colour: None,
        tile: None,
        failed: false,
    });
    let choice = |system: Option<String>, colour, seed, on, note| Choice {
        system,
        colour,
        seed,
        on,
        note,
        json,
    };
    match action {
        InventoryAction::List {
            system,
            colour,
            tile,
            failed,
        } => list(
            &home,
            &choice(system, colour, None, None, None),
            tile.as_deref(),
            failed,
        ),
        InventoryAction::Have {
            tiles,
            seed,
            system,
            colour,
            failed,
            count,
            on,
            note,
            from,
        } => {
            let c = choice(system, colour, seed, on, note);
            let once = |id: &Identity, n: &disk::Named, on: &str| {
                doc::print_line(
                    id,
                    !failed,
                    on,
                    Some(&n.hash),
                    from.as_deref(),
                    c.note.as_deref(),
                )
            };
            let many = |id: &Identity, n: &disk::Named, on: &str| {
                vec![once(id, n, on); count.max(1) as usize]
            };
            write_lines(&home, &c, &tiles, &many, "have")
        }
        InventoryAction::Gone {
            tiles,
            seed,
            system,
            colour,
            on,
            note,
        } => {
            let c = choice(system, colour, seed, on, note);
            let mk = |id: &Identity, _: &disk::Named, on: &str| {
                vec![doc::gone_line(id, on, c.note.as_deref())]
            };
            write_lines(&home, &c, &tiles, &mk, "gone")
        }
        InventoryAction::Relink { old, new, on } => {
            let on = match date_of(on.as_deref()) {
                Ok(d) => d,
                Err(e) => {
                    tell!("error: {e}");
                    return ERROR;
                }
            };
            let cwd = std::env::current_dir().ok();
            // the old name need not be a tile any more; the new one must be
            let to =
                match disk::name_tile(&new, cwd.as_deref(), Some(&home), dt_lang::System::Db, None)
                {
                    Ok(n) => n.canon,
                    Err(e) => return fail(&e, json),
                };
            let from = if old.starts_with("lib:") || old.starts_with("home:") {
                old.trim_end_matches(".dtile").to_owned()
            } else {
                let abs = cwd.map_or_else(|| PathBuf::from(&old), |c| c.join(&old));
                disk::canonical(&abs.to_string_lossy(), Some(&home))
            };
            let (inv, _) = match read(&home) {
                Ok(x) => x,
                Err(e) => return fail(&e, json),
            };
            if !inv.pieces().iter().any(|p| p.id.tile == from) {
                tell!(
                    "error: the inventory has nothing for {from} (`dharmatiles inventory list --system all` shows the paths it knows)"
                );
                return ERROR;
            }
            match disk::append(&home, &[doc::renamed_line(&from, &to, &on)]) {
                Ok(p) => {
                    tell!("{from} is now {to}: {}", p.display());
                    OK
                }
                Err(e) => fail(&e, json),
            }
        }
    }
}

/// The inventory's rows as the page and `list --json` read them: every piece with what is held, and whether its tile
/// changed or is gone.
pub(crate) fn pieces_of(inv: &doc::Inventory, home: &Path) -> Vec<InventoryRow> {
    inv.pieces()
        .into_iter()
        .map(|p| InventoryRow {
            edited: p.have > 0 && disk::edited_since(&p.id.tile, p.hash.as_deref(), Some(home)),
            missing: !disk::tile_exists(&p.id.tile, Some(home)),
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
pub(crate) fn state_of(home: &Path) -> Result<InventoryState, Vec<Diag>> {
    let path = disk::path_in(home);
    let path_text = path.to_string_lossy().into_owned();
    let id = crate::preview::tile_id(&path_text);
    let home_text = home.to_string_lossy().into_owned();
    Ok(match disk::load(home)? {
        None => InventoryState::Absent {
            home: home_text,
            path: path_text,
            id,
        },
        Some(l) => InventoryState::Read {
            home: home_text,
            path: path_text,
            id,
            pieces: pieces_of(&l.inventory, home),
            last_colour: last_colour(&l.inventory).into(),
        },
    })
}
