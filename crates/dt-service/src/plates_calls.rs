//! A print list in the editor: its plates on the printer's bed in the 3D view.
//!
//! Opening a `.dmap` plans it (the tiles, the printer, the filaments), shows the plate in view built from the **low**
//! tier of each piece, and builds the pieces on that plate in full.  When they land the plates are laid out again by what
//! each piece really covers, and the view swaps to the full meshes.  Every step is said in the message stream and the
//! status line: the low meshes are marked *not yet checked* until the full builds replace them, never silently.
//!
//! Written on [`PlatesHost`]: the program (its build lanes, its disk cache) and a web worker (its own turn, no cache) both
//! plan a list with it, and `platesShow` and `platesSet` are its verbs.  Writing the plates out (`platesExport`) stays the
//! program's for now.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::SystemTime;

use dt_doc::map::Mode;
use dt_eval::{Built, Diag, Options, Tier};
use dt_kit::{Material, TriMesh};
use dt_print::plates::{self, Boxes, Context, Layout, Overrides, Plan};
use serde::{Deserialize, Serialize};

use crate::api::Progress;
use crate::call::{Refusal, Status, Verb};
use crate::header::{MeshHeader, MeshKind};
use crate::list_calls::{ListEdited, ListHost, done, list_path, with_current};
use crate::message::{Kind, LayoutData, Message, MessageData, Scope, Severity};
use crate::patch::Patch;
use crate::payload;
use crate::plan::{
    ListPiece, PlanGround, PlanKind, PlanPlate, PlanSettings, PlannedKind, PlatesDoc, PlatesPanel,
    PlatesPlanned, PlatesUnread, PlatesWritten, PrinterBroken, PrinterRead, PrinterRow, SlotCheck,
    TilePlan,
};
use crate::read_calls::Done;
use crate::scene::{Note, SceneJson};
use crate::scene_read::{Scene, read};

// ---- the calls' types ------------------------------------------------------------------

/// `platesShow`: the plate of a print list the view shows.
#[derive(Deserialize, Serialize, Debug, ts_rs::TS)]
#[ts(optional_fields = nullable)]
#[serde(deny_unknown_fields)]
pub struct PlatesShowReq {
    /// The print list's id.
    pub list: String,
    /// The plate in view, from 0.
    pub plate: usize,
}

/// `platesSet`: a word of the list's `plates` line, set or (null) taken off.
#[derive(Deserialize, Serialize, Debug, ts_rs::TS)]
#[ts(optional_fields = nullable)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PlatesSetReq {
    pub list: String,
    /// `printer`, `mode`, `group`, `gap`, `margin`, `turn`, `system` or `inventory`.
    pub key: String,
    /// The word; null takes it off.
    pub value: Option<String>,
    pub doc_rev: u64,
}

pub const PLATES_SHOW: Verb<PlatesShowReq, Done> = Verb::new("platesShow");
pub const PLATES_SET: Verb<PlatesSetReq, ListEdited> = Verb::new("platesSet");

/// What planning a list asks of whoever runs it, beyond what a map is read with ([`ListHost`]): the tile's state (patches
/// the owner takes), the builds, and the files' times.
pub trait PlatesHost: ListHost {
    /// Tell the owner something of the list `id` (the plates, a scene, its progress).
    fn update(&self, id: &str, patch: Patch);
    /// The plate the view of list `id` is on, from 0.
    fn plate_in_view(&self, id: &str) -> usize;
    /// Set the plate the view of list `id` is on.
    fn set_plate(&self, id: &str, plate: usize);
    /// The palette a mesh is painted with.
    fn palette(&self) -> dt_kit::Palette;
    /// The inventory's file (what the plan reads, whose change lays a list out again), where there is one.
    fn inventory_path(&self) -> Option<String>;
    /// The time of the last change of `path`, where the store keeps one.
    fn modified(&self, path: &str) -> Option<SystemTime>;
    /// Is the full build of the tile at `path` for `opts` already kept?
    fn full_kept(&self, path: &str, opts: &Options) -> bool;
    /// The tile at `path` built for `opts` (kept when the host has a cache).
    ///
    /// # Errors
    /// The tile's mistakes.
    fn build_piece(&self, path: &str, opts: &Options) -> Result<Built, Vec<Diag>>;
    /// A piece's low build, for a crown's look at it: none when it does not build or a newer reading of the map is on its
    /// way and will look again.
    fn crown_build(&self, path: &str, opts: Options) -> Option<Built>;
    /// The Bambu Studio template's project settings for `machine`, where one is reachable: `None` when there is
    /// none (the plates cannot be a Bambu project without it).
    ///
    /// # Errors
    /// A template that is found and cannot be read, or this host cannot read one at all (said in words).
    fn bambu_template(&self, machine: &dt_eval::Machine) -> Result<Option<String>, String>;
    /// Seconds, from any starting point that does not change (a build's length is the difference of two): the host's own
    /// clock, since a web page has no `Instant`.
    fn seconds(&self) -> f64;
    /// A test's hand on the moment between the files' times being taken and the files being read.
    fn after_seen(&self, _path: &str) {}
}

fn bad(msg: impl Into<String>) -> Refusal {
    Refusal::refused(Status::BadRequest, msg)
}

/// A piece's best build at hand: the full one when the cache has it, else the low one (built now, and kept).
struct Shown {
    /// The build, thinned for the view (a plate of a dozen full grass tiles is millions of triangles the page does not need
    /// to draw).
    built: Built,
    /// What the whole build covers: the layout is made from this, never from the thinned mesh.
    boxes: Boxes,
    full: bool,
}

/// The triangles a plate of `pieces` may show, over all of them.
const PLATE_BUDGET: usize = 900_000;

fn shown(
    host: &impl PlatesHost,
    plan: &Plan,
    k: usize,
    want_full: bool,
) -> Result<Shown, Vec<Diag>> {
    let opts = plan.options(k, &host.options(), &*host.profiles())?;
    let kind = &plan.kinds[k];
    let full = want_full || host.full_kept(&kind.path, &opts);
    let built = host.build_piece(
        &kind.path,
        &Options {
            tier: if full { Tier::Full } else { Tier::Low },
            ..opts
        },
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
#[expect(
    clippy::disallowed_methods,
    reason = "permitted silence: parsing where None is the meaning (a probe of a written word or an optional parameter)"
)]
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
        let base = dt_value::idx(mesh.positions.len());
        mesh.positions.extend(dt_print::stand_positions(
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
#[expect(
    clippy::disallowed_methods,
    reason = "permitted silence: a probe where no machine means no label or bed on the panel; the plan that reads the same printer reports the failure"
)]
fn written_panel(
    list: &dt_doc::map::PrintList,
    profiles: &dyn dt_eval::Profiles,
    writable: bool,
) -> PlatesPanel {
    let pl = &list.plates;
    let printer = pl.printer.as_ref().map_or_else(
        || dt_doc::map::DEFAULT_PRINTER.to_owned(),
        |p| p.value.clone(),
    );
    let machine = dt_eval::printer_machine_in(profiles, &printer).ok();
    PlatesPanel::Written(PlatesWritten {
        planned: false,
        writable,
        printer,
        label: machine.as_ref().map(dt_eval::Machine::label),
        bed: machine.as_ref().and_then(|m| m.bed),
        slots: machine.as_ref().map(dt_eval::Machine::slot_count),
        mode: pl.mode.as_ref().map(|m| m.value.word().to_owned()),
        group: pl.group.as_ref().map(|m| m.value.word().to_owned()),
        gap: pl.gap.as_ref().map(|m| m.value),
        margin: pl.margin.as_ref().map(|m| m.value),
        printers: printer_rows(profiles),
        check_inventory: pl.inventory.as_ref().is_none_or(|i| i.value),
        pieces: list_pieces(list),
    })
}

/// The list's own pieces, in the order written, for the panel to edit: the
/// count, and taking one out, are edits of the list's text.
pub fn list_pieces(list: &dt_doc::map::PrintList) -> Vec<ListPiece> {
    let ids = dt_doc::map::piece_ids(list);
    list.pieces
        .iter()
        .enumerate()
        .map(|(i, p)| ListPiece {
            tile: p.tile.value.clone(),
            count: p.count.value,
            // the id the piece has, or the one the first edit of the file will write on it
            id: ids[i].clone(),
            pinned: list
                .plates
                .pins
                .iter()
                .any(|q| p.id.as_ref().is_some_and(|i| i.value == q.id.value)),
            seed: p.seed.as_ref().map(|s| s.value),
            system: p.system.as_ref().map(|s| s.value),
            print: p.print.word().to_owned(),
        })
        .collect()
}

/// Every profile, for the choosers: brand, then model.  One with no bed
/// (`default`, `fine`) is only floors: it builds tiles but cannot hold plates.
/// A profile that does not read is still listed, with what is wrong with it,
/// so it is never missing without a word.
pub fn printer_rows(profiles: &dyn dt_eval::Profiles) -> Vec<PrinterRow> {
    profiles
        .names()
        .into_iter()
        .map(|(name, kind)| {
            let mine = kind == dt_eval::ProfileKind::Mine;
            match dt_eval::printer_machine_in(profiles, &name) {
                Ok(m) => PrinterRow::Read(PrinterRead {
                    label: m.label(),
                    slots: m.slot_count(),
                    zones: m.excluded.len(),
                    name,
                    brand: m.brand,
                    model: m.model,
                    bed: m.bed,
                    height: m.height,
                    verify: m.verify,
                    from_slicer: m.from_slicer,
                    mine,
                }),
                Err(e) => PrinterRow::Broken(PrinterBroken {
                    label: name.clone(),
                    name,
                    slots: 1,
                    mine,
                    broken: e
                        .first()
                        .map_or_else(|| "does not read".to_owned(), |d| d.message.clone()),
                }),
            }
        })
        .collect()
}

/// What the page's panel reads: the settings, the pieces and the plates.
#[expect(
    clippy::too_many_arguments,
    reason = "the flat inputs of one maths step, named at the call"
)]
fn plates_panel(
    plan: &Plan,
    layout: &Layout,
    boxes: &[Boxes],
    exact: &[bool],
    view: usize,
    writable: bool,
    list: &dt_doc::map::PrintList,
    profiles: &dyn dt_eval::Profiles,
) -> PlatesPlanned {
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
    let names = |v: &[Material]| v.iter().map(|m| m.name().to_owned()).collect::<Vec<_>>();
    // each laid-out piece names its list piece by id, as an edit does
    let ids = dt_doc::map::piece_ids(list);
    PlatesPlanned {
        planned: true,
        settings: PlanSettings::of(plan),
        kinds: plan
            .kinds
            .iter()
            .enumerate()
            .map(|(k, kind)| PlanKind {
                planned: PlannedKind::of(kind),
                size: boxes[k].size,
                height: boxes[k].height,
                exact: exact[k],
                filaments: names(if exact[k] {
                    &boxes[k].filaments
                } else {
                    &kind.estimate
                }),
            })
            .collect(),
        layout: PlanPlate::of_layout(plan, layout, &ids),
        view,
        exact: exact.iter().all(|e| *e),
        grouping_cost: layout.grouping_cost,
        slot_check: SlotCheck {
            need,
            have: s.machine.slot_count(),
            ok: s.mode == Mode::Single || need <= s.machine.slot_count() as usize,
        },
        writable,
        format: plates::export::Format::for_printer(&s.machine)
            .word()
            .to_owned(),
        printers: printer_rows(profiles),
        pieces: list_pieces(list),
    }
}

/// Why a list did not plan: the list, if it read, and the mistakes.
type Unplanned = (Option<dt_doc::map::PrintList>, Vec<Diag>);

/// A list's text as a list: every mistake at its text.
fn read_list(path: &str, text: &str) -> Result<dt_doc::map::PrintList, Vec<Diag>> {
    dt_doc::map::decode(path, text).map_err(|e| e.iter().map(Diag::from).collect())
}

/// What you have printed, for a list that looks in the inventory (the default; `plates inventory=ignore` and the command
/// line's `--all` do not): the shelf of the inventory, or `None` where there is no inventory yet or the list leaves it alone.
///
/// # Errors
/// An inventory that is there and has mistakes (each at its text): a list is never laid out against a log that is not
/// understood.
fn shelf_for(
    host: &impl PlatesHost,
    list: &dt_doc::map::PrintList,
    ov: &Overrides,
) -> Result<Option<dt_doc::inventory::Shelf>, Vec<Diag>> {
    let looks = ov
        .inventory
        .or_else(|| list.plates.inventory.as_ref().map(|i| i.value))
        .unwrap_or(true);
    if !looks {
        return Ok(None);
    }
    Ok(host.inventory()?.map(|i| dt_doc::inventory::Shelf::of(&i)))
}

/// A list's name: its file's stem.
fn title_of(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .map_or_else(|| "plates".to_owned(), |s| s.to_string_lossy().into_owned())
}

/// The list's text planned: the list and its plan, or why it does not.
pub fn plan_from(
    host: &impl PlatesHost,
    path: &str,
    text: &str,
    ov: &Overrides,
) -> Result<(dt_doc::map::PrintList, Plan), Box<Unplanned>> {
    let list = read_list(path, text).map_err(|e| Box::new((None, e)))?;
    let base = host.options();
    let dir = Path::new(path).parent();
    let shelf = shelf_for(host, &list, ov).map_err(|e| Box::new((Some(list.clone()), e)))?;
    let home = host.tiles_home();
    let profiles = host.profiles();
    let ctx = Context {
        sources: host.tile_source(),
        list_dir: dir,
        home: home.as_deref(),
        profiles: &*profiles,
        base: &base,
        shelf: shelf.as_ref(),
    };
    match plates::plan(&list, &title_of(path), &ctx, ov) {
        Ok(p) => Ok((list, p)),
        Err(e) => Err(Box::new((Some(list), e))),
    }
}

fn say(host: &impl PlatesHost, id: &str, scope: Scope, make: impl FnOnce(&str) -> Vec<Message>) {
    host.report(id, scope, make);
}

/// The notes of a map as messages: its crowns over taller neighbours, and a piece's own mistakes.
fn scene_messages(notes: &[Note], diags: &[Diag], id: &str, name: &str) -> Vec<Message> {
    let mut out = crate::tile_calls::of_errors(diags, Scope::Check, id, name, &[]);
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

#[expect(
    clippy::disallowed_methods,
    reason = "permitted silence: a probe where None means no scene yet; rebuild_plates reads the same text and reports why it does not read"
)]
pub fn rebuild(host: &impl PlatesHost, id: &str, path: &str) {
    let profiles = host.profiles();
    let profiles: &dyn dt_eval::Profiles = &*profiles;
    // the times of the files this plan will be of, taken *before* they are read: a file that changes while the plan is
    // made is a change the worker sees afterwards, never one this plan is taken to have included
    let mut seen = BTreeMap::from([(path.to_owned(), host.modified(path))]);
    // the inventory decides what prints: the list is laid out again when it changes
    if let Some(inv) = host.inventory_path() {
        let at = host.modified(&inv);
        seen.insert(inv, at);
    }
    host.after_seen(path);
    // one read: the revision, the scene and the plan are all of this text (a read that
    // failed is reported by the plan, below)
    // (and the file's revision, read with it: this plan and scene are of that revision)
    let (text, rev) = match host.docs().read(path) {
        Ok((t, r)) => (Ok(t), r),
        Err(e) => (
            Err(vec![Diag::plain(
                dt_lang::codes::SOURCE,
                format!("cannot read {path}: {e}"),
            )]),
            0,
        ),
    };
    let list = text.as_ref().ok().and_then(|t| read_list(path, t).ok());
    let scene = list.map(|list| {
        let mut sc = read(host, path, &list, rev);
        // the scene reaches the page now, before anything is built: a map shows its pieces
        // (as boxes, then their meshes) while its plates are still being laid out
        let json = sc.json.clone();
        let ground = unplanned_ground(host);
        let panel = written_panel(&list, profiles, true);
        host.update(
            id,
            Patch::PlatesScene {
                doc: Box::new(PlatesDoc {
                    ground,
                    writable: true,
                    plates: panel,
                    scene: Some(json),
                    doc_rev: rev,
                }),
            },
        );
        if sc.pieces.iter().filter(|p| p.at.is_some()).count() > 1 {
            sc.notes = crowns(host, &sc);
        }
        (list, sc)
    });
    rebuild_plates(
        host,
        id,
        path,
        &text,
        rev,
        scene.as_ref().map(|(_, s)| s),
        seen,
    );
}

#[expect(
    clippy::disallowed_methods,
    reason = "permitted silence: a best-effort filesystem probe (a missing or unreadable file means none yet, read again on the next call)"
)]
fn rebuild_plates(
    host: &impl PlatesHost,
    id: &str,
    path: &str,
    text: &Result<String, Vec<Diag>>,
    rev: u64,
    scene: Option<&Scene>,
    seen: BTreeMap<String, Option<SystemTime>>,
) {
    let profiles = host.profiles();
    let profiles: &dyn dt_eval::Profiles = &*profiles;
    let t0 = host.seconds();
    let scene_json = scene.map(|s| s.json.clone());
    let kept: Vec<Note> = scene.map_or_else(Vec::new, |s| s.notes.clone());
    host.update(id, Patch::SceneNotes(kept));
    let no_notes = (Vec::new(), Vec::new());
    let (notes, diags) = scene.map_or(no_notes, |s| (s.notes.clone(), s.diags.clone()));
    let is_map = scene.is_some_and(|s| s.json.map);
    let files = seen;
    let writable = host
        .storage()
        .stat(Path::new(path))
        .is_ok_and(|m| !m.readonly);
    let view = host.plate_in_view(id);
    let fail_with = |list: Option<&dt_doc::map::PrintList>, errs: Vec<Diag>, scope: Scope| {
        // a map whose every piece is on your shelf has nothing to print: that is a fact about
        // the plates, not a mistake in the map
        let calm = is_map && errs.len() == 1 && errs[0].code == dt_lang::codes::INVENTORY;
        say(host, id, scope, |name| {
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
                let mut out = crate::tile_calls::of_errors(&errs, scope, id, name, &[]);
                out.extend(scene_messages(&notes, &diags, id, name));
                out
            }
        });
        let loose = list.map_or_else(
            || {
                PlatesPanel::Unread(PlatesUnread {
                    planned: false,
                    writable,
                    printers: printer_rows(profiles),
                })
            },
            |l| written_panel(l, profiles, writable),
        );
        let (files, scene_json) = (files.clone(), scene_json.clone());
        let mistakes = dt_eval::mistakes(&errs);
        let ground = unplanned_ground(host);
        host.update(
            id,
            Patch::PlatesFailed {
                calm,
                errors: mistakes,
                files,
                doc: Box::new(PlatesDoc {
                    ground,
                    writable,
                    plates: loose,
                    scene: scene_json,
                    doc_rev: rev,
                }),
            },
        );
    };
    let planned = match text {
        Ok(t) => plan_from(host, path, t, &Overrides::default()),
        Err(e) => Err(Box::new((None, e.clone()))),
    };
    let (list, plan) = match planned {
        Ok(x) => x,
        Err(b) => return fail_with(b.0.as_ref(), b.1, Scope::Check),
    };
    let s = &plan.settings;
    say(host, id, Scope::Check, |name| {
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
                .code(dt_lang::codes::PIN),
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
                .code(dt_lang::codes::VERIFY),
            );
        }
        out
    });
    // each piece's best build at hand: the full one if it is kept, else the low one
    let mut parts: Vec<Shown> = Vec::new();
    for k in 0..plan.kinds.len() {
        match shown(host, &plan, k, false) {
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
    let palette = host.palette();
    let push =
        |layout: &Layout, boxes: &[Boxes], parts: &[Shown], running: bool, note: Option<&str>| {
            let (mesh, painted, clean, owner) = plate_mesh(layout, parts, view, &palette);
            let exact: Vec<bool> = parts.iter().map(|p| p.full).collect();
            let sp = shared_plan(
                &plan,
                &list,
                layout,
                boxes,
                &exact,
                view,
                writable,
                profiles,
                scene_json.clone(),
                rev,
            );
            let mut meta = MeshHeader::new(MeshKind::Plates);
            meta.overhang_deg = Some(host.options().printer.overhang_deg);
            meta.footprint = s.machine.bed;
            meta.bed = s.machine.bed;
            meta.excluded = Some(s.machine.excluded.clone());
            meta.plate = Some(view);
            meta.plates = Some(layout.plates.len());
            meta.tier = Some(
                if exact.iter().all(|e| *e) {
                    "full"
                } else if exact.iter().any(|e| *e) {
                    "mixed"
                } else {
                    "low"
                }
                .to_owned(),
            );
            let zero = vec![0u16; owner.len()];
            match payload::encode_traced(&mesh, &painted, &clean, &owner, &zero, meta) {
                Ok(bytes) => {
                    let files = files.clone();
                    let note = note.unwrap_or("draft meshes, not yet checked").to_owned();
                    host.update(
                        id,
                        Patch::PlatesReady {
                            plan: sp,
                            bytes: Arc::new(bytes),
                            rev,
                            files,
                            shown: running.then(|| crate::api::Shown {
                                kind: "preview".to_owned(),
                                note,
                                checked: false,
                                tier: None,
                            }),
                        },
                    );
                }
                Err(e) => {
                    let d = Diag::plain(
                        dt_lang::codes::VIEW,
                        format!("cannot prepare the view of {path}: {e}"),
                    );
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
    say(host, id, Scope::Build, |name| {
        let low = parts.iter().filter(|p| !p.full).count();
        let mut out = vec![
            Message::new(
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
                        format!(
                            ": draft meshes for {low} tile{}, not yet checked",
                            if low == 1 { "" } else { "s" }
                        )
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
            .data(MessageData::Layout(LayoutData {
                low,
                plates: layout.plates.len(),
                build_s: (host.seconds() - t0),
            })),
        ];
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
                .code(dt_lang::codes::PIN),
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
                .code(dt_lang::codes::GROUPING)
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
        let progress = Progress {
            what: "building".to_owned(),
            fraction: n as f64 / todo.len() as f64,
            label: format!("building {} in full", plan.kinds[k].name),
            key: "plates".to_owned(),
            count: Some((format!("{} of {}", n + 1, todo.len()), 0, 0)),
            elapsed_s: (host.seconds() - t0),
            eta_s: None,
            estimate_s: None,
        };
        host.update(id, Patch::Progress(progress));
        match shown(host, &plan, k, true) {
            Ok(p) => {
                let opts = match plan.options(k, &host.options(), profiles) {
                    Ok(o) => o,
                    Err(e) => return fail_with(Some(&list), e, Scope::Build),
                };
                let f = p.built.verdict(&opts.printer).reasons().to_vec();
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
        say(host, id, Scope::Build, |name| {
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
                    (host.seconds() - t0),
                    if moved {
                        "; the pieces cover a little more than the draft meshes did, so the plates were laid out again"
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
                    .code(dt_lang::codes::GATE),
                );
            }
            out
        });
    }
    host.update(id, Patch::ClearProgressAndShown);
}

/// The page's plan for a finished layout.
#[expect(
    clippy::too_many_arguments,
    reason = "the flat inputs of one maths step, named at the call"
)]
fn shared_plan(
    plan: &Plan,
    list: &dt_doc::map::PrintList,
    layout: &Layout,
    boxes: &[Boxes],
    exact: &[bool],
    view: usize,
    writable: bool,
    profiles: &dyn dt_eval::Profiles,
    scene: Option<SceneJson>,
    doc_rev: u64,
) -> TilePlan {
    TilePlan::Plates(Box::new(PlatesDoc {
        ground: PlanGround {
            square_mm: dt_kit::Frame::for_system(plan.settings.system, 1, 1).square_mm,
            size_mm: plan.settings.machine.bed,
            datum_z: Some(0.0),
        },
        writable,
        plates: PlatesPanel::Planned(Box::new(plates_panel(
            plan, layout, boxes, exact, view, writable, list, profiles,
        ))),
        scene,
        doc_rev,
    }))
}

/// The ground of a list that has no layout: the system's square, and no bed (no printer is read).
fn unplanned_ground(host: &impl PlatesHost) -> PlanGround {
    PlanGround {
        square_mm: dt_kit::Frame::for_system(host.options().system, 1, 1).square_mm,
        size_mm: None,
        datum_z: Some(0.0),
    }
}

/// The plate in view: set, and the list laid out again.
///
/// # Errors
/// [`Refusal`] (not found): there is no such print list.
pub fn show(host: &impl PlatesHost, v: PlatesShowReq) -> Result<Done, Refusal> {
    list_path(host, &v.list)?;
    host.set_plate(&v.list, v.plate);
    again(host, &v.list);
    Ok(Done {})
}

/// Build the list again: its plate in view or its file changed.
pub fn again(host: &impl PlatesHost, id: &str) {
    // building from now: a plan shown as ready while a newer one is due would be read as the answer
    host.update(id, Patch::MarkStale);
}

/// `text` with one word of the list's `plates` line set (null takes it off), the rest of it as it is.
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
            // the entry of that name goes, and no other (`KdlNode::remove` by a name does not find it)
            node.entries_mut()
                .retain(|e| e.name().is_none_or(|n| n.value() != key));
        }
    }
    Ok(doc.to_string())
}

/// `platesSet`: change a word of the `plates` line (the printer, the mode, the grouping, the gap, the margin, turns, the
/// system); the file is the document, so it is rewritten and the list builds again.  A value of null removes the word.
///
/// # Errors
/// [`Refusal`]: no such list, a word that is not a setting or does not read, or a file that moved on.
pub fn set(host: &impl PlatesHost, v: PlatesSetReq) -> Result<ListEdited, Refusal> {
    let path = list_path(host, &v.list)?;
    let id = v.list.as_str();
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
        && e.iter().any(|d| d.code != dt_lang::codes::UNKNOWN_NODE)
    {
        return Err(bad(&e[0].message));
    }
    let r = host.docs().edit(&path, Some(v.doc_rev), |text| {
        with_word(&path, text, key, value)
            .map(|t| (t, ()))
            .map_err(|e| Refusal::failure(Status::Internal, e))
    });
    match r {
        Ok((e, ())) => {
            if key == "printer" {
                // another bed: back to the first plate (the printer itself is the document's word, which the answer's scene
                // carries: the chip and the status line read it there)
                host.update(id, Patch::PlateZero);
            }
            again(host, id);
            Ok(done(host, &path, &e, None))
        }
        Err(e) => Err(with_current(host, &path, e.into_refusal())),
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
pub fn crowns(host: &impl PlatesHost, scene: &Scene) -> Vec<Note> {
    let base = host.options();
    let mut shapes: BTreeMap<(String, u64, dt_lang::System), Option<Shape>> = BTreeMap::new();
    let placed: Vec<&crate::scene_read::PieceInfo> = scene
        .pieces
        .iter()
        .filter(|p| p.at.is_some() && p.error.is_none())
        .collect();
    for p in &placed {
        let key = (p.path.clone(), p.seed_used, p.system);
        if shapes.contains_key(&key) {
            continue;
        }
        let Some(at) = p.at else { continue };
        let mut opts = Options {
            system: p.system,
            seed: p.seed,
            ..base.clone()
        };
        if opts
            .use_printer_in(&scene.printer, &*host.profiles())
            .is_err()
        {
            shapes.insert(key, None);
            continue;
        }
        // a piece's low build: the program does it on its tier lane, like the page's own look at it
        let built = host.crown_build(
            &p.path,
            Options {
                tier: Tier::Low,
                ..opts
            },
        );
        shapes.insert(key, built.map(|b| shape_of(&b.export.mesh, at.w, at.d)));
    }
    let mut notes = Vec::new();
    for a in &placed {
        let (Some(pa), Some(Some(sa))) =
            (a.at, shapes.get(&(a.path.clone(), a.seed_used, a.system)))
        else {
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
            let (Some(pb), Some(Some(sb))) =
                (b.at, shapes.get(&(b.path.clone(), b.seed_used, b.system)))
            else {
                continue;
            };
            let f = pb.footprint();
            let (ox, oy) = (
                reach[2].min(f[2]) - reach[0].max(f[0]),
                reach[3].min(f[3]) - reach[1].max(f[1]),
            );
            let top = pb.z + sb.top;
            if ox > 0.5 && oy > 0.5 && top > crown_z + 0.5 {
                let who = |p: &crate::scene_read::PieceInfo| {
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
                    code: dt_lang::codes::CROWN,
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
    use super::with_word;

    #[test]
    fn a_word_is_set_and_taken_off_and_the_rest_stays() {
        let text = "dharmatiles version=\"1.0.0\"\n\nmap {\n    about \"x\"\n}\n\nplates printer=\"bambu/x1c\" system=db\n";
        let set = with_word("a.dmap", text, "gap", Some("3mm")).expect("a word is set");
        assert!(
            set.contains("gap=\"3mm\"") && set.contains("printer=\"bambu/x1c\""),
            "{set}"
        );
        let off = with_word("a.dmap", &set, "printer", None).expect("a word is taken off");
        assert!(
            !off.contains("printer") && off.contains("system=db") && off.contains("gap=\"3mm\""),
            "{off}"
        );
    }
}
