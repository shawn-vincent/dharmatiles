//! A map in the editor: the scene a `.dmap` with placed pieces draws.
//!
//! Opening a map reads its pieces (no geometry): each tile's size, the seed it
//! is built with, where it stands, how it is turned and how high its
//! tile's zero stands above the ground (`bottom`).  The page asks for each piece's mesh at the tier it wants
//! (`/api/tiles/ID/tier`, from the cache or built), places it with the
//! numbers here, and draws the whole.  A crown over a taller neighbour is a
//! note, found from the pieces' real shapes (their low builds), never a
//! refusal.
//!
//! Positions are the document's: across and down (south) from the map's
//! north-west corner, in millimetres, the piece's own square (35 mm on
//! DungeonBlocks, 25.4 on OpenLOCK) where it was written in `sq`.  The tile's
//! own frame has x east and y north from its south-west corner; [`Placement`]
//! takes that to the map's, turned clockwise by `rot` about the footprint's
//! middle.

use std::collections::BTreeMap;
use std::path::Path;

use dt_doc::map::{PrintList, parse_height};
use dt_eval::{Diag, Files, Options, Tier};
use serde::Serialize;

use super::{Shared, lock};

/// A map as the page reads it (`scene`): pieces, grid, bounds, heights.
#[derive(Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct SceneJson {
    /// The revision of the file this is the reading of.
    pub doc_rev: u64,
    /// A map (placed pieces or a grid) rather than a print list.
    pub map: bool,
    pub grid: Option<SceneGrid>,
    pub about: Option<String>,
    pub pieces: Vec<ScenePiece>,
    /// How many pieces are placed.
    pub placed: usize,
    /// `[x0, y0, x1, y1]` of the placed pieces, mm, y down.
    pub bounds: Option<[f64; 4]>,
    pub heights: Vec<HeightName>,
    /// An inventory exists, so the shelf's word on each piece means something.
    pub shelf_exists: bool,
    /// The pieces the inventory leaves off (`shelf.state` is `have`) and the pieces that print.
    pub have: usize,
    pub to_print: usize,
    /// `single` or `multi`.
    pub colour: String,
    pub system: String,
    /// The square of `system`, mm: where a tile dropped on the map lands.
    pub square_mm: f64,
    pub printer: String,
}

impl SceneJson {
    /// The scene as JSON, for the places that carry it inside another document.
    #[allow(clippy::expect_used)]
    pub(super) fn value(&self) -> serde_json::Value {
        serde_json::to_value(self).expect("a scene is plain data")
    }
}

#[derive(Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct SceneGrid {
    pub cols: u32,
    pub rows: u32,
}

/// A standard or the map's own height name, in dmm.
#[derive(Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct HeightName {
    pub name: String,
    pub dmm: f64,
    pub own: bool,
}

/// One piece of a map.
#[derive(Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct ScenePiece {
    /// The id the piece has, or the one the first edit of the file will write on it.
    pub id: String,
    /// Whether the file has written it yet.
    pub written: bool,
    /// The tile as the piece names it.
    pub tile: String,
    pub canon: String,
    pub name: String,
    /// The listed tile's id, when the tile is listed.
    pub tile_id: Option<String>,
    pub path: String,
    pub seed: Option<u64>,
    pub seed_used: u64,
    pub system: String,
    pub colour: String,
    pub cols: u32,
    pub rows: u32,
    /// One square, mm.
    pub sq: f64,
    /// Width and depth, mm.
    pub size: [f64; 2],
    pub at: Option<[f64; 2]>,
    pub rot: Option<i64>,
    pub bottom_mm: f64,
    pub bottom: Option<String>,
    pub print: String,
    pub count: u32,
    pub pinned: bool,
    pub error: Option<String>,
    /// What the shelf says of the piece (none for a piece with a mistake).
    pub shelf: Option<ShelfMark>,
    /// The seeds you own of this piece's tile in its system and colour mode, and how many of each.
    pub owned_seeds: Vec<OwnedSeed>,
}

/// A seed you own of a tile, and how many prints of it.
#[derive(Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct OwnedSeed {
    pub seed: u64,
    pub have: u32,
}

/// What the inventory says of a piece: `have` (a print you own is this piece), `need` (it prints), `other` (you own
/// it in the other colour mode: it still prints), `skip` (`print=skip`).
#[derive(Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct ShelfMark {
    pub state: String,
    pub text: String,
}

/// A note about the scene for the message stream: said where it was found.
#[derive(Clone, Debug)]
pub(super) struct Note {
    pub text: String,
    pub code: &'static str,
    pub help: Option<String>,
}

/// Where a placed piece stands.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Placement {
    /// Its north-west corner, across and down from the map's, mm.
    pub x: f64,
    pub y: f64,
    /// The tile's width and depth in its own frame, mm.
    pub w: f64,
    pub d: f64,
    /// Clockwise quarter turns: 0, 1, 2 or 3.
    pub turns: u8,
    /// The height above the map's ground of the tile's zero, mm.
    pub z: f64,
}

impl Placement {
    /// The footprint on the map, `[x0, y0, x1, y1]` with y down.
    pub(crate) fn footprint(self) -> [f64; 4] {
        let (w, d) = if self.turns % 2 == 1 {
            (self.d, self.w)
        } else {
            (self.w, self.d)
        };
        [self.x, self.y, self.x + w, self.y + d]
    }

    /// A point of the tile's frame (x east, y north) on the map (y down).
    pub(crate) fn on_map(self, px: f64, py: f64) -> [f64; 2] {
        let f = self.footprint();
        let (cw, cd) = ((f[0] + f[2]) / 2.0, (f[1] + f[3]) / 2.0);
        let (dx, dy) = (px - self.w / 2.0, (self.d - py) - self.d / 2.0);
        let (s, c) = match self.turns % 4 {
            0 => (0.0, 1.0),
            1 => (1.0, 0.0),
            2 => (0.0, -1.0),
            _ => (-1.0, 0.0),
        };
        [cw + dx * c - dy * s, cd + dx * s + dy * c]
    }

    /// A box of the tile's frame as a box on the map.
    pub(crate) fn box_on_map(self, b: [f64; 4]) -> [f64; 4] {
        let corners = [
            self.on_map(b[0], b[1]),
            self.on_map(b[2], b[1]),
            self.on_map(b[2], b[3]),
            self.on_map(b[0], b[3]),
        ];
        let mut r = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
        for c in corners {
            r = [
                r[0].min(c[0]),
                r[1].min(c[1]),
                r[2].max(c[0]),
                r[3].max(c[1]),
            ];
        }
        r
    }
}

/// What a tile's check said, by tile, seed and system: its squares across and down, and the seed it builds with.
type Checks = BTreeMap<(String, Option<u64>, String), Result<(u32, u32, u64), String>>;

/// One piece as the scene knows it.
pub(super) struct PieceInfo {
    pub index: usize,
    pub id: Option<String>,
    pub name: String,
    pub path: String,
    pub seed_used: u64,
    pub seed: Option<u64>,
    pub system: String,
    pub at: Option<Placement>,
    pub error: Option<String>,
}

/// What a map says: for the page, and what the program goes on to do.
pub(super) struct Scene {
    pub json: SceneJson,
    pub pieces: Vec<PieceInfo>,
    /// Mistakes in a piece's own words (a height that names nothing), at their text.
    pub diags: Vec<Diag>,
    pub notes: Vec<Note>,
    pub printer: String,
}

/// Is this list a map (placed pieces, or a grid), rather than a print list?
pub(super) fn is_map(list: &PrintList) -> bool {
    list.grid.is_some() || list.pieces.iter().any(|p| p.place.is_some())
}

/// The standard heights, `lib:parts/heights.dtile`'s lets: name to its text.
fn standard_heights() -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let Ok(text) = dt_eval::Source::read(&Files, "lib:parts/heights.dtile") else {
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
fn names_of(list: &PrintList) -> BTreeMap<String, String> {
    let mut names = standard_heights();
    for l in &list.lets {
        names.insert(l.name.value.clone(), l.expr.value.clone());
    }
    names
}

/// The standard height names with their value in dmm, for the page's chooser.
fn height_names(list: &PrintList) -> Vec<HeightName> {
    let names = names_of(list);
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
fn colour_of(list: &PrintList, printer: &str, home: Option<&Path>) -> &'static str {
    use dt_doc::map::Mode;
    let slots = dt_eval::printer_machine(printer, home).map_or(1, |m| m.slot_count());
    let mode = list.plates.mode.as_ref().map_or(
        if slots > 1 {
            Mode::Multicolour
        } else {
            Mode::Single
        },
        |m| m.value,
    );
    dt_eval::plates::colour_of(mode).word()
}

/// Say what the shelf holds of each piece, in the order written: copies of one piece share what you own, so the first
/// ones are the ones you have.  With no inventory nothing is had and every piece prints.  Gives whether an inventory
/// exists, how many pieces it leaves off and how many print.
///
/// An inventory that does not read counts as none here: the snapshot says what is wrong with it.
fn mark_shelf(pieces: &mut [ScenePiece], home: Option<&Path>) -> (bool, usize, usize) {
    use dt_doc::inventory::{Colour, Identity, Shelf};
    let loaded = home.and_then(|h| dt_eval::inventory::load(h).ok().flatten());
    let shelf = loaded.as_ref().map(|l| Shelf::of(&l.inventory));
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
            for q in l.inventory.pieces() {
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
            system: p.system.clone(),
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
pub(super) fn read(shared: &Shared, path: &str, list: &PrintList, rev: u64) -> Scene {
    let base = shared.options();
    let home = shared.home.as_deref();
    let dir = Path::new(path).parent();
    let printer = list.plates.printer.as_ref().map_or_else(
        || dt_doc::map::DEFAULT_PRINTER.to_owned(),
        |p| p.value.clone(),
    );
    let default_system = list
        .plates
        .system
        .as_ref()
        .map_or_else(|| base.system.clone(), |s| s.value.clone());
    let colour = colour_of(list, &printer, home);
    let names = names_of(list);
    let tiles = lock(&shared.tiles)
        .iter()
        .map(|t| t.path.clone())
        .collect::<Vec<_>>();
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
            .map_or_else(|| default_system.clone(), |s| s.value.clone());
        let sq = dt_kit::Frame::for_system(&system, 1, 1).square_mm;
        let tpath = dt_eval::plates::tile_path(&p.tile.value, dir, home);
        let mut error = tpath.as_ref().err().cloned();
        let tpath = tpath.unwrap_or_default();
        let seed = p.seed.as_ref().map(|s| s.value);
        let info = if error.is_none() {
            checked
                .entry((tpath.clone(), seed, system.clone()))
                .or_insert_with(|| {
                    // a tile's size and seed, kept while its file is as it was (the reading of a map is asked often:
                    // every edit of it); a mistake is never kept, it is found again each time
                    let stamp = crate::watch::mtime(&tpath);
                    let memo = (tpath.clone(), seed, system.clone(), stamp);
                    if let Some(known) = lock(&shared.scene_checks).get(&memo) {
                        return Ok(*known);
                    }
                    let opts = Options {
                        system: system.clone(),
                        seed,
                        ..base.clone()
                    };
                    match dt_eval::check(&Files, &tpath, &opts) {
                        Ok(c) => c
                            .tile
                            .as_ref()
                            .map(|t| (t.cols, t.rows, c.seed))
                            .inspect(|found| {
                                let mut kept = lock(&shared.scene_checks);
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
                    "dt::map",
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
                    Diag::at(&list.src, h.span, "dt::map", e.clone(), "this bottom").help(
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
        let canon = dt_eval::inventory::canonical(&tpath, home);
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
            tile_id: listed.then(|| super::tile_id(&tpath)),
            path: tpath.clone(),
            seed,
            seed_used,
            system: system.clone(),
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
    let (shelf_exists, have, to_print) = mark_shelf(&mut out, home);
    let square_mm = dt_kit::Frame::for_system(&default_system, 1, 1).square_mm;
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
        heights: height_names(list),
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

/// What a built piece's shape says: its highest point, and the part of it
/// that stands out past its square (a crown) with the lowest point there.
struct Shape {
    top: f64,
    crown: Option<([f64; 4], f64)>,
}

fn shape_of(mesh: &dt_kit::TriMesh, w: f64, d: f64) -> Shape {
    let mut top = f64::MIN;
    let mut crown: Option<([f64; 4], f64)> = None;
    let slack = 0.5;
    for p in &mesh.positions {
        let [x, y, z] = p.map(f64::from);
        top = top.max(z);
        if x < -slack || x > w + slack || y < -slack || y > d + slack {
            let c = crown.get_or_insert(([x, y, x, y], z));
            c.0 = [c.0[0].min(x), c.0[1].min(y), c.0[2].max(x), c.0[3].max(y)];
            c.1 = c.1.min(z);
        }
    }
    Shape { top, crown }
}

/// A crown over a taller neighbour: found from each piece's low build (kept
/// beside the full one), said as a note for each pair.  Never a refusal.
pub(super) fn crowns(shared: &Shared, scene: &Scene) -> Vec<Note> {
    let base = shared.options();
    let mut shapes: BTreeMap<(String, u64, String), Option<Shape>> = BTreeMap::new();
    let placed: Vec<&PieceInfo> = scene
        .pieces
        .iter()
        .filter(|p| p.at.is_some() && p.error.is_none())
        .collect();
    for p in &placed {
        let key = (p.path.clone(), p.seed_used, p.system.clone());
        if shapes.contains_key(&key) {
            continue;
        }
        let Some(at) = p.at else { continue };
        let mut opts = Options {
            system: p.system.clone(),
            seed: p.seed,
            ..base.clone()
        };
        if opts
            .use_printer(&scene.printer, shared.home.as_deref())
            .is_err()
        {
            shapes.insert(key, None);
            continue;
        }
        // a piece's low build is a job on the tier lane, like the page's own look at it
        let path = p.path.clone();
        let jobkey = format!("tier|crown|{}|{:?}|{}", p.path, opts.seed, opts.system);
        let built = shared.on_lane(
            super::queue::Lane::Tier,
            jobkey,
            shared.rev_of(&path),
            "",
            move |shared, cancel| {
                Ok(dt_eval::build_tier(
                    &Files,
                    &path,
                    &opts,
                    Tier::Low,
                    &dt_eval::Progress::new(std::sync::Arc::new(super::queue::Cancelled(
                        std::sync::Arc::clone(cancel),
                    ))),
                    None,
                    shared.store.as_deref(),
                ))
            },
        );
        let built = match built {
            Ok(b) => b,
            // a newer reading of the map is on its way and will look again
            Err(super::queue::Interrupted::Superseded) => {
                shapes.insert(key, None);
                continue;
            }
            Err(super::queue::Interrupted::Panicked(pn)) => std::panic::resume_unwind(pn),
        };
        shapes.insert(
            key,
            built.ok().map(|b| shape_of(&b.export.mesh, at.w, at.d)),
        );
    }
    let mut notes = Vec::new();
    for a in &placed {
        let (Some(pa), Some(Some(sa))) = (
            a.at,
            shapes.get(&(a.path.clone(), a.seed_used, a.system.clone())),
        ) else {
            continue;
        };
        let Some((cbox, clow)) = sa.crown else {
            continue;
        };
        let reach = pa.box_on_map(cbox);
        let crown_z = pa.z + clow;
        for b in &placed {
            if b.index == a.index {
                continue;
            }
            let (Some(pb), Some(Some(sb))) = (
                b.at,
                shapes.get(&(b.path.clone(), b.seed_used, b.system.clone())),
            ) else {
                continue;
            };
            let f = pb.footprint();
            let (ox, oy) = (
                reach[2].min(f[2]) - reach[0].max(f[0]),
                reach[3].min(f[3]) - reach[1].max(f[1]),
            );
            let top = pb.z + sb.top;
            if ox > 0.5 && oy > 0.5 && top > crown_z + 0.5 {
                let who = |p: &PieceInfo| {
                    p.id.as_ref()
                        .map_or_else(|| p.name.clone(), |i| format!("{} ({i})", p.name))
                };
                notes.push(Note {
                    text: format!(
                        "The crown of {} reaches over {}, which stands {:.0} mm up into it",
                        who(a),
                        who(b),
                        top - crown_z
                    ),
                    code: "dt::crown",
                    help: Some(
                        "Move one of them, or raise the crown's piece; nothing is refused, the real shapes just meet here"
                            .to_owned(),
                    ),
                });
            }
        }
    }
    notes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: [f64; 2], b: [f64; 2]) {
        assert!(
            (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9,
            "{a:?} {b:?}"
        );
    }

    fn at(x: f64, y: f64, w: f64, d: f64, turns: u8) -> Placement {
        Placement {
            x,
            y,
            w,
            d,
            turns,
            z: 0.0,
        }
    }

    /// The page's `matrixOf` (js/map-model.js) says the same: a tile's south-west corner is the
    /// footprint's south-west, a quarter turn clockwise sends the north edge east.
    #[test]
    fn a_piece_stands_where_the_page_draws_it() {
        let p = at(70.0, 35.0, 70.0, 35.0, 0);
        assert_eq!(p.footprint(), [70.0, 35.0, 140.0, 70.0]);
        near(p.on_map(0.0, 0.0), [70.0, 70.0]);
        near(p.on_map(70.0, 35.0), [140.0, 35.0]);
        let q = at(0.0, 0.0, 70.0, 35.0, 1);
        assert_eq!(q.footprint(), [0.0, 0.0, 35.0, 70.0]);
        // the tile's north-west corner goes to the footprint's north-east
        near(q.on_map(0.0, 35.0), [35.0, 0.0]);
        near(q.on_map(0.0, 0.0), [0.0, 0.0]);
        near(q.on_map(70.0, 0.0), [0.0, 70.0]);
        let r = at(0.0, 0.0, 35.0, 35.0, 2);
        near(r.on_map(0.0, 35.0), [35.0, 35.0]);
        // a crown's box turns with the piece: five mm west of a 70 x 35 tile, turned, lies north of it
        let b = q.box_on_map([-5.0, 0.0, 0.0, 35.0]);
        near([b[0], b[1]], [0.0, -5.0]);
        near([b[2], b[3]], [35.0, 0.0]);
    }
}
