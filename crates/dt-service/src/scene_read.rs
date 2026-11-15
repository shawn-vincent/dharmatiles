//! Reading a map: the scene a `.dmap` with placed pieces draws, with no geometry (each tile's size, the seed it is built
//! with, where it stands, how it is turned and how high its zero stands above the ground).  Written on [`SceneHost`]: the
//! program and a web worker both read a map with it.
//!
//! Positions are the document's: across and down (south) from the map's north-west corner, in millimetres, the piece's own
//! square (35 mm on DungeonBlocks, 25.4 on OpenLOCK) where it was written in `sq`.  The tile's own frame has x east and y
//! north from its south-west corner; [`Placement`] takes that to the map's, turned clockwise by `rot` about the footprint's
//! middle.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use dt_doc::map::{PrintList, parse_height};
use dt_eval::{Diag, Machine, Options, Source};
use dt_value::LockExt as _;

use crate::scene::{
    HeightName, Note, OwnedSeed, Placement, SceneGrid, SceneJson, ScenePiece, ShelfMark,
};
use crate::tile::tile_id;
use crate::tile_host::TileHost;

/// What a map reading asks of whoever runs it, beyond the tile calls' ([`TileHost`]): where My tiles is, where a piece's
/// tile is read from, which tiles are listed, a printer's machine, the shelf and what a map reading remembers.
pub trait SceneHost: TileHost {
    /// The folder `home:` names (My tiles); none where there is none.
    fn tiles_home(&self) -> Option<PathBuf>;
    /// Where a piece's tile (and what it uses) is read from.
    fn tile_source(&self) -> &dyn Source;
    /// The path of every tile the host lists.
    fn tile_paths(&self) -> Vec<String>;
    /// Where printer profiles are found.
    fn profiles(&self) -> Box<dyn dt_eval::Profiles + '_>;
    /// What the user owns: the inventory, `None` where there is none yet.
    ///
    /// # Errors
    /// The inventory is there and cannot be read, or has mistakes (each at its text).
    fn inventory(&self) -> Result<Option<dt_doc::inventory::Inventory>, Vec<Diag>>;
    /// What a reading of a map remembers between readings.
    fn scene_memo(&self) -> &SceneMemo;
}

/// A printer profile's machine, by name.
///
/// # Errors
/// The profile is not there, or does not decode.
pub fn machine(host: &impl SceneHost, printer: &str) -> Result<Machine, Vec<Diag>> {
    dt_eval::printer_machine_in(&*host.profiles(), printer)
}

/// What the user owns as a map's reading sees it.
#[expect(
    clippy::disallowed_methods,
    reason = "permitted silence: a probe where an inventory that does not read counts as none here; the snapshot and the plan report what is wrong with it"
)]
#[must_use]
pub fn shelf(host: &impl SceneHost) -> Option<dt_doc::inventory::Inventory> {
    host.inventory().ok().flatten()
}

/// A tile's size and seed as a piece reads them, by tile, seed, system and the file's time.
type Memo = (String, Option<u64>, dt_lang::System, Option<SystemTime>);

/// What a reading of a map remembers: a tile's squares across and down and the seed it builds with, while its file is as
/// it was (a map's reading is asked often, after every edit of it).
#[derive(Default)]
pub struct SceneMemo(Mutex<BTreeMap<Memo, (u32, u32, u64)>>);

/// What a tile's check said, by tile, seed and system: its squares across and down, and the seed it builds with.
type Checks = BTreeMap<(String, Option<u64>, dt_lang::System), Result<(u32, u32, u64), String>>;

/// One piece as the scene knows it.
pub struct PieceInfo {
    pub index: usize,
    pub id: Option<String>,
    pub name: String,
    pub path: String,
    pub seed_used: u64,
    pub seed: Option<u64>,
    pub system: dt_lang::System,
    pub at: Option<Placement>,
    pub error: Option<String>,
}

/// What a map says: for the page, and what the program goes on to do.
pub struct Scene {
    pub json: SceneJson,
    pub pieces: Vec<PieceInfo>,
    /// Mistakes in a piece's own words (a height that names nothing), at their text.
    pub diags: Vec<Diag>,
    pub notes: Vec<Note>,
    pub printer: String,
}

/// Is this list a map (placed pieces, or a grid), rather than a print list?
pub fn is_map(list: &PrintList) -> bool {
    list.grid.is_some() || list.pieces.iter().any(|p| p.place.is_some())
}

/// The standard heights, `lib:parts/heights.dtile`'s lets: name to its text.
fn standard_heights(lib: &dyn Source) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let Ok(text) = lib.read("lib:parts/heights.dtile") else {
        return out;
    };
    let Ok(doc) = text.parse::<kdl::KdlDocument>() else {
        return out;
    };
    for n in doc.nodes().iter().filter(|n| n.name().value() == "let") {
        let args: Vec<&kdl::KdlEntry> = n.entries().iter().filter(|e| e.name().is_none()).collect();
        if let [name, expr] = args.as_slice()
            && let (Some(nm), Some(ex)) = (name.value().as_string(), expr.value().as_string())
        {
            out.insert(nm.trim_start_matches('$').to_owned(), ex.to_owned());
        }
    }
    out
}

/// A name's value in mm for a square of `sq`, through the names it uses.
fn name_mm(
    name: &str,
    names: &BTreeMap<String, String>,
    sq: f64,
    depth: usize,
) -> Result<f64, String> {
    if depth > 8 {
        return Err(format!("`${name}` refers to itself"));
    }
    let Some(text) = names.get(name) else {
        let known: Vec<&str> = names.keys().map(String::as_str).collect();
        return Err(format!(
            "`${name}` is not a height name (the standard ones are {})",
            known
                .iter()
                .filter(|k| k.ends_with("Height"))
                .map(|k| format!("${k}"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    };
    let h = parse_height(text).map_err(|e| {
        format!("`${name}` is `{text}`, which varies across a tile; a piece's bottom is one length ({e})")
    })?;
    h.mm(sq, &|n| name_mm(n, names, sq, depth + 1))
}

/// The map's names: the standard heights, then its own `let`s.
fn names_of(list: &PrintList, lib: &dyn Source) -> BTreeMap<String, String> {
    let mut names = standard_heights(lib);
    for l in &list.lets {
        names.insert(l.name.value.clone(), l.expr.value.clone());
    }
    names
}

/// The standard height names with their value in dmm, for the page's chooser.
#[expect(
    clippy::disallowed_methods,
    reason = "permitted silence: a probe where a name whose expression does not evaluate is left out of the chooser; the check reports the expression where it is written"
)]
fn height_names(list: &PrintList, lib: &dyn Source) -> Vec<HeightName> {
    let names = names_of(list, lib);
    let own: Vec<&str> = list.lets.iter().map(|l| l.name.value.as_str()).collect();
    names
        .keys()
        .filter_map(|k| {
            let v = name_mm(k, &names, dt_doc::schema::SQUARE_MM, 0).ok()?;
            Some(HeightName {
                name: k.clone(),
                dmm: v,
                own: own.contains(&k.as_str()),
            })
        })
        .collect()
}

/// The mode a list's plates would be in, as `single` or `multi` (the
/// inventory's word).
#[expect(
    clippy::disallowed_methods,
    reason = "permitted silence: a probe where a printer that does not read counts as one slot; the plan reports the printer where it is named"
)]
fn colour_of(list: &PrintList, printer: &str, host: &impl SceneHost) -> &'static str {
    use dt_doc::map::Mode;
    let slots = machine(host, printer).map_or(1, |m| m.slot_count());
    let mode = list.plates.mode.as_ref().map_or(
        if slots > 1 {
            Mode::Multicolour
        } else {
            Mode::Single
        },
        |m| m.value,
    );
    dt_print::plates::colour_of(mode).word()
}

/// Say what the shelf holds of each piece, in the order written: copies of one piece share what you own, so the first
/// ones are the ones you have.  With no inventory nothing is had and every piece prints.  Gives whether an inventory
/// exists, how many pieces it leaves off and how many print.
///
/// An inventory that does not read counts as none here: the snapshot says what is wrong with it.
fn mark_shelf(
    pieces: &mut [ScenePiece],
    loaded: Option<dt_doc::inventory::Inventory>,
) -> (bool, usize, usize) {
    use dt_doc::inventory::{Colour, Identity, Shelf};
    let shelf = loaded.as_ref().map(Shelf::of);
    let exists = shelf.is_some();
    let mut left: BTreeMap<Identity, u32> = BTreeMap::new();
    let (mut have, mut to_print) = (0, 0);
    for p in pieces.iter_mut().filter(|p| p.error.is_none()) {
        if let Some(l) = &loaded {
            let mut by: BTreeMap<u64, u32> = BTreeMap::new();
            let colour = if p.colour == "multi" {
                Colour::Multi
            } else {
                Colour::Single
            };
            for q in l.pieces() {
                if q.have > 0
                    && q.id.tile == p.canon
                    && q.id.system == p.system
                    && q.id.colour == colour
                {
                    *by.entry(q.id.seed).or_insert(0) += q.have;
                }
            }
            p.owned_seeds = by
                .into_iter()
                .map(|(seed, have)| OwnedSeed { seed, have })
                .collect();
        }
        let id = Identity {
            tile: p.canon.clone(),
            seed: p.seed_used,
            system: p.system,
            colour: if p.colour == "multi" {
                Colour::Multi
            } else {
                Colour::Single
            },
        };
        let mark = |state: &str, text: &str| ShelfMark {
            state: state.to_owned(),
            text: text.to_owned(),
        };
        if p.print == "skip" {
            p.shelf = Some(mark("skip", "left off"));
            continue;
        }
        let n = *left
            .entry(id.clone())
            .or_insert_with(|| shelf.as_ref().map_or(0, |s| s.have(&id)));
        let other = shelf.as_ref().map_or(0, |s| s.other_colour(&id));
        if p.print != "always" && exists && n > 0 {
            left.insert(id, n - 1);
            p.shelf = Some(mark("have", "have"));
            have += 1;
        } else if exists && n == 0 && other > 0 {
            p.shelf = Some(mark("other", "to print"));
            to_print += 1;
        } else {
            p.shelf = Some(mark("need", "to print"));
            to_print += 1;
        }
    }
    (exists, have, to_print)
}

/// Read a map: its pieces' tiles, places and heights.  No tile is built.
#[expect(
    clippy::disallowed_methods,
    reason = "permitted silence: parsing where None is the meaning (a probe of a written word or an optional parameter)"
)]
pub fn read(host: &impl SceneHost, path: &str, list: &PrintList, rev: u64) -> Scene {
    let base = host.options();
    let home_path = host.tiles_home();
    let home = home_path.as_deref();
    let tile_source = host.tile_source();
    let dir = Path::new(path).parent();
    let printer = list.plates.printer.as_ref().map_or_else(
        || dt_doc::map::DEFAULT_PRINTER.to_owned(),
        |p| p.value.clone(),
    );
    let default_system = list
        .plates
        .system
        .as_ref()
        .map_or_else(|| base.system, |s| s.value);
    let colour = colour_of(list, &printer, host);
    let names = names_of(list, host.library());
    let tiles = host.tile_paths();
    let mut diags = Vec::new();
    let mut pieces = Vec::new();
    let mut out = Vec::new();
    let mut checked: Checks = BTreeMap::new();
    let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
    let ids = dt_doc::map::piece_ids(list);
    for (index, p) in list.pieces.iter().enumerate() {
        let system = p
            .system
            .as_ref()
            .map_or_else(|| default_system, |s| s.value);
        let sq = dt_kit::Frame::for_system(system, 1, 1).square_mm;
        let tpath = dt_print::plates::tile_path(&p.tile.value, dir, home);
        let mut error = tpath.as_ref().err().cloned();
        let tpath = tpath.unwrap_or_default();
        let seed = p.seed.as_ref().map(|s| s.value);
        let info = if error.is_none() {
            checked
                .entry((tpath.clone(), seed, system))
                .or_insert_with(|| {
                    // a tile's size and seed, kept while its file is as it was (the reading of a map is asked often:
                    // every edit of it); a mistake is never kept, it is found again each time
                    let stamp = host
                        .storage()
                        .stat(Path::new(&tpath))
                        .ok()
                        .and_then(|e| e.modified);
                    let memo = (tpath.clone(), seed, system, stamp);
                    if let Some(known) = host.scene_memo().0.lock_unpoisoned().get(&memo) {
                        return Ok(*known);
                    }
                    let opts = Options {
                        system,
                        seed,
                        ..base.clone()
                    };
                    match dt_eval::check(tile_source, &tpath, &opts) {
                        Ok(c) => c
                            .tile
                            .as_ref()
                            .map(|t| (t.cols, t.rows, c.seed))
                            .inspect(|found| {
                                let mut kept = host.scene_memo().0.lock_unpoisoned();
                                if kept.len() > 500 {
                                    kept.clear();
                                }
                                kept.insert(memo, *found);
                            })
                            .ok_or_else(|| format!("`{}` is not a tile", p.tile.value)),
                        Err(e) => Err(format!(
                            "`{}` does not check: {}",
                            p.tile.value,
                            e.first().map_or("", |d| d.message.as_str())
                        )),
                    }
                })
                .clone()
        } else {
            Err(error.clone().unwrap_or_default())
        };
        let (cols, rows, seed_used) = match &info {
            Ok(x) => *x,
            Err(e) => {
                error = Some(e.clone());
                diags.push(Diag::at(
                    &list.src,
                    p.tile.span,
                    dt_lang::codes::MAP,
                    e.clone(),
                    "this tile",
                ));
                (1, 1, 0)
            }
        };
        let (w, d) = (f64::from(cols) * sq, f64::from(rows) * sq);
        let bottom_mm = match &p.bottom {
            None => Ok(0.0),
            Some(h) => {
                parse_height(&h.value).and_then(|ht| ht.mm(sq, &|n| name_mm(n, &names, sq, 0)))
            }
        };
        let bottom_mm = bottom_mm.unwrap_or_else(|e| {
            if let Some(h) = &p.bottom {
                diags.push(
                    Diag::at(&list.src, h.span, dt_lang::codes::MAP, e.clone(), "this bottom").help(
                        "`bottom` is a `$` name from the standard heights or the map's own `let`, or a length like `20dmm`",
                    ),
                );
            }
            0.0
        });
        let at = p.place.map(|a| Placement {
            x: a.x.mm(sq),
            y: a.y.mm(sq),
            w,
            d,
            turns: u8::try_from(p.rot.as_ref().map_or(0, |r| r.value) / 90).unwrap_or(0),
            z: bottom_mm,
        });
        if let Some(pl) = &at {
            let f = pl.footprint();
            lo = [lo[0].min(f[0]), lo[1].min(f[1])];
            hi = [hi[0].max(f[2]), hi[1].max(f[3])];
        }
        let canon = dt_print::inventory::canonical(&tpath, home);
        let name = tpath
            .rsplit(['/', ':'])
            .next()
            .unwrap_or(&tpath)
            .trim_end_matches(".dtile")
            .to_owned();
        let listed = tiles.contains(&tpath);
        out.push(ScenePiece {
            id: ids[index].clone(),
            written: p.id.is_some(),
            tile: p.tile.value.clone(),
            canon,
            name: name.clone(),
            tile_id: listed.then(|| tile_id(&tpath)),
            path: tpath.clone(),
            seed,
            seed_used,
            system,
            colour: colour.to_owned(),
            cols,
            rows,
            sq,
            size: [w, d],
            at: at.map(|a| [a.x, a.y]),
            rot: p.rot.as_ref().map(|r| r.value),
            bottom_mm,
            bottom: p.bottom.as_ref().map(|h| h.value.clone()),
            print: p.print.word().to_owned(),
            count: p.count.value,
            pinned: list
                .plates
                .pins
                .iter()
                .any(|q| p.id.as_ref().is_some_and(|i| i.value == q.id.value)),
            error: error.clone(),
            shelf: None,
            owned_seeds: Vec::new(),
        });
        pieces.push(PieceInfo {
            index,
            id: Some(ids[index].clone()),
            name,
            path: tpath,
            seed_used,
            seed,
            system,
            at,
            error,
        });
    }
    let placed = pieces.iter().filter(|p| p.at.is_some()).count();
    let grid = list.grid.map(|g| SceneGrid {
        cols: g.cols,
        rows: g.rows,
    });
    let bounds = (placed > 0).then(|| [lo[0], lo[1], hi[0], hi[1]]);
    let (shelf_exists, have, to_print) = mark_shelf(&mut out, shelf(host));
    let square_mm = dt_kit::Frame::for_system(default_system, 1, 1).square_mm;
    let json = SceneJson {
        shelf_exists,
        have,
        to_print,
        doc_rev: rev,
        map: is_map(list),
        grid,
        about: list.about.clone(),
        pieces: out,
        placed,
        bounds,
        heights: height_names(list, host.library()),
        colour: colour.to_owned(),
        system: default_system,
        square_mm,
        printer: printer.clone(),
    };
    Scene {
        json,
        pieces,
        diags,
        notes: Vec::new(),
        printer,
    }
}

/// The scene of `text` (the file at revision `rev`) as the page reads it (no builds).
#[expect(
    clippy::disallowed_methods,
    reason = "permitted silence: a probe where None means no scene yet; a map that does not decode is reported by the check route where its text is typed"
)]
pub fn scene_of(host: &impl SceneHost, path: &str, text: &str, rev: u64) -> Option<SceneJson> {
    dt_doc::map::decode(path, text)
        .ok()
        .map(|list| read(host, path, &list, rev).json)
}
