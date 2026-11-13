//! A print list in the editor: its plates on the printer's bed in the 3D view.
//!
//! Opening a `.dmap` plans it (the tiles, the printer, the filaments), shows
//! the plate in view built from the **low** tier of each piece, and starts
//! the full builds of the pieces on that plate (the others build when you
//! export).  When they land the plates are laid out again by what each piece
//! really covers, and the view swaps to the full meshes.  Every step is said
//! in the message stream and the status line: the low meshes are marked *not
//! yet checked* until the full builds replace them, never silently.
//!
//! The page's side of it is `js/editor/plates.js`: a panel (the printer, the
//! mode, the plates, the filament check, Export) over this module's JSON.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use dt_doc::map::Mode;
use dt_eval::plates::{self, Boxes, Context, Layout, Overrides, Plan};
use dt_eval::{Built, Diag, Files, Tier};
use dt_kit::{Material, TriMesh};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::api::{Out, Reply};
use super::edit::Done;
use super::lists::ListEdited;
use super::messages::Scope;
use super::scene::{self, Note, Scene};
use super::{Shared, State, edit, fail, lock, messages, payload};
use crate::message::{Kind, Message, Severity};
use crate::plates as cli;

// ---- the calls' types ------------------------------------------------------------------

#[derive(Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(optional_fields = nullable))]
#[serde(deny_unknown_fields)]
pub struct PlatesShowReq {
    /// The print list's id.
    pub list: String,
    /// The plate in view, from 0.
    pub plate: usize,
}

#[derive(Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(optional_fields = nullable))]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PlatesSetReq {
    pub list: String,
    /// `printer`, `mode`, `group`, `gap`, `margin`, `turn`, `system` or `inventory`.
    pub key: String,
    /// The word; null takes it off.
    pub value: Option<String>,
    pub doc_rev: u64,
}

#[derive(Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(optional_fields = nullable))]
#[serde(deny_unknown_fields)]
pub struct PlatesExportReq {
    pub list: String,
}

/// The plates written.
#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct PlatesExported {
    pub files: Vec<String>,
    pub format: String,
    /// The pieces that fail the printability gate, by tile.
    pub failures: Vec<String>,
    pub plates: usize,
    pub total_s: f64,
    pub out: String,
}

/// Is this document a print list?
pub(super) fn is_plates(path: &str) -> bool {
    path.to_ascii_lowercase().ends_with(".dmap")
}

/// A piece's best build at hand: the full one when the cache has it, else
/// the low one (built now, and kept).
struct Shown {
    /// The build, thinned for the view (a plate of a dozen full grass tiles
    /// is millions of triangles the page does not need to draw).
    built: Built,
    /// What the whole build covers: the layout is made from this, never from
    /// the thinned mesh.
    boxes: Boxes,
    full: bool,
}

/// The triangles a plate of `pieces` may show, over all of them.
const PLATE_BUDGET: usize = 900_000;

/// Is the full build of `path` already kept?
fn full_kept(shared: &Shared, path: &str, opts: &dt_eval::Options) -> bool {
    let Some(store) = shared.store.as_deref() else {
        return false;
    };
    let tracked = dt_eval::Tracked::new(&Files);
    // a tile that does not check cannot be built; its error comes with the build
    if dt_eval::check(&tracked, path, opts).is_err() {
        return false;
    }
    store.contains(&store.key(&tracked.reads(), path, opts))
}

fn shown(shared: &Shared, plan: &Plan, k: usize, want_full: bool) -> Result<Shown, Vec<Diag>> {
    let opts = plan.options(k, &shared.options(), shared.home.as_deref())?;
    let kind = &plan.kinds[k];
    let full = want_full || full_kept(shared, &kind.path, &opts);
    let built = dt_eval::build_tier(
        &Files,
        &kind.path,
        &opts,
        if full { Tier::Full } else { Tier::Low },
        &dt_eval::Progress::none(),
        None,
        shared.store.as_deref(),
    )?;
    let boxes = Boxes::of(&built.export.mesh, &built.materials);
    let per = (PLATE_BUDGET / plan.pieces().clamp(1, 36)).clamp(12_000, 400_000);
    Ok(Shown {
        built: dt_eval::tiers::thin_built(built, per),
        boxes,
        full,
    })
}

/// What a plate's mesh is made of: the mesh, its colours, and each triangle's
/// piece (its place in the plate's list, so the page can tell which piece a
/// point is on).
type PlateMesh = (TriMesh, Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<u16>);

/// The pieces on one plate, as one mesh with each triangle's colour: every
/// piece baked at its place, resting on the bed.
fn plate_mesh(
    layout: &Layout,
    parts: &[Shown],
    plate: usize,
    palette: &dt_kit::Palette,
) -> PlateMesh {
    let mut mesh = TriMesh::default();
    let (mut painted, mut clean) = (Vec::new(), Vec::new());
    let mut owner: Vec<u16> = Vec::new();
    let colours: Vec<_> = parts
        .iter()
        .map(|p| p.built.colours_pair(palette))
        .collect();
    for (n, i) in layout
        .plates
        .get(plate)
        .map_or(&[][..], Vec::as_slice)
        .iter()
        .enumerate()
    {
        let part = &parts[i.kind].built;
        // placed by the box the packer measured (the full mesh's), not the box of the thinned
        // mesh drawn here, which is smaller and would stand the piece off its footprint
        let bx = &parts[i.kind].boxes;
        let base = u32::try_from(mesh.positions.len()).unwrap_or(u32::MAX);
        mesh.positions.extend(dt_kit::stand_positions(
            &part.export.mesh.positions,
            bx.lo,
            bx.hi(),
            i.at.x,
            i.at.y,
            i.at.turned,
        ));
        mesh.triangles.extend(
            part.export
                .mesh
                .triangles
                .iter()
                .map(|t| t.map(|v| v + base)),
        );
        painted.extend_from_slice(&colours[i.kind].0);
        clean.extend_from_slice(&colours[i.kind].1);
        owner.extend(std::iter::repeat_n(
            u16::try_from(n).unwrap_or(u16::MAX - 1),
            part.export.mesh.triangles.len(),
        ));
    }
    (mesh, painted, clean, owner)
}

/// The settings as written, for a page that has no plan to show (the list
/// does not plan): the panel still offers the printer and the mode, so the
/// mistake can be mended there.
fn loose_json(list: &dt_doc::map::PrintList, home: Option<&Path>) -> Value {
    let pl = &list.plates;
    let printer = pl.printer.as_ref().map_or_else(
        || dt_doc::map::DEFAULT_PRINTER.to_owned(),
        |p| p.value.clone(),
    );
    let machine = dt_eval::printer_machine(&printer, home).ok();
    json!({
        "printer": printer,
        "label": machine.as_ref().map(dt_eval::Machine::label),
        "bed": machine.as_ref().and_then(|m| m.bed),
        "slots": machine.as_ref().map(dt_eval::Machine::slot_count),
        "mode": pl.mode.as_ref().map(|m| m.value.word()),
        "group": pl.group.as_ref().map(|m| m.value.word()),
        "gap": pl.gap.as_ref().map(|m| m.value),
        "margin": pl.margin.as_ref().map(|m| m.value),
        "printers": printers_json(home),
        "check_inventory": pl.inventory.as_ref().is_none_or(|i| i.value),
        "pieces": pieces_json(list),
        "planned": false,
    })
}

/// The list's own pieces, in the order written, for the panel to edit: the
/// count, and taking one out, are edits of the list's text.
pub(super) fn pieces_json(list: &dt_doc::map::PrintList) -> Value {
    let ids = dt_doc::map::piece_ids(list);
    Value::Array(
        list.pieces
            .iter()
            .enumerate()
            .map(|(i, p)| {
                json!({
                    // the id the piece has, or the one the first edit of the file will write on it
                    "tile": p.tile.value, "count": p.count.value,
                    "id": ids[i],
                    "pinned": list.plates.pins.iter().any(|q| p.id.as_ref().is_some_and(|i| i.value == q.id.value)),
                    "seed": p.seed.as_ref().map(|s| s.value),
                    "system": p.system.as_ref().map(|s| s.value.clone()),
                    "print": p.print.word(),
                })
            })
            .collect(),
    )
}

/// Every profile, for the choosers: brand, then model.  One with no bed
/// (`default`, `fine`) is only floors: it builds tiles but cannot hold plates.
/// A profile that does not read is still listed, with what is wrong with it,
/// so it is never missing without a word.
pub(super) fn printers_json(home: Option<&Path>) -> Vec<Value> {
    let mut out = Vec::new();
    for (name, kind) in dt_eval::printer_names(home) {
        let mine = kind == dt_eval::ProfileKind::Mine;
        match dt_eval::printer_machine(&name, home) {
            Ok(m) => out.push(json!({
                "name": name, "brand": m.brand, "model": m.model, "label": m.label(),
                "bed": m.bed, "height": m.height, "slots": m.slot_count(), "verify": m.verify,
                "zones": m.excluded.len(), "from_slicer": m.from_slicer, "mine": mine,
            })),
            Err(e) => out.push(json!({
                "name": name, "brand": null, "model": null, "label": name, "bed": null,
                "slots": 1, "mine": mine,
                "broken": e.first().map_or_else(|| "does not read".to_owned(), |d| d.message.clone()),
            })),
        }
    }
    out
}

/// What the page's panel reads: the settings, the pieces and the plates.
fn plates_json(
    plan: &Plan,
    layout: &Layout,
    boxes: &[Boxes],
    exact: &[bool],
    view: usize,
    writable: bool,
    home: Option<&Path>,
) -> Value {
    let s = &plan.settings;
    let need = plan
        .kinds
        .iter()
        .enumerate()
        .map(|(k, kind)| {
            if exact[k] {
                boxes[k].filaments.len()
            } else {
                kind.estimate.len()
            }
        })
        .max()
        .unwrap_or(0);
    let mut j = cli::plan_json(plan);
    let names = |v: &[Material]| v.iter().map(|m| m.name()).collect::<Vec<_>>();
    j["kinds"] = Value::Array(
        plan.kinds
            .iter()
            .enumerate()
            .map(|(k, kind)| {
                json!({
                    "tile": kind.tile, "name": kind.name, "count": kind.count,
                    "want": kind.want, "have": kind.have, "other_colour": kind.other_colour,
                    "canon": kind.canon, "seed_used": kind.seed_used,
                    "seed": kind.seed, "system": kind.system, "size": boxes[k].size,
                    "height": boxes[k].height, "exact": exact[k],
                    "filaments": names(if exact[k] { &boxes[k].filaments } else { &kind.estimate }),
                })
            })
            .collect(),
    );
    j["layout"] = cli::layout_json(plan, layout);
    j["view"] = json!(view);
    j["exact"] = json!(exact.iter().all(|e| *e));
    j["grouping_cost"] = json!(layout.grouping_cost);
    j["slot_check"] = json!({
        "need": need, "have": s.machine.slot_count(),
        "ok": s.mode == Mode::Single || need <= s.machine.slot_count() as usize,
    });
    j["writable"] = json!(writable);
    j["format"] = json!(plates::export::Format::for_printer(&s.machine).word());
    j["printers"] = json!(printers_json(home));
    j["planned"] = json!(true);
    j
}

/// Why a list did not plan: the list, if it read, and the mistakes.
type Unplanned = (Option<dt_doc::map::PrintList>, Vec<Diag>);

fn plan_for(
    shared: &Shared,
    path: &str,
    ov: &Overrides,
) -> Result<(dt_doc::map::PrintList, Plan), Box<Unplanned>> {
    let text = cli::read_text(Path::new(path)).map_err(|e| Box::new((None, e)))?;
    plan_from(shared, path, &text, ov)
}

/// [`plan_for`] from the text already read.
fn plan_from(
    shared: &Shared,
    path: &str,
    text: &str,
    ov: &Overrides,
) -> Result<(dt_doc::map::PrintList, Plan), Box<Unplanned>> {
    let list = cli::read_list_text(Path::new(path), text).map_err(|e| Box::new((None, e)))?;
    let base = shared.options();
    let dir = Path::new(path).parent();
    let shelf = cli::shelf_for(&list, ov, shared.home.as_deref())
        .map_err(|e| Box::new((Some(list.clone()), e)))?;
    let ctx = Context {
        sources: &Files,
        list_dir: dir,
        home: shared.home.as_deref(),
        base: &base,
        shelf: shelf.as_ref(),
    };
    match plates::plan(&list, &cli::title_of(Path::new(path)), &ctx, ov) {
        Ok(p) => Ok((list, p)),
        Err(e) => Err(Box::new((Some(list), e))),
    }
}

fn say(shared: &Shared, id: &str, scope: Scope, make: impl FnOnce(&str) -> Vec<Message>) {
    shared.report(id, scope, make);
}

/// The notes of a map as messages: its crowns over taller neighbours, and a
/// piece's own mistakes.
fn scene_messages(notes: &[Note], diags: &[Diag], id: &str, name: &str) -> Vec<Message> {
    let mut out = messages::of_errors(diags, Scope::Check, id, name, &[]);
    for n in notes {
        out.push(
            Message::new(
                Kind::Note,
                Severity::Note,
                Scope::Check,
                id,
                name,
                n.text.clone(),
            )
            .code(n.code)
            .help(n.help.as_deref()),
        );
    }
    out
}

/// Open or rebuild a print list or a map for the page: the map's scene first
/// (cheap: nothing is built but what the crown check looks at), then its plates.
/// A test's hand on the moment between the files' times being taken and the files being read: `path`'s hook runs there,
/// once, so a test can change a file exactly where a real change would be the race.
#[cfg(test)]
pub(super) static AFTER_SEEN: std::sync::Mutex<BTreeMap<String, Box<dyn FnOnce() + Send>>> =
    std::sync::Mutex::new(BTreeMap::new());

#[cfg(test)]
fn after_seen(path: &str) {
    if let Some(hook) = lock(&AFTER_SEEN).remove(path) {
        hook();
    }
}

pub(super) fn rebuild(shared: &Arc<Shared>, id: &str, path: &str) {
    // the times of the files this plan will be of, taken *before* they are read: a file that changes while the plan is
    // made is a change the worker sees afterwards, never one this plan is taken to have included
    let mut seen = BTreeMap::from([(path.to_owned(), crate::watch::mtime(path))]);
    // the inventory decides what prints: the list is laid out again when it changes
    if let Some(h) = shared.home.as_deref() {
        let inv = dt_eval::inventory::path_in(h)
            .to_string_lossy()
            .into_owned();
        let at = crate::watch::mtime(&inv);
        seen.insert(inv, at);
    }
    #[cfg(test)]
    after_seen(path);
    // one read: the revision, the scene and the plan are all of this text (a read that
    // failed is reported by the plan, below)
    // (and the file's revision, read with it: this plan and scene are of that revision)
    let (text, rev) = match shared.docs.read(path) {
        Ok((t, r)) => (Ok(t), r),
        Err(e) => (
            Err(vec![Diag::plain(
                "dt::source",
                format!("cannot read {path}: {e}"),
            )]),
            0,
        ),
    };
    let list = text
        .as_ref()
        .ok()
        .and_then(|t| cli::read_list_text(Path::new(path), t).ok());
    let scene = list.map(|list| {
        let mut sc = scene::read(shared, path, &list, rev);
        // the scene reaches the page now, before anything is built: a map shows its pieces
        // (as boxes, then their meshes) while its plates are still being laid out
        shared.update(id, |t| {
            let mut plan = t.plan.clone().unwrap_or_else(|| {
                let mut loose = loose_json(&list, shared.home.as_deref());
                loose["writable"] = json!(true);
                json!({"kind": "plates", "size_mm": [0, 0], "writable": true, "plates": loose})
            });
            plan["scene"] = sc.json.value();
            plan["docRev"] = json!(rev);
            t.plan = Some(plan);
        });
        if sc.pieces.iter().filter(|p| p.at.is_some()).count() > 1 {
            sc.notes = scene::crowns(shared, &sc);
        }
        (list, sc)
    });
    rebuild_plates(
        shared,
        id,
        path,
        &text,
        rev,
        scene.as_ref().map(|(_, s)| s),
        seen,
    );
}

fn rebuild_plates(
    shared: &Arc<Shared>,
    id: &str,
    path: &str,
    text: &Result<String, Vec<Diag>>,
    rev: u64,
    scene: Option<&Scene>,
    seen: BTreeMap<String, Option<std::time::SystemTime>>,
) {
    let t0 = Instant::now();
    let scene_json = scene.map(|s| s.json.value());
    let kept: Vec<Note> = scene.map_or_else(Vec::new, |s| s.notes.clone());
    shared.update(id, |t| t.scene_notes = kept);
    let no_notes = (Vec::new(), Vec::new());
    let (notes, diags) = scene.map_or(no_notes, |s| (s.notes.clone(), s.diags.clone()));
    let is_map = scene.is_some_and(|s| s.json.map);
    let files = seen;
    let writable = std::fs::metadata(path).is_ok_and(|m| !m.permissions().readonly());
    let view = lock(&shared.tiles)
        .iter()
        .find(|t| t.id == id)
        .map_or(0, |t| t.plate);
    let fail_with = |list: Option<&dt_doc::map::PrintList>, errs: Vec<Diag>, scope: Scope| {
        // a map whose every piece is on your shelf has nothing to print: that is a fact about
        // the plates, not a mistake in the map
        let calm = is_map && errs.len() == 1 && errs[0].code == "dt::inventory";
        say(shared, id, scope, |name| {
            if calm {
                let mut out = scene_messages(&notes, &diags, id, name);
                out.push(Message::new(
                    Kind::Note,
                    Severity::Info,
                    scope,
                    id,
                    name,
                    "Everything on this map is already in your inventory: there is nothing to print".to_owned(),
                ));
                out
            } else {
                let mut out = messages::of_errors(&errs, scope, id, name, &[]);
                out.extend(scene_messages(&notes, &diags, id, name));
                out
            }
        });
        let mut loose = list.map_or_else(
            || json!({"planned": false, "printers": printers_json(shared.home.as_deref())}),
            |l| loose_json(l, shared.home.as_deref()),
        );
        loose["writable"] = json!(writable);
        shared.update(id, |t| {
            t.status = if calm { State::Ready } else { State::Failed };
            t.progress = None;
            t.shown = None;
            t.errors = if calm {
                Vec::new()
            } else {
                super::api::mistakes(&errs)
            };
            t.files = files.clone();
            t.mesh = None;
            t.plan = Some(json!({
                "kind": "plates", "plates": loose, "writable": writable, "size_mm": [0, 0],
                "scene": scene_json, "docRev": rev,
            }));
        });
    };
    let planned = match text {
        Ok(t) => plan_from(shared, path, t, &Overrides::default()),
        Err(e) => Err(Box::new((None, e.clone()))),
    };
    let (list, plan) = match planned {
        Ok(x) => x,
        Err(b) => return fail_with(b.0.as_ref(), b.1, Scope::Check),
    };
    let s = &plan.settings;
    say(shared, id, Scope::Check, |name| {
        let mut out = vec![Message::new(
            Kind::Check,
            Severity::Info,
            Scope::Check,
            id,
            name,
            format!(
                "Read {name}: {} pieces of {} tiles for the {}",
                plan.pieces(),
                plan.kinds.len(),
                s.machine.label()
            ),
        )];
        out.extend(scene_messages(&notes, &diags, id, name));
        for (pin, why) in &plan.loose_pins {
            out.push(
                Message::new(
                    Kind::Note,
                    Severity::Note,
                    Scope::Check,
                    id,
                    name,
                    format!("The pin on {pin} is not used: {why}"),
                )
                .code("dt::pin"),
            );
        }
        if let Some(v) = &s.machine.verify {
            out.push(
                Message::new(
                    Kind::Note,
                    Severity::Note,
                    Scope::Check,
                    id,
                    name,
                    format!(
                        "The {}'s numbers are not yet checked against the maker: {v}",
                        s.machine.label()
                    ),
                )
                .code("dt::verify"),
            );
        }
        out
    });
    // each piece's best build at hand: the full one if it is kept, else the low one
    let mut parts: Vec<Shown> = Vec::new();
    for k in 0..plan.kinds.len() {
        match shown(shared, &plan, k, false) {
            Ok(p) => parts.push(p),
            Err(e) => return fail_with(Some(&list), e, Scope::Build),
        }
    }
    let boxes_of =
        |parts: &[Shown]| -> Vec<Boxes> { parts.iter().map(|p| p.boxes.clone()).collect() };
    let mut boxes = boxes_of(&parts);
    let mut layout = match plates::layout(&plan, &boxes) {
        Ok(l) => l,
        Err(e) => return fail_with(Some(&list), e, Scope::Build),
    };
    let view = view.min(layout.plates.len().saturating_sub(1));
    let palette = shared.palette();
    let push = |layout: &Layout,
                boxes: &[Boxes],
                parts: &[Shown],
                running: bool,
                note: Option<&str>| {
        let (mesh, painted, clean, owner) = plate_mesh(layout, parts, view, &palette);
        let exact: Vec<bool> = parts.iter().map(|p| p.full).collect();
        let mut sp = shared_plan(
            &plan,
            layout,
            boxes,
            &exact,
            view,
            writable,
            shared.home.as_deref(),
        );
        sp["plates"]["pieces"] = pieces_json(&list);
        // each laid-out piece names its list piece by id, as an edit does
        let ids = dt_doc::map::piece_ids(&list);
        if let Some(plates) = sp["plates"]["layout"].as_array_mut() {
            for pl in plates {
                if let Some(on) = pl["pieces"].as_array_mut() {
                    for p in on {
                        p["id"] = p["piece"]
                            .as_u64()
                            .and_then(|i| usize::try_from(i).ok())
                            .and_then(|i| ids.get(i))
                            .map_or(Value::Null, |i| json!(i));
                    }
                }
            }
        }
        sp["scene"] = scene_json.clone().unwrap_or(Value::Null);
        sp["docRev"] = json!(rev);
        let meta = json!({
            "overhang_deg": shared.options().printer.overhang_deg,
            "footprint": s.machine.bed,
            "bed": s.machine.bed, "excluded": s.machine.excluded,
            "plate": view, "plates": layout.plates.len(),
            "tier": if exact.iter().all(|e| *e) { "full" } else if exact.iter().any(|e| *e) { "mixed" } else { "low" },
        });
        let zero = vec![0u16; owner.len()];
        match payload::encode_traced(&mesh, &painted, &clean, &owner, &zero, &meta) {
            Ok(bytes) => shared.update(id, |t| {
                t.status = State::Ready;
                t.plan = Some(sp);
                t.mesh = Some(Arc::new(bytes));
                t.mesh_version += 1;
                t.doc_rev = Some(rev);
                t.files = files.clone();
                t.errors.clear();
                t.shown = running.then(|| {
                    json!({"kind": "preview", "checked": false, "note": note.unwrap_or("low meshes, not yet checked")})
                });
            }),
            Err(e) => {
                let d = Diag::plain("dt::view", format!("cannot prepare the view of {path}: {e}"));
                fail_with(Some(&list), vec![d], Scope::Build);
            }
        }
    };
    let on_view: Vec<usize> = {
        let mut k: Vec<usize> = layout
            .plates
            .get(view)
            .map_or_else(Vec::new, |p| p.iter().map(|i| i.kind).collect());
        k.sort_unstable();
        k.dedup();
        k
    };
    let todo: Vec<usize> = on_view
        .iter()
        .copied()
        .filter(|k| !parts[*k].full)
        .collect();
    say(shared, id, Scope::Build, |name| {
        let low = parts.iter().filter(|p| !p.full).count();
        let mut out = vec![Message::new(
            Kind::Build,
            Severity::Info,
            Scope::Build,
            id,
            name,
            format!(
                "{} pieces on {} plate{}{}{}",
                layout.pieces(),
                layout.plates.len(),
                if layout.plates.len() == 1 { "" } else { "s" },
                if low > 0 {
                    format!(": low meshes for {low} tile{}, not yet checked", if low == 1 { "" } else { "s" })
                } else {
                    String::new()
                },
                if todo.is_empty() {
                    String::new()
                } else {
                    format!("; building {} in full for plate {}", todo.len(), view + 1)
                }
            ),
        )
        .data(json!({"low": low, "plates": layout.plates.len(), "build_s": t0.elapsed().as_secs_f64()}))];
        for (pin, why) in &layout.bad_pins {
            out.push(
                Message::new(
                    Kind::Note,
                    Severity::Note,
                    Scope::Build,
                    id,
                    name,
                    format!("The pin on {pin} could not hold: {why}; it is packed like the rest"),
                )
                .code("dt::pin"),
            );
        }
        if let Some(n) = layout.grouping_cost {
            out.push(
                Message::new(
                    Kind::Note,
                    Severity::Warning,
                    Scope::Build,
                    id,
                    name,
                    format!(
                        "Grouping by filament set takes {n} more plate{} than packing by size",
                        if n == 1 { "" } else { "s" }
                    ),
                )
                .code("dt::grouping")
                .help(Some("set group to size in the panel to pack by size only")),
            );
        }
        out
    });
    push(&layout, &boxes, &parts, !todo.is_empty(), None);
    // Q17: full builds for the plate in view only; the rest build on Export
    let before = layout.plates.clone();
    let mut failing: Vec<(usize, Vec<String>)> = Vec::new();
    for (n, &k) in todo.iter().enumerate() {
        shared.update(id, |t| {
            t.progress = Some(super::api::Progress {
                what: "building".to_owned(),
                fraction: n as f64 / todo.len() as f64,
                label: format!("building {} in full", plan.kinds[k].name),
                key: "plates".to_owned(),
                count: Some((format!("{} of {}", n + 1, todo.len()), 0, 0)),
                elapsed_s: t0.elapsed().as_secs_f64(),
                eta_s: None,
                estimate_s: None,
            });
        });
        match shown(shared, &plan, k, true) {
            Ok(p) => {
                let opts = plan
                    .options(k, &shared.options(), shared.home.as_deref())
                    .ok();
                let f = opts.map_or_else(Vec::new, |o| p.built.audit.failures(&o.printer));
                if !f.is_empty() {
                    failing.push((k, f));
                }
                parts[k] = p;
            }
            Err(e) => return fail_with(Some(&list), e, Scope::Build),
        }
        boxes = boxes_of(&parts);
        layout = match plates::layout(&plan, &boxes) {
            Ok(l) => l,
            Err(e) => return fail_with(Some(&list), e, Scope::Build),
        };
        push(
            &layout,
            &boxes,
            &parts,
            n + 1 < todo.len(),
            Some("building the full meshes"),
        );
    }
    if !todo.is_empty() {
        let moved = before != layout.plates;
        say(shared, id, Scope::Build, |name| {
            let mut out = vec![Message::new(
                Kind::Build,
                Severity::Info,
                Scope::Build,
                id,
                name,
                format!(
                    "{} pieces on {} plate{}: plate {} is built in full and checked ({:.1} s){}",
                    layout.pieces(),
                    layout.plates.len(),
                    if layout.plates.len() == 1 { "" } else { "s" },
                    view + 1,
                    t0.elapsed().as_secs_f64(),
                    if moved {
                        "; the pieces cover a little more than the low meshes did, so the plates were laid out again"
                    } else {
                        ""
                    }
                ),
            )];
            for (k, f) in &failing {
                out.push(
                    Message::new(
                        Kind::Gate,
                        Severity::Error,
                        Scope::Build,
                        id,
                        name,
                        format!(
                            "{} fails the printability gate: {}",
                            plan.kinds[*k].name,
                            f.join("; ")
                        ),
                    )
                    .code("dt::gate"),
                );
            }
            out
        });
    }
    shared.update(id, |t| {
        t.progress = None;
        t.shown = None;
    });
}

/// The page's plan JSON for a finished layout.
fn shared_plan(
    plan: &Plan,
    layout: &Layout,
    boxes: &[Boxes],
    exact: &[bool],
    view: usize,
    writable: bool,
    home: Option<&Path>,
) -> Value {
    let bed = plan.settings.machine.bed.unwrap_or([0.0, 0.0]);
    json!({
        "kind": "plates", "tile": plan.title, "cols": 1, "rows": 1,
        "square_mm": dt_kit::Frame::for_system(&plan.settings.system, 1, 1).square_mm,
        "size_mm": bed, "datum_z": 0.0, "stack": [],
        "plates": plates_json(plan, layout, boxes, exact, view, writable, home),
    })
}

fn bad(msg: &str) -> (&'static str, Value) {
    fail::refused("400 Bad Request", msg)
}

fn slot_of(shared: &Shared, id: &str) -> Result<(String, String), Reply> {
    lock(&shared.tiles)
        .iter()
        .find(|t| t.id == id && is_plates(&t.path))
        .map(|t| (t.id.clone(), t.path.clone()))
        .ok_or_else(|| fail::refused("404 Not Found", "no such print list"))
}

/// `/api/plates/show` `{list, plate}`: the plate in view.
pub(super) fn show(shared: &Shared, v: PlatesShowReq) -> Out<Done> {
    let (id, _) = slot_of(shared, &v.list)?;
    if let Some(t) = lock(&shared.tiles).iter_mut().find(|t| t.id == id) {
        t.plate = v.plate;
    }
    again(shared, &id);
    Ok(Done {})
}

/// Build the list again: its plate in view or its file changed.
pub(super) fn again(shared: &Shared, id: &str) {
    // building from now: a plan shown as ready while a newer one is due would be read as the answer
    shared.update(id, |t| {
        t.stale = true;
        if t.status != State::Idle {
            t.status = State::Building;
        }
    });
    shared.queue_cv.notify_all();
}

/// `text` with one word of the list's `plates` line set (null takes it off),
/// the rest of it as it is.
///
/// # Errors
/// The text does not parse.
fn with_word(path: &str, text: &str, key: &str, value: Option<&str>) -> Result<String, String> {
    let mut text = text.to_owned();
    // a list with no plates line gets one at the end
    let has = text
        .lines()
        .any(|l| l.trim_start().starts_with("plates") && !l.trim_start().starts_with("plates_"));
    if !has {
        if !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str("\nplates\n");
    }
    let mut doc: kdl::KdlDocument = text
        .parse()
        .map_err(|e: kdl::KdlError| format!("{path} is not KDL: {e}"))?;
    let node = doc
        .nodes_mut()
        .iter_mut()
        .find(|n| n.name().value() == "plates")
        .ok_or_else(|| format!("{path} has no plates line"))?;
    match value {
        Some(v) => {
            node.insert(key, kdl::KdlValue::String(v.to_owned()));
        }
        None => {
            node.remove(key);
        }
    }
    Ok(doc.to_string())
}

/// `/api/plates/set` `{list, key, value, docRev}`: change a word of the `plates` line
/// (the printer, the mode, the grouping, the gap, the margin, turns, the
/// system); the file is the document, so it is rewritten and the list builds
/// again.  A value of null removes the word.
pub(super) fn set(shared: &Shared, v: PlatesSetReq) -> Out<ListEdited> {
    let (id, path) = slot_of(shared, &v.list)?;
    let key = v.key.as_str();
    if ![
        "printer",
        "mode",
        "group",
        "gap",
        "margin",
        "turn",
        "system",
        "inventory",
    ]
    .contains(&key)
    {
        return Err(bad("that is not a setting of the plates line"));
    }
    let value = v.value.as_deref();
    // check the word the way the file would be read, before writing
    let probe = format!(
        "dharmatiles version=1\nmap {{\n piece \"x\"\n}}\nplates {key}=\"{}\"\n",
        value.unwrap_or("")
    );
    if value.is_some()
        && let Err(e) = dt_doc::map::decode("plates", &probe)
        && e.iter().any(|d| d.code != "dt::unknown-node")
    {
        return Err(bad(&e[0].message));
    }
    let r = shared.docs.edit(&path, Some(v.doc_rev), |text| {
        with_word(&path, text, key, value)
            .map(|t| (t, ()))
            .map_err(|e| fail::failure("500 Internal Server Error", e))
    });
    match r {
        Ok((e, ())) => {
            if key == "printer" {
                // another bed: back to the first plate (the printer itself is the document's word,
                // which the answer's scene carries: the chip and the status line read it there)
                shared.update(&id, |t| t.plate = 0);
            }
            again(shared, &id);
            Ok(super::lists::done(shared, &path, &e, None))
        }
        Err(e) => Err(super::lists::with_current(shared, &path, e)),
    }
}

/// `/api/plates/export` `{tile}`: build every piece in full, lay the plates
/// out by what they cover and write the files.  This runs the whole build:
/// the page waits, the stream says what is happening.
pub(super) fn export(shared: &Shared, v: PlatesExportReq) -> Out<PlatesExported> {
    let (id, path) = slot_of(shared, &v.list)?;
    let rev = shared.rev_of(&path);
    // on the export lane, one at a time, in its turn
    edit::lane_out(shared.on_lane(
        super::queue::Lane::Export,
        format!("export|{id}"),
        rev,
        &id.clone(),
        move |shared, _| Ok(export_now(shared, &id, &path)),
    ))
}

fn export_now(shared: &Shared, id: &str, path: &str) -> Out<PlatesExported> {
    let id = id.to_owned();
    let path = path.to_owned();
    let t0 = Instant::now();
    let report_err = |errs: &[Diag]| {
        say(shared, &id, Scope::Export, |name| {
            messages::of_errors(errs, Scope::Export, &id, name, &[])
        });
        fail::mistakes(errs)
    };
    let (_, plan) = match plan_for(shared, &path, &Overrides::default()) {
        Ok(x) => x,
        Err(b) => return Err(report_err(&b.1)),
    };
    let s = &plan.settings;
    let fmt = plates::export::Format::for_printer(&s.machine);
    let tpl = if fmt == plates::export::Format::Bambu {
        match cli::template_for(&s.machine, None, shared.home.as_deref()) {
            Ok(Some(t)) => Some(t),
            Ok(None) => {
                return Err(report_err(&[Diag::plain(
                    "dt::plates",
                    "a Bambu Studio project needs a template for the printer's settings",
                )
                .help("save any project from Bambu Studio for this printer as bambu.3mf in your tiles folder")]));
            }
            Err(e) => return Err(report_err(&[Diag::plain("dt::plates", e)])),
        }
    } else {
        None
    };
    let palette = shared
        .palette_checked()
        .map_err(|e| fail::failure("500 Internal Server Error", e))?;
    let base = shared.options();
    let builds = match cli::build_all(
        &plan,
        &base,
        shared.home.as_deref(),
        shared.store.as_deref(),
        &mut |n, of, name| {
            shared.update(&id, |t| {
                t.progress = Some(super::api::Progress {
                    what: "building".to_owned(),
                    fraction: (n - 1) as f64 / of as f64,
                    label: format!("building {name} in full for the export"),
                    key: "plates".to_owned(),
                    count: Some((format!("{n} of {of}"), 0, 0)),
                    elapsed_s: t0.elapsed().as_secs_f64(),
                    eta_s: None,
                    estimate_s: None,
                });
            });
        },
    ) {
        Ok(b) => b,
        Err(e) => {
            shared.update(&id, |t| t.progress = None);
            return Err(report_err(&e));
        }
    };
    shared.update(&id, |t| t.progress = None);
    let layout = match cli::layout_of(&plan, &builds) {
        Ok(l) => l,
        Err(e) => return Err(report_err(&e)),
    };
    let b0 = &builds.built[0];
    let record = dt_kit::BuildRecord {
        engine: b0.record.engine.clone(),
        document: path.clone(),
        document_hash: b0.record.document_hash,
        seed: 0,
        system: s.system.clone(),
        printer: s.machine.name.clone(),
    };
    let refs: Vec<&Built> = builds.built.iter().collect();
    let files = match plates::export::write_set(
        &plan,
        &layout,
        &refs,
        fmt,
        &palette,
        tpl.as_deref(),
        &record,
    ) {
        Ok(f) => f,
        Err(e) => return Err(report_err(&[e])),
    };
    let out = edit::export_dir(&path)
        .map(|o| o.join("plates").join(&plan.title))
        .map_err(|e| fail::failure("500 Internal Server Error", e))?;
    std::fs::create_dir_all(&out).map_err(|e| {
        fail::failure(
            "500 Internal Server Error",
            format!("cannot create {}: {e}", out.display()),
        )
    })?;
    let mut written = Vec::new();
    for (name, bytes) in &files {
        let p = out.join(name);
        dt_eval::atomic::write_atomic(&p, bytes).map_err(|e| {
            fail::failure(
                "500 Internal Server Error",
                format!("cannot write {}: {e}", p.display()),
            )
        })?;
        written.push(p.display().to_string());
    }
    let failing: Vec<String> = builds
        .failures
        .iter()
        .enumerate()
        .flat_map(|(k, f)| {
            let name = plan.kinds[k].name.clone();
            f.iter()
                .map(move |l| format!("{name}: {l}"))
                .collect::<Vec<_>>()
        })
        .collect();
    say(shared, &id, Scope::Export, |name| {
        let mut out = vec![Message::new(
            Kind::Export,
            Severity::Info,
            Scope::Export,
            &id,
            name,
            format!(
                "Exported {} plate{} ({} piece{}) for the {}: {}",
                layout.plates.len(),
                if layout.plates.len() == 1 { "" } else { "s" },
                layout.pieces(),
                if layout.pieces() == 1 { "" } else { "s" },
                s.machine.label(),
                written
                    .iter()
                    .map(|f| f.rsplit('/').next().unwrap_or(f))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        )
        .data(json!({"files": written, "format": fmt.word(), "build_s": t0.elapsed().as_secs_f64()}))];
        for f in &failing {
            out.push(
                Message::new(
                    Kind::Gate,
                    Severity::Error,
                    Scope::Export,
                    &id,
                    name,
                    format!("{f}: the printability gate fails"),
                )
                .code("dt::gate"),
            );
        }
        out
    });
    // the pieces are full builds now: the view may show them
    again(shared, &id);
    Ok(PlatesExported {
        files: written,
        format: fmt.word().to_owned(),
        failures: failing,
        plates: layout.plates.len(),
        total_s: t0.elapsed().as_secs_f64(),
        out: out.display().to_string(),
    })
}
