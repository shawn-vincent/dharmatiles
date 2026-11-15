//! The inventory in the editor: the snapshot carries it, "I have this", the
//! sheet after a print, gone, a tile that moved, a reprint into a print list,
//! and what the plates make of it.  A scratch home of the test's own.

use std::fmt::Debug;

use super::plates::plates_planned;
use super::*;
use crate::inventory_wire::{ColourWord, InventoryRow, InventoryState};
use crate::preview::inventory as calls_inventory;
use crate::preview::placing::Near;
use crate::preview::resources::Resource;
use crate::preview::scene::{SceneJson, ScenePiece};
use crate::preview::{lists, maps, places, tiers};

struct Home {
    dir: std::path::PathBuf,
    shared: Arc<Shared>,
    r: Running,
    token: String,
}

fn start_home(name: &str, files: &[(&str, &str)]) -> Home {
    let dir = std::env::temp_dir().join(format!("dt-inv-view-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for (f, text) in files {
        std::fs::write(dir.join(f), text).unwrap();
    }
    let roots =
        vec![Root::new("My tiles", RootKind::Dir(dir.clone())).with_role(roots::Role::Home)];
    let shared =
        Arc::new(Shared::with_roots(roots, &[], &Options::default()).with_home(dir.clone()));
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let addr = listener.local_addr().unwrap();
    server::spawn(listener, Arc::clone(&shared));
    let stop = Arc::new(AtomicBool::new(false));
    let (s, st) = (Arc::clone(&shared), Arc::clone(&stop));
    std::thread::spawn(move || work(&s, true, &st));
    let r = Running { addr, stop };
    let token = session_token(&r);
    Home {
        dir,
        shared,
        r,
        token,
    }
}

impl Home {
    fn path(&self, f: &str) -> String {
        self.dir.join(f).to_string_lossy().into_owned()
    }

    fn id(&self, f: &str) -> String {
        tile_id(&self.path(f))
    }

    /// A `POST` of a request type, with the token: the status line and the route's response type.
    fn call<Q: Serialize, T: DeserializeOwned>(&self, route: &str, body: &Q) -> (String, T) {
        post(&self.r, route, Some(&self.token), None, body)
    }

    /// A `POST` that fails or is refused: the status line and the error body.
    fn refused<Q: Serialize>(&self, route: &str, body: &Q) -> (String, ErrorBody) {
        post_err(&self.r, route, Some(&self.token), None, body)
    }

    /// A `POST` of which only the status line matters.
    fn head<Q: Serialize>(&self, route: &str, body: &Q) -> String {
        post_head(&self.r, route, Some(&self.token), None, body)
    }

    /// The revision the program holds for the document `id` (what the page names its edits against).
    fn rev(&self, id: &str) -> u64 {
        read::<route::TextRes>(&get(&self.r, &format!("/api/tiles/{id}/text")).1).doc_rev
    }

    /// An edit that is refused: the request made against the revision the program holds for `list` now; the
    /// status line and the error body.
    fn edit_refused<Q: Serialize>(
        &self,
        route: &str,
        list: &str,
        make: impl FnOnce(u64) -> Q,
    ) -> (String, ErrorBody) {
        self.refused(route, &make(self.rev(list)))
    }

    /// A call that must answer 200: its response type.
    fn ok<Q: Serialize, T: DeserializeOwned + Debug>(&self, route: &str, body: &Q) -> T {
        let (head, v) = self.call(route, body);
        assert!(head.starts_with("HTTP/1.1 200"), "{route}: {head} {v:?}");
        v
    }

    /// An edit as the page sends it, made against the revision the program holds for `list` now, that must answer 200.
    fn edited<Q: Serialize, T: DeserializeOwned + Debug>(
        &self,
        route: &str,
        list: &str,
        make: impl FnOnce(u64) -> Q,
    ) -> T {
        self.ok(route, &make(self.rev(list)))
    }

    fn snapshot(&self) -> Snapshot {
        tiles(&self.r)
    }

    /// The snapshot's inventory, as its type.
    fn inventory(&self) -> InventoryState {
        self.snapshot().inventory
    }

    /// The first piece the snapshot's inventory holds.
    fn first_piece(&self) -> InventoryRow {
        self.inventory().pieces()[0].clone()
    }

    fn tile(&self, f: &str) -> Option<TileSummary> {
        let id = self.id(f);
        self.snapshot().tiles.into_iter().find(|t| t.id == id)
    }

    fn wait(&self, f: &str, what: &str, mut ok: impl FnMut(&TileSummary) -> bool) -> TileSummary {
        let t0 = Instant::now();
        loop {
            // a folder's tiles build when they are chosen
            self.shared.want(&self.id(f));
            if let Some(t) = self.tile(f)
                && ok(&t)
            {
                return t;
            }
            assert!(
                t0.elapsed() < Duration::from_secs(120),
                "timed out waiting for {what}: {:?}",
                self.snapshot()
            );
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    fn text(&self) -> String {
        std::fs::read_to_string(self.dir.join("inventory.dinv")).unwrap()
    }

    /// "I have this" for the file `f`, in `colour`, on the day `on`.
    fn have_req(
        &self,
        f: &str,
        colour: ColourWord,
        on: Option<&str>,
    ) -> calls_inventory::InventoryHaveReq {
        calls_inventory::InventoryHaveReq {
            tile: self.id(f),
            colour,
            system: None,
            seed: None,
            count: None,
            on: on.map(str::to_owned),
        }
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// A map's scene, once its document plans.
fn scene(t: &TileSummary) -> Option<&SceneJson> {
    match &t.plan {
        Some(TilePlan::Plates(d)) => d.scene.as_ref(),
        _ => None,
    }
}

/// The scene has `n` pieces.
fn has_pieces(t: &TileSummary, n: usize) -> bool {
    scene(t).is_some_and(|s| s.pieces.len() == n)
}

/// The piece named `id` in a scene.
fn piece_of<'a>(s: &'a SceneJson, id: &str) -> &'a ScenePiece {
    s.pieces
        .iter()
        .find(|p| p.id == id)
        .unwrap_or_else(|| panic!("no piece {id} in the scene"))
}

/// A piece of the inventory as the calls name it (the glade, seed 5, DungeonBlocks).
fn gone_req(tile: &str, colour: ColourWord) -> calls_inventory::InventoryGoneReq {
    calls_inventory::InventoryGoneReq {
        tile: tile.to_owned(),
        seed: 5,
        system: dt_lang::System::Db,
        colour,
        note: None,
        on: None,
    }
}

fn reprint_req(colour: ColourWord, list: Option<String>) -> calls_inventory::InventoryReprintReq {
    calls_inventory::InventoryReprintReq {
        tile: "home:clearing".to_owned(),
        seed: 5,
        system: dt_lang::System::Db,
        colour,
        list,
        doc_rev: None,
    }
}

fn new_list(name: &str, map: Option<bool>) -> lists::ListsNewReq {
    lists::ListsNewReq {
        name: Some(name.to_owned()),
        root: None,
        dir: None,
        map,
    }
}

fn add_req(tile: &str, list: Option<&str>) -> lists::ListsAddReq {
    lists::ListsAddReq {
        tile: tile.to_owned(),
        list: list.map(str::to_owned),
        seed: None,
        system: None,
        doc_rev: None,
    }
}

fn piece_req(list: &str, id: &str, doc_rev: u64) -> maps::MapsPieceReq {
    maps::MapsPieceReq {
        list: list.to_owned(),
        id: id.to_owned(),
        doc_rev,
    }
}

fn set_req(list: &str, id: &str, set: maps::PieceWords, doc_rev: u64) -> maps::MapsSetReq {
    maps::MapsSetReq {
        list: list.to_owned(),
        id: id.to_owned(),
        set,
        doc_rev,
    }
}

fn edit_of(id: &str, set: maps::PieceWords) -> maps::PieceEdit {
    maps::PieceEdit {
        id: id.to_owned(),
        set,
    }
}

fn add_piece_req(list: &str, tile: &str, doc_rev: u64) -> maps::MapsAddReq {
    maps::MapsAddReq {
        list: list.to_owned(),
        tile: tile.to_owned(),
        at: None,
        near: None,
        rot: None,
        seed: None,
        seed_word: None,
        system: None,
        doc_rev,
    }
}

/// The words that turn a piece to `rot`.
fn rot_words(rot: i64) -> maps::PieceWords {
    maps::PieceWords {
        rot: Some(Some(rot)),
        ..Default::default()
    }
}

fn folder_new_req(name: &str) -> places::FolderNamedReq {
    places::FolderNamedReq {
        root: 0,
        dir: String::new(),
        name: name.to_owned(),
    }
}

const GLADE: &str =
    "dharmatiles version=1\ntile size=\"1x1\" seed=5 {\n    stack {\n        dirt\n    }\n}\n";
const KIT: &str = "dharmatiles version=1\nmap {\n    piece \"home:glade\" count=3\n}\nplates printer=\"sovol/sv06\" mode=\"single\"\n";

#[test]
fn i_have_this_makes_the_file_and_the_snapshot_and_the_pills_follow() {
    let h = start_home("have", &[("glade.dtile", GLADE)]);
    // no inventory yet: nothing is made
    assert!(matches!(h.inventory(), InventoryState::Absent { .. }));
    assert!(!h.dir.join("inventory.dinv").exists());
    // a call needs the token
    let body = h.have_req("glade.dtile", ColourWord::Single, None);
    assert!(post_head(&h.r, "/api/inventory/have", None, None, &body).starts_with("HTTP/1.1 403"));
    // a bad colour is refused, nothing written (a word no type can say, so the body is the wire's own)
    let raw = format!(r#"{{"tile":{:?},"colour":"rainbow"}}"#, h.id("glade.dtile"));
    let (head, _) = post_raw(&h.r, "/api/inventory/have", Some(&h.token), None, &raw);
    assert!(head.starts_with("HTTP/1.1 400"), "{head}");
    assert!(!h.dir.join("inventory.dinv").exists());
    // a print list or the inventory is not a tile
    let v: calls_inventory::InventoryWrote = h.ok(
        "/api/inventory/have",
        &h.have_req("glade.dtile", ColourWord::Multi, Some("2026-11-12")),
    );
    assert_eq!(v.lines.len(), 1);
    let text = h.text();
    assert!(
        text.contains(
            "print \"home:glade\" seed=5 system=db colour=multi result=ok on=\"2026-11-12\" hash=\""
        ),
        "{text}"
    );
    // the snapshot: one piece, held, at the tile's own seed
    let inv = h.inventory();
    let InventoryState::Read { last_colour, .. } = &inv else {
        panic!("the inventory reads: {inv:?}");
    };
    let piece = &inv.pieces()[0];
    assert_eq!(piece.tile, "home:glade");
    assert_eq!((piece.seed, piece.have), (5, 1));
    assert!(!piece.edited);
    assert_eq!(
        *last_colour,
        ColourWord::Multi,
        "the colour the log last used"
    );
    // the file is a tile of My tiles now, opened as an inventory
    let t = h.wait("inventory.dinv", "the inventory document", |t| {
        t.status == "ready" && matches!(t.plan, Some(TilePlan::Inventory(_)))
    });
    let Some(TilePlan::Inventory(doc)) = &t.plan else {
        panic!("not an inventory's plan: {:?}", t.plan);
    };
    assert_eq!(doc.inventory.pieces[0].have, 1);
    assert!(doc.inventory.text.contains("home:glade"));
    // the tile edited since it was printed: information only, it still counts
    std::fs::write(h.dir.join("glade.dtile"), format!("{GLADE}// edited\n")).unwrap();
    h.shared.rescan();
    let piece = h.first_piece();
    assert!(piece.edited);
    assert_eq!(piece.have, 1);
    // a second print of the same piece is a second piece; a mistake in the file is never added to
    let _: calls_inventory::InventoryWrote = h.ok(
        "/api/inventory/have",
        &h.have_req("glade.dtile", ColourWord::Multi, None),
    );
    assert_eq!(h.first_piece().have, 2);
    let good = h.text();
    std::fs::write(
        h.dir.join("inventory.dinv"),
        good.replace("seed=5", "seed=\"x\""),
    )
    .unwrap();
    let (head, v) = h.refused(
        "/api/inventory/have",
        &h.have_req("glade.dtile", ColourWord::Multi, None),
    );
    assert!(head.starts_with("HTTP/1.1 422"), "{head} {v:?}");
    let errors = v.errors.as_ref().expect("the file's mistakes");
    assert!(
        errors[0].message.contains("`seed` is a whole number"),
        "{v:?}"
    );
    assert_eq!(h.text().matches("print ").count(), 2, "not a line added");
    // and the stream says so, loudly
    let (_, body) = get(&h.r, "/api/messages");
    let m = String::from_utf8_lossy(&body).into_owned();
    assert!(m.contains("Could not add to the inventory"), "{m}");
}

#[test]
fn the_sheet_after_a_print_adds_the_plate_good_and_failed_and_the_plates_shrink() {
    let h = start_home("sheet", &[("glade.dtile", GLADE), ("kit.dmap", KIT)]);
    let t = h.wait("kit.dmap", "the plates", |t| {
        t.status == "ready" && plates_planned(t).is_some()
    });
    let p = plates_planned(&t).unwrap();
    assert_eq!(p.kinds[0].planned.count, 3);
    assert_eq!(p.kinds[0].planned.canon, "home:glade");
    assert_eq!(p.kinds[0].planned.seed_used, 5);
    assert_eq!(p.layout[0].pieces.len(), 3);
    // all three come off plate 1, the second one failed
    let kit = h.id("kit.dmap");
    let v: calls_inventory::InventoryWrote = h.ok(
        "/api/inventory/record",
        &calls_inventory::InventoryRecordReq {
            list: kit.clone(),
            plate: 1,
            failed: Some(vec![1]),
            on: Some("2026-11-13".to_owned()),
        },
    );
    assert_eq!(v.lines.len(), 3);
    let text = h.text();
    assert_eq!(text.matches("result=ok").count(), 2, "{text}");
    assert_eq!(text.matches("result=failed").count(), 1, "{text}");
    assert!(
        text.contains("from=\"kit/plate-1\"") && text.contains("on=\"2026-11-13\""),
        "{text}"
    );
    assert!(
        text.contains("colour=single"),
        "the list prints in single colour: {text}"
    );
    let piece = h.first_piece();
    assert_eq!((piece.have, piece.failed), (2, 1));
    // the list is laid out again against the shelf: one to print, two in the inventory
    let t = h.wait("kit.dmap", "the plates against the shelf", |t| {
        t.status == "ready" && plates_planned(t).is_some_and(|p| p.kinds[0].planned.have == 2)
    });
    let p = plates_planned(&t).unwrap();
    assert_eq!((p.kinds[0].planned.want, p.kinds[0].planned.count), (3, 1));
    assert_eq!(p.layout[0].pieces.len(), 1);
    // the switch: everything prints again, written in the list's plates line
    let _: lists::ListEdited = h.edited("/api/plates/set", &kit, |doc_rev| {
        crate::preview::plates::PlatesSetReq {
            list: kit.clone(),
            key: "inventory".to_owned(),
            value: Some("ignore".to_owned()),
            doc_rev,
        }
    });
    let t = h.wait("kit.dmap", "every piece", |t| {
        t.status == "ready" && plates_planned(t).is_some_and(|p| p.kinds[0].planned.count == 3)
    });
    assert!(!plates_planned(&t).unwrap().settings.check_inventory);
    let kit_text = std::fs::read_to_string(h.dir.join("kit.dmap")).unwrap();
    assert!(
        kit_text.contains("inventory=ignore") || kit_text.contains("inventory=\"ignore\""),
        "{kit_text}"
    );
    // a plate that is not there, a piece that is not on it, and a list that is not one are refused
    for (list, plate, failed) in [
        (kit.clone(), 9, None),
        (kit.clone(), 1, Some(vec![7])),
        (h.id("glade.dtile"), 1, None),
    ] {
        let head = h.head(
            "/api/inventory/record",
            &calls_inventory::InventoryRecordReq {
                list,
                plate,
                failed,
                on: None,
            },
        );
        assert!(
            head.starts_with("HTTP/1.1 400") || head.starts_with("HTTP/1.1 404"),
            "{head}"
        );
    }
    assert_eq!(
        h.text().matches("print ").count(),
        3,
        "nothing more was written"
    );
}

#[test]
fn gone_relink_rename_and_reprint_are_lines_added_never_edits() {
    let h = start_home("moves", &[("glade.dtile", GLADE), ("kit.dmap", KIT)]);
    let _: calls_inventory::InventoryWrote = h.ok(
        "/api/inventory/have",
        &calls_inventory::InventoryHaveReq {
            count: Some(2),
            ..h.have_req("glade.dtile", ColourWord::Single, Some("2026-11-09"))
        },
    );
    let before = h.text();
    // gone: one of the piece; none to take away is refused
    let _: calls_inventory::InventoryWrote = h.ok(
        "/api/inventory/gone",
        &calls_inventory::InventoryGoneReq {
            on: Some("2026-11-10".to_owned()),
            note: Some("dropped on the floor".to_owned()),
            ..gone_req("home:glade", ColourWord::Single)
        },
    );
    let piece = h.first_piece();
    assert_eq!((piece.have, piece.gone), (1, 1));
    let after = h.text();
    assert!(
        after.starts_with(before.trim_end().trim_end_matches('}')),
        "every earlier byte kept"
    );
    assert!(after.contains("gone \"home:glade\" seed=5 system=db colour=single on=\"2026-11-10\" note=\"dropped on the floor\""), "{after}");
    assert!(
        h.head(
            "/api/inventory/gone",
            &gone_req("home:glade", ColourWord::Multi)
        )
        .starts_with("HTTP/1.1 409")
    );
    // renaming the tile in the editor adds a renamed line, and the count follows
    let (head, _): (String, places::Listed) = h.call(
        &format!("/api/tiles/{}/rename", h.id("glade.dtile")),
        &edit::RenameReq {
            name: "meadow".to_owned(),
        },
    );
    assert!(head.starts_with("HTTP/1.1 200"), "{head}");
    assert!(
        h.text()
            .contains("renamed \"home:glade\" to=\"home:meadow\""),
        "{}",
        h.text()
    );
    let piece = h.first_piece();
    assert_eq!((piece.tile.as_str(), piece.have), ("home:meadow", 1));
    assert!(!piece.missing);
    // a tile moved into a folder follows too (the move route)
    std::fs::create_dir_all(h.dir.join("sets")).unwrap();
    h.shared.rescan();
    let (head, _): (String, places::ListedIn) = h.call(
        &format!("/api/tiles/{}/move", h.id("meadow.dtile")),
        &places::TileMoveReq {
            root: 0,
            dir: "sets".to_owned(),
        },
    );
    assert!(head.starts_with("HTTP/1.1 200"), "{head}");
    assert_eq!(h.first_piece().tile, "home:sets/meadow");
    // moved by hand, outside the editor: the path is not there any more, and one click re-links it
    std::fs::rename(
        h.dir.join("sets/meadow.dtile"),
        h.dir.join("clearing.dtile"),
    )
    .unwrap();
    h.shared.rescan();
    assert!(h.first_piece().missing);
    let _: calls_inventory::InventoryWrote = h.ok(
        "/api/inventory/relink",
        &calls_inventory::InventoryRelinkReq {
            from: "home:sets/meadow".to_owned(),
            to: h.id("clearing.dtile"),
            on: Some("2026-11-11".to_owned()),
        },
    );
    let piece = h.first_piece();
    assert_eq!(
        (piece.tile.as_str(), piece.missing),
        ("home:clearing", false)
    );
    assert!(
        h.text()
            .contains("renamed \"home:sets/meadow\" to=\"home:clearing\" on=\"2026-11-11\""),
        "{}",
        h.text()
    );
    assert!(
        h.head(
            "/api/inventory/relink",
            &calls_inventory::InventoryRelinkReq {
                from: "home:never".to_owned(),
                to: h.id("clearing.dtile"),
                on: None,
            }
        )
        .starts_with("HTTP/1.1 409")
    );
    // renaming a folder carries the tiles in it
    std::fs::create_dir_all(h.dir.join("old")).unwrap();
    std::fs::write(h.dir.join("old/pond.dtile"), GLADE).unwrap();
    h.shared.rescan();
    let _: calls_inventory::InventoryWrote = h.ok(
        "/api/inventory/have",
        &h.have_req("old/pond.dtile", ColourWord::Single, None),
    );
    let (head, _): (String, places::FolderDone) = h.call(
        "/api/folders/rename",
        &places::FolderNamedReq {
            root: 0,
            dir: "old".to_owned(),
            name: "new".to_owned(),
        },
    );
    assert!(head.starts_with("HTTP/1.1 200"), "{head}");
    assert!(
        h.text()
            .contains("renamed \"home:old/pond\" to=\"home:new/pond\""),
        "{}",
        h.text()
    );
    // reprint: a piece you still have goes into the list with print=always, its recorded seed and system
    let v: calls_inventory::InventoryReprinted = h.ok(
        "/api/inventory/reprint",
        &reprint_req(ColourWord::Single, Some(h.id("kit.dmap"))),
    );
    assert_eq!((v.always, v.made), (true, false));
    let kit = std::fs::read_to_string(h.dir.join("kit.dmap")).unwrap();
    assert!(
        kit.contains("piece p2 \"home:clearing\" count=1 seed=5 system=db print=always"),
        "{kit}"
    );
    assert!(
        kit.contains("plates printer=\"sovol/sv06\""),
        "the rest is kept: {kit}"
    );
    // a piece you have none of is a plain piece; no list makes a new one, laid out for the piece's colour mode
    let _: calls_inventory::InventoryWrote = h.ok(
        "/api/inventory/gone",
        &gone_req("home:clearing", ColourWord::Single),
    );
    let v: calls_inventory::InventoryReprinted = h.ok(
        "/api/inventory/reprint",
        &reprint_req(ColourWord::Multi, None),
    );
    assert!(v.made);
    let made = std::fs::read_to_string(&v.list).unwrap();
    assert!(
        made.contains("piece p1 \"home:clearing\" count=1 seed=5 system=db\n")
            && !made.contains("always"),
        "{made}"
    );
    assert!(made.contains("system=db mode=multicolour"), "{made}");
    assert!(
        dt_doc::map::decode("reprint.dmap", &made).is_ok(),
        "it reads as a print list"
    );
    // a list with a different colour mode is warned about, not refused
    let v: calls_inventory::InventoryReprinted = h.ok(
        "/api/inventory/reprint",
        &reprint_req(ColourWord::Multi, Some(h.id("kit.dmap"))),
    );
    let warning = v.warning.as_deref().unwrap_or_default();
    assert!(
        warning.contains("single colour") || warning.contains("multicolour"),
        "{v:?}"
    );
}

#[test]
fn undo_is_refused_when_the_file_changed_since_the_edit() {
    let h = start_home("undo409", &[("glade.dtile", GLADE)]);
    let _: places::Listed = h.ok("/api/lists/new", &new_list("Crossing", None));
    let file = h.dir.join("Crossing.dmap");
    let list = h.id("Crossing.dmap");
    let b: lists::ListsAddRes = h.ok(
        "/api/lists/add",
        &add_req(&h.id("glade.dtile"), Some(&list)),
    );
    let (before, rev) = (b.edited.before.clone().unwrap(), b.edited.doc_rev);
    // the file is changed behind the editor's back (another tab, a text editor)
    let other = std::fs::read_to_string(&file)
        .unwrap()
        .replace("count=1", "count=7");
    assert!(other.contains("count=7"), "{other}");
    std::fs::write(&file, &other).unwrap();
    let (head, v) = h.refused(
        "/api/lists/restore",
        &lists::ListsRestoreReq {
            list: list.clone(),
            text: before.clone(),
            doc_rev: rev,
        },
    );
    assert!(head.starts_with("HTTP/1.1 409"), "{head} {v:?}");
    assert!(
        v.doc_rev.unwrap() > rev,
        "the file's change is a revision of its own: {v:?}"
    );
    assert_eq!(
        v.text.as_deref(),
        Some(other.as_str()),
        "the answer carries the file as it is"
    );
    assert!(v.error.message.contains("changed since this edit"), "{v:?}");
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        other,
        "nothing was written"
    );
    // a restore that says nothing of the revision it expects is not a restore (a body the request type cannot
    // write: `docRev` is not optional, so this one is the wire's own)
    let raw = format!(
        r#"{{"list":{list:?},"text":{}}}"#,
        serde_json::to_string(&before).unwrap()
    );
    let (head, _) = post_raw(&h.r, "/api/lists/restore", Some(&h.token), None, &raw);
    assert!(head.starts_with("HTTP/1.1 400"), "{head}");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), other);
}

#[test]
fn a_print_list_is_started_filled_edited_and_checked_from_the_editor() {
    let h = start_home("lists", &[("glade.dtile", GLADE)]);
    // a new list in My tiles: an empty map for the printer last chosen, never over a file
    let (head, v): (String, places::Listed) = h.call("/api/lists/new", &new_list("Crossing", None));
    assert!(head.starts_with("HTTP/1.1 200"), "{head} {v:?}");
    let text = std::fs::read_to_string(h.dir.join("Crossing.dmap")).unwrap();
    assert!(
        text.contains("map {") && text.contains("plates printer=\"bambu/x1c-ams\" system=db"),
        "{text}"
    );
    assert!(dt_doc::map::decode("c", &text).is_ok());
    let (_, v2): (String, places::Listed) = h.call("/api/lists/new", &new_list("Crossing", None));
    assert!(v2.path.ends_with("Crossing 2.dmap"), "{v2:?}");
    // a name that climbs out of the folder is a name, not a path
    h.head("/api/lists/new", &new_list("../../escape", None));
    assert!(!h.dir.join("../../escape.dmap").exists());
    let list = h.id("Crossing.dmap");
    let glade = h.id("glade.dtile");
    // add a tile: a piece; again: its count; the program says what it replaced, for Undo
    let b: lists::ListsAddRes = h.ok("/api/lists/add", &add_req(&glade, Some(&list)));
    assert_eq!((b.count, b.made), (1, false));
    let b: lists::ListsAddRes = h.ok("/api/lists/add", &add_req(&glade, Some(&list)));
    assert_eq!(b.count, 2);
    let before = b.edited.before.clone().unwrap();
    let now = std::fs::read_to_string(h.dir.join("Crossing.dmap")).unwrap();
    assert!(now.contains("piece p1 \"home:glade\" count=2"), "{now}");
    assert!(before.contains("count=1"));
    // the plates are laid out again with the two
    h.wait("Crossing.dmap", "two pieces planned", |t| {
        t.status == "ready" && plates_planned(t).is_some_and(|p| p.pieces[0].count == 2)
    });
    // count and remove through the panel's calls, Undo by restore
    // the pieces are named by id (the editor wrote them), and nothing needs saying
    let c: lists::ListEdited =
        h.edited("/api/lists/count", &list, |doc_rev| lists::ListsCountReq {
            list: list.clone(),
            id: "p1".to_owned(),
            count: 5,
            doc_rev,
        });
    assert!(c.note.is_none(), "{c:?}");
    assert!(
        std::fs::read_to_string(h.dir.join("Crossing.dmap"))
            .unwrap()
            .contains("count=5")
    );
    assert!(
        h.edit_refused("/api/lists/count", &list, |doc_rev| lists::ListsCountReq {
            list: list.clone(),
            id: "p1".to_owned(),
            count: 0,
            doc_rev,
        })
        .0
        .starts_with("HTTP/1.1 400")
    );
    let r: lists::ListEdited = h.edited("/api/lists/remove", &list, |doc_rev| {
        lists::ListsRemoveReq {
            list: list.clone(),
            id: "p1".to_owned(),
            doc_rev,
        }
    });
    assert!(r.note.is_none(), "ids are written once: {r:?}");
    assert!(
        !std::fs::read_to_string(h.dir.join("Crossing.dmap"))
            .unwrap()
            .contains("piece")
    );
    let _: lists::ListEdited = h.ok(
        "/api/lists/restore",
        &lists::ListsRestoreReq {
            list: list.clone(),
            text: r.before.clone().unwrap(),
            doc_rev: r.doc_rev,
        },
    );
    assert!(
        std::fs::read_to_string(h.dir.join("Crossing.dmap"))
            .unwrap()
            .contains("count=5")
    );
    // a tile with no list named starts a new one holding it
    let b: lists::ListsAddRes = h.ok("/api/lists/add", &add_req(&glade, None));
    assert!(b.made);
    assert!(
        std::fs::read_to_string(&b.edited.path)
            .unwrap()
            .contains("piece p1 \"home:glade\"")
    );
    // a list that does not read is shown as data and never written
    std::fs::write(
        h.dir.join("Crossing.dmap"),
        "dharmatiles version=1\nmap {\n piece\n}\n",
    )
    .unwrap();
    let (head, v) = h.refused("/api/lists/add", &add_req(&glade, Some(&list)));
    assert!(
        head.starts_with("HTTP/1.1 422") && v.errors.is_some(),
        "{head} {v:?}"
    );
    assert_eq!(
        std::fs::read_to_string(h.dir.join("Crossing.dmap")).unwrap(),
        "dharmatiles version=1\nmap {\n piece\n}\n"
    );
    // the source of a list is served, saved with the token, and checked by its decoder
    let (head, body) = get(&h.r, &format!("/api/tiles/{list}/text"));
    assert!(head.starts_with("HTTP/1.1 200"), "{head}");
    let served: route::TextRes = read(&body);
    assert!(served.text.contains("map {"), "{served:?}");
    let edited = "dharmatiles version=1\nmap {\n piece \"home:glade\" count=7\n}\n";
    let (head, v): (String, edit::SaveRes) = h.call(
        &format!("/api/tiles/{list}/save"),
        &edit::SaveReq {
            text: edited.to_owned(),
            doc_rev: served.doc_rev,
            file: None,
        },
    );
    assert!(head.starts_with("HTTP/1.1 200"), "{head} {v:?}");
    assert_eq!(
        std::fs::read_to_string(h.dir.join("Crossing.dmap")).unwrap(),
        edited
    );
    h.wait(
        "Crossing.dmap",
        "the plates planned again from the saved source",
        |t| t.status == "ready" && plates_planned(t).is_some_and(|p| p.pieces[0].count == 7),
    );
    // the source of a list is checked by its decoder, mistakes at their text
    let check = |text: &str| -> edit::CheckRes {
        h.call(
            &format!("/api/tiles/{list}/check"),
            &edit::TextReq {
                text: text.to_owned(),
                file: None,
            },
        )
        .1
    };
    let c = check("dharmatiles version=1\nmap {\n piece \"x\" count=0\n}\n");
    assert!(!c.ok, "{c:?}");
    assert!(c.errors[0].message.contains("count"), "{c:?}");
    let c = check("dharmatiles version=1\nmap {\n piece \"x\"\n}\n");
    assert!(c.ok, "{c:?}");
}

const TOWN: &str = "dharmatiles version=1\nmap {\n    grid cols=4 rows=3\n    let $step \"4dmm\"\n    piece p1 \"home:glade\" at=\"(0sq, 0sq)\" rot=0\n    piece p2 \"home:glade\" at=\"(1sq, 0sq)\" rot=90 seed=9 bottom=\"$step + 1mm\"\n    piece p3 \"home:glade\" at=\"(2sq, 1sq)\" rot=0 bottom=\"$wallHeight\"\n}\nplates printer=\"sovol/sv06\" mode=\"single\"\n";

#[test]
fn a_map_is_read_edited_pinned_and_given_to_the_page_as_a_scene() {
    let h = start_home("maps", &[("glade.dtile", GLADE), ("town.dmap", TOWN)]);
    let list = h.id("town.dmap");
    let file = h.dir.join("town.dmap");
    // opened, a map is a scene before anything is laid out: each piece's place in mm, its bottom by name
    let t = h.wait("town.dmap", "the scene", |t| has_pieces(t, 3));
    let sc = scene(&t).unwrap();
    assert!(sc.map);
    assert_eq!(sc.grid.as_ref().unwrap().cols, 4);
    assert_eq!(piece_of(sc, "p2").at, Some([35.0, 0.0]));
    assert_eq!(sc.pieces[1].rot, Some(90));
    assert_eq!(sc.pieces[1].seed, Some(9));
    // 4 dmm + 1 mm on DungeonBlocks, and a standard name: 33.1 dmm
    assert!((sc.pieces[1].bottom_mm - 5.0).abs() < 1e-9);
    assert!((sc.pieces[2].bottom_mm - 33.1).abs() < 1e-9);
    assert_eq!(sc.pieces[0].seed_used, 5, "the tile's own seed");
    assert!(sc.heights.iter().any(|n| n.name == "wallHeight"));
    assert!(sc.doc_rev >= 1);
    assert_eq!(sc.pieces[1].id, "p2");
    // a piece is named by its id and has no index: a field the type does not have, so this is the wire's own read
    let wire: Value = read(&get(&h.r, "/api/tiles").1);
    let on_wire = wire["tiles"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == list.as_str())
        .unwrap();
    assert!(
        on_wire["plan"]["scene"]["pieces"][1].get("index").is_none(),
        "a piece is named by its id"
    );
    // a tile of the map is served at a tier, as the map builds it (its own seed here)
    let id = h.id("glade.dtile");
    let want = |tier: &str, seed: Option<u64>, system: &str| tiers::TiersWantReq {
        scope: "t".to_owned(),
        doc: None,
        wants: vec![tiers::TierWant {
            seed,
            system: Some(system.parse().unwrap()),
            ..tier_want(&id, tier)
        }],
    };
    let (head, v): (String, tiers::TiersWantRes) =
        h.call("/api/tiers/want", &want("super-low", Some(9), "db"));
    assert!(head.starts_with("HTTP/1.1 200"), "{head} {v:?}");
    let (head, body) = wait_tier(&h.r, &v.keys[0], None);
    assert!(head.starts_with("HTTP/1.1 200"), "{head}");
    assert_eq!(&body[..4], b"DTM1");
    let head = h.head("/api/tiers/want", &want("low", None, "xx"));
    assert!(head.starts_with("HTTP/1.1 400"), "{head}");
    // add: a new piece with its own id, in the answer's scene at once
    let b: maps::MapsPieced = h.edited("/api/maps/add", &list, |doc_rev| maps::MapsAddReq {
        at: Some("(3sq, 2sq)".to_owned()),
        rot: Some(180),
        seed: Some(77),
        ..add_piece_req(&list, &id, doc_rev)
    });
    assert_eq!(b.id, "p4");
    let bs = b.edited.scene.as_ref().unwrap();
    assert_eq!(
        bs.doc_rev, b.edited.doc_rev,
        "the scene is of the revision the call made"
    );
    // an edit made against an older revision is refused with the file as it is
    let stale = b.edited.doc_rev - 1;
    let (head, v) = h.refused("/api/maps/set", &set_req(&list, "p1", rot_words(90), stale));
    assert!(head.starts_with("HTTP/1.1 409"), "{head} {v:?}");
    assert_eq!(v.doc_rev, Some(b.edited.doc_rev));
    assert_eq!(
        v.scene.as_ref().unwrap().pieces.len(),
        4,
        "the current state comes with it"
    );
    // and one that names no revision is not an edit (a body the request type cannot write: `docRev` is not
    // optional, so this one is the wire's own)
    let raw = format!(r#"{{"list":{list:?},"id":"p1","set":{{"rot":90}}}}"#);
    let (head, _) = post_raw(&h.r, "/api/maps/set", Some(&h.token), None, &raw);
    assert!(head.starts_with("HTTP/1.1 400"), "{head}");
    assert_eq!(bs.pieces.len(), 4);
    assert_eq!(bs.pieces[3].seed, Some(77));
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(
        text.contains("piece p4 \"home:glade\" at=\"(3sq, 2sq)\" rot=180 seed=77"),
        "{text}"
    );
    assert!(
        text.contains("grid cols=4 rows=3") && text.contains("let $step"),
        "every other byte is kept"
    );
    // set: move and turn, take a seed off; the toast's Undo is the text it replaced
    let before = std::fs::read_to_string(&file).unwrap();
    let b: lists::ListEdited = h.edited("/api/maps/set", &list, |doc_rev| {
        set_req(
            &list,
            "p2",
            maps::PieceWords {
                at: Some(Some("(1sq, 1sq)".to_owned())),
                rot: Some(Some(270)),
                seed: Some(None),
                ..Default::default()
            },
            doc_rev,
        )
    });
    assert_eq!(b.before.as_deref(), Some(before.as_str()));
    let l = dt_doc::map::decode("t", &std::fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(l.pieces[1].rot.as_ref().unwrap().value, 270);
    assert!(l.pieces[1].seed.is_none());
    // a word the document refuses is shown as data and writes nothing
    let was = std::fs::read_to_string(&file).unwrap();
    let (head, v) = h.edit_refused("/api/maps/set", &list, |doc_rev| {
        set_req(&list, "p2", rot_words(45), doc_rev)
    });
    assert!(
        head.starts_with("HTTP/1.1 422") && v.errors.is_some(),
        "{head} {v:?}"
    );
    assert_eq!(std::fs::read_to_string(&file).unwrap(), was);
    // batch: several pieces in one change
    let b: maps::MapsBatched = h.edited("/api/maps/batch", &list, |doc_rev| maps::MapsBatchReq {
        list: list.clone(),
        edits: vec![
            edit_of(
                "p1",
                maps::PieceWords {
                    seed: Some(Some(3)),
                    ..Default::default()
                },
            ),
            edit_of(
                "p3",
                maps::PieceWords {
                    seed: Some(Some(4)),
                    print: Some(Some("skip".to_owned())),
                    ..Default::default()
                },
            ),
        ],
        doc_rev,
    });
    assert_eq!(b.changed, 2);
    assert_eq!(b.edited.scene.as_ref().unwrap().pieces[2].print, "skip");
    // duplicate: its own id, the original's seed, the place asked for; remove takes the piece and its pin
    let b: maps::MapsPieced = h.edited("/api/maps/dup", &list, |doc_rev| maps::MapsDupReq {
        list: list.clone(),
        id: "p1".to_owned(),
        at: Some("(0sq, 2sq)".to_owned()),
        near: None,
        doc_rev,
    });
    assert_eq!(b.id, "p5");
    let bs = b.edited.scene.as_ref().unwrap();
    assert_eq!(bs.pieces[1].id, "p5");
    assert_eq!(bs.pieces[1].seed, Some(3));
    // a pin names the piece by its id
    let pin = |doc_rev: u64, x: f64, y: f64, turn: Option<bool>| maps::MapsPinReq {
        list: list.clone(),
        id: "p5".to_owned(),
        plate: 1,
        x,
        y,
        turn,
        doc_rev,
    };
    let b: maps::MapsPieced = h.edited("/api/maps/pin", &list, |r| pin(r, 12.34, 20.0, Some(true)));
    assert_eq!(b.id, "p5");
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(
        text.contains("pin \"p5\" plate=1 at=\"(12.3mm, 20mm)\" turn=90"),
        "{text}"
    );
    assert!(b.edited.scene.as_ref().unwrap().pieces[1].pinned);
    let _: lists::ListEdited = h.edited("/api/maps/unpin", &list, |r| piece_req(&list, "p5", r));
    assert!(!std::fs::read_to_string(&file).unwrap().contains("pin "));
    let _: maps::MapsPieced = h.edited("/api/maps/pin", &list, |r| pin(r, 5.0, 5.0, None));
    let _: lists::ListEdited = h.edited("/api/maps/remove", &list, |r| piece_req(&list, "p5", r));
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(
        !text.contains("p5") && !text.contains("pin "),
        "the piece goes with its pin: {text}"
    );
    let u: lists::ListEdited =
        h.edited("/api/maps/unpin-all", &list, |doc_rev| maps::MapsListReq {
            list: list.clone(),
            doc_rev,
        });
    // undo through the print lists' restore, the scene answered at once
    let r: lists::ListEdited = h.ok(
        "/api/lists/restore",
        &lists::ListsRestoreReq {
            list: list.clone(),
            text: TOWN.to_owned(),
            doc_rev: u.doc_rev,
        },
    );
    assert_eq!(r.scene.as_ref().unwrap().pieces.len(), 3);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), TOWN);
    // a new map has a grid, so it opens as a map before a piece is placed
    let (head, v): (String, places::Listed) =
        h.call("/api/lists/new", &new_list("Table", Some(true)));
    assert!(head.starts_with("HTTP/1.1 200"), "{head} {v:?}");
    let made = std::fs::read_to_string(&v.path).unwrap();
    assert!(
        made.contains("grid cols=8 rows=6") && made.contains("about \"A map.\""),
        "{made}"
    );
    assert!(dt_doc::map::decode("m", &made).is_ok());
    // the plates see the placed pieces as pieces to print, one each
    let t = h.wait("town.dmap", "the plates", |t| {
        t.status == "ready" && plates_planned(t).is_some()
    });
    assert_eq!(plates_planned(&t).unwrap().layout[0].pieces.len(), 3);
}

#[test]
fn the_first_edit_of_a_file_without_ids_writes_ids_on_every_piece_and_says_so_once() {
    let hand = "dharmatiles version=1\n// by hand\nmap {\n    grid cols=4 rows=3\n    piece \"home:glade\" at=\"(0sq, 0sq)\" rot=0\n    piece p1 \"home:glade\" at=\"(1sq, 0sq)\" rot=0\n    piece \"home:glade\" at=\"(2sq, 0sq)\" rot=0\n}\n";
    let h = start_home("noids", &[("glade.dtile", GLADE), ("hand.dmap", hand)]);
    let list = h.id("hand.dmap");
    // the scene names each piece by the id the first edit will write (p1 is taken: p2, then p1, then p3)
    let t = h.wait("hand.dmap", "the scene", |t| has_pieces(t, 3));
    let ids: Vec<_> = scene(&t)
        .unwrap()
        .pieces
        .iter()
        .map(|p| (p.id.clone(), p.written))
        .collect();
    assert_eq!(
        ids,
        [
            ("p2".to_owned(), false),
            ("p1".to_owned(), true),
            ("p3".to_owned(), false)
        ]
    );
    // turning the piece the scene calls p3 writes the ids on all of them, in that one write
    let b: lists::ListEdited = h.edited("/api/maps/set", &list, |doc_rev| {
        set_req(&list, "p3", rot_words(90), doc_rev)
    });
    assert!(b.note.as_deref().unwrap().contains("an id"), "{b:?}");
    let text = std::fs::read_to_string(h.dir.join("hand.dmap")).unwrap();
    assert!(text.contains("// by hand"), "{text}");
    assert!(
        text.contains("piece p2 \"home:glade\" at=\"(0sq, 0sq)\""),
        "{text}"
    );
    assert!(
        text.contains("piece p3 \"home:glade\" at=\"(2sq, 0sq)\" rot=90"),
        "{text}"
    );
    assert!(b.scene.as_ref().unwrap().pieces[2].written);
    // the next edit has nothing to say
    let b: lists::ListEdited = h.edited("/api/maps/set", &list, |doc_rev| {
        set_req(&list, "p2", rot_words(180), doc_rev)
    });
    assert!(b.note.is_none(), "{b:?}");
}

#[test]
fn the_scene_says_which_pieces_the_shelf_leaves_off_and_how_many_print() {
    let h = start_home("shelf", &[("glade.dtile", GLADE), ("town.dmap", TOWN)]);
    // no inventory: nothing is had, every piece prints
    let t = h.wait("town.dmap", "the scene", |t| has_pieces(t, 3));
    let sc = scene(&t).unwrap();
    assert!(!sc.shelf_exists);
    assert_eq!((sc.have, sc.to_print), (0, 3));
    assert_eq!(sc.pieces[0].shelf.as_ref().unwrap().state, "need");
    // one good print of the tile's own seed: the first piece of that seed in the order written is the one you have
    let _: calls_inventory::InventoryWrote = h.ok(
        "/api/inventory/have",
        &h.have_req("glade.dtile", ColourWord::Single, Some("2026-11-12")),
    );
    let t = h.wait("town.dmap", "the shelf", |t| {
        scene(t).is_some_and(|s| s.shelf_exists)
    });
    let sc = scene(&t).unwrap();
    let state = |i: usize| sc.pieces[i].shelf.as_ref().unwrap().state.clone();
    assert_eq!(
        (state(0), state(1), state(2)),
        ("have".to_owned(), "need".to_owned(), "need".to_owned()),
        "{sc:?}"
    );
    assert_eq!((sc.have, sc.to_print), (1, 2));
    // a piece left off by its own word is neither
    assert_eq!(sc.pieces[0].shelf.as_ref().unwrap().text, "have");
}

/// The place of every piece of the scene in the answer of an edit.
fn spots(scene: &SceneJson) -> Vec<(String, f64, f64)> {
    scene
        .pieces
        .iter()
        .filter_map(|p| p.at.map(|at| (p.id.clone(), at[0], at[1])))
        .collect()
}

#[test]
fn two_edits_asking_for_the_same_square_end_on_two_squares_and_each_says_where_it_went() {
    let h = start_home("near", &[("glade.dtile", GLADE), ("town.dmap", TOWN)]);
    let list = h.id("town.dmap");
    let tile = h.id("glade.dtile");
    h.wait("town.dmap", "the scene", |t| has_pieces(t, 3));
    let rev = h.rev(&list);
    // both are made against the same revision and both ask for the nearest free square to (10, 10)
    let asks = |h: &Home, rev: u64| {
        let req = maps::MapsAddReq {
            near: Some(Near { x: 10.0, y: 10.0 }),
            rot: Some(0),
            ..add_piece_req(&list, &tile, rev)
        };
        post_raw(&h.r, "/api/maps/add", Some(&h.token), None, &wire(&req))
    };
    let (a, b) = std::thread::scope(|s| {
        let x = s.spawn(|| asks(&h, rev));
        let y = s.spawn(|| asks(&h, rev));
        (x.join().unwrap(), y.join().unwrap())
    });
    // one is made; the other, made against a revision that has gone, is refused with the revision now, and asks again
    let (won, lost) = if a.0.starts_with("HTTP/1.1 200") {
        (a, b)
    } else {
        (b, a)
    };
    assert!(won.0.starts_with("HTTP/1.1 200"), "{}", won.0);
    assert!(lost.0.starts_with("HTTP/1.1 409"), "{}", lost.0);
    let won: maps::MapsPieced = read(&won.1);
    let lost: ErrorBody = read(&lost.1);
    let (head, body) = asks(&h, lost.doc_rev.unwrap());
    assert!(head.starts_with("HTTP/1.1 200"), "{head}");
    let again: maps::MapsPieced = read(&body);
    // the first got the square asked for; the second, made against a map with that square taken, another, and says so
    assert_eq!(won.shifted, 0);
    assert_eq!(again.shifted, 1, "{again:?}");
    let first = spots(won.edited.scene.as_ref().unwrap())
        .into_iter()
        .find(|(id, ..)| *id == won.id)
        .unwrap();
    let second = spots(again.edited.scene.as_ref().unwrap())
        .into_iter()
        .find(|(id, ..)| *id == again.id)
        .unwrap();
    assert_eq!((first.1, first.2), (350.0, 350.0));
    assert_ne!(
        (first.1, first.2),
        (second.1, second.2),
        "two pieces on one square"
    );
}

#[test]
fn fresh_seeds_are_not_owned_not_on_the_map_and_not_repeated_within_an_edit() {
    let h = start_home("fresh", &[("glade.dtile", GLADE), ("town.dmap", TOWN)]);
    let list = h.id("town.dmap");
    h.wait("town.dmap", "the scene", |t| has_pieces(t, 3));
    // one good print of seed 5 is on the shelf: a fresh seed is not that one, nor 9 (p2), in any piece of one batch
    let _: calls_inventory::InventoryWrote = h.ok(
        "/api/inventory/have",
        &h.have_req("glade.dtile", ColourWord::Single, Some("2026-11-12")),
    );
    let fresh = maps::PieceWords {
        fresh_seed: Some(true),
        ..Default::default()
    };
    let b: maps::MapsBatched = h.edited("/api/maps/batch", &list, |doc_rev| maps::MapsBatchReq {
        list: list.clone(),
        edits: vec![
            edit_of(
                "p1",
                maps::PieceWords {
                    fresh_seed: Some(true),
                    ..Default::default()
                },
            ),
            edit_of("p3", fresh),
        ],
        doc_rev,
    });
    let sc = b.edited.scene.as_ref().unwrap();
    let seeds: Vec<u64> = ["p1", "p3"]
        .iter()
        .map(|id| piece_of(sc, id).seed_used)
        .collect();
    assert_ne!(seeds[0], seeds[1], "a seed repeated within an edit");
    assert!(seeds.iter().all(|s| ![5, 9].contains(s)), "{seeds:?}");
    // both an `at` and a `near`, or a seed and a fresh one, is a mistake
    let (head, _) = h.edit_refused("/api/maps/set", &list, |doc_rev| {
        set_req(
            &list,
            "p1",
            maps::PieceWords {
                at: Some(Some("(1sq, 1sq)".to_owned())),
                near: Some(Near { x: 1.0, y: 1.0 }),
                ..Default::default()
            },
            doc_rev,
        )
    });
    assert!(head.starts_with("HTTP/1.1 400"), "{head}");
}

/// What the stream says, for as long as it says anything: the events (parsed) from a stream opened now, until one
/// that `stop` accepts.  `on_open` runs once the stream's first event (the snapshot) is in: the stream is then
/// subscribed, so what `on_open` makes happen is told on it and nothing is missed.
fn events_until(
    h: &Home,
    on_open: impl FnOnce(),
    stop: impl Fn(&StreamEvent) -> bool,
) -> Vec<StreamEvent> {
    let mut c = TcpStream::connect(h.r.addr).unwrap();
    write!(c, "GET /api/events HTTP/1.1\r\nHost: {}\r\n\r\n", h.r.addr).unwrap();
    c.set_read_timeout(Some(Duration::from_secs(60))).unwrap();
    let mut seen = Vec::new();
    let mut on_open = Some(on_open);
    for l in BufReader::new(c).lines() {
        let Ok(l) = l else { break };
        let Some(data) = l.strip_prefix("data: ") else {
            continue;
        };
        let v: StreamEvent = read(data.as_bytes());
        let done = stop(&v);
        seen.push(v);
        if done {
            break;
        }
        if let Some(f) = on_open.take() {
            f();
        }
    }
    seen
}

#[test]
fn a_change_is_told_as_the_resource_that_changed_and_not_as_the_whole() {
    let h = start_home("resources", &[("glade.dtile", GLADE), ("town.dmap", TOWN)]);
    let new_folder = |name: &str| {
        let (head, v): (String, places::FolderDone) =
            h.call("/api/folders/new", &folder_new_req(name));
        assert!(head.starts_with("HTTP/1.1 200"), "{head} {v:?}");
    };
    let names_folder = |v: &StreamEvent, name: &str| {
        matches!(v, StreamEvent::Resource(e)
            if matches!(&e.resource, Resource::Places(p) if p.folders.iter().any(|f| f.path == name)))
    };
    // The stream is open once its snapshot is in.  What the server tells after a start (the inventory settling, a tile
    // read) is told before the first folder is made: wait until it has nothing left, so none of it can be mistaken for
    // what the folder changed.  Then a folder, and a second one as the end of the first one's telling: one change is
    // one batch (places first), so what lies between the two places events is the rest of the first batch.
    let seen = events_until(
        &h,
        || {
            wait_idle(&h.r);
            new_folder("dungeon");
            new_folder("sentinel");
        },
        |v| names_folder(v, "sentinel"),
    );
    assert!(
        matches!(seen[0], StreamEvent::Snapshot(_)),
        "the stream begins with the whole"
    );
    let at = seen
        .iter()
        .position(|v| names_folder(v, "dungeon"))
        .expect("the folder is told as the places");
    let end = seen.len() - 1;
    // the folder is told as the places, with a revision of its own that rose
    let StreamEvent::Resource(told) = &seen[at] else {
        panic!("not a resource: {:?}", seen[at]);
    };
    assert!(told.res_rev >= 1);
    // and as nothing else: no whole snapshot, no inventory, no list of tiles, no tile
    assert!(
        seen[at + 1..end].iter().all(|v| match v {
            StreamEvent::Snapshot(_) | StreamEvent::Tile { .. } => false,
            StreamEvent::Resource(e) =>
                !matches!(e.resource, Resource::Inventory(_) | Resource::List(_)),
            _ => true,
        }),
        "a folder does not resend the inventory or the tiles: {:?}",
        &seen[at + 1..end]
    );
}

#[test]
fn a_documents_scene_and_plates_are_of_one_revision_whenever_the_page_is_sent_them() {
    let h = start_home("onerev", &[("glade.dtile", GLADE), ("town.dmap", TOWN)]);
    let tiles_of = |v: &StreamEvent| -> Vec<TileSummary> {
        match v {
            StreamEvent::Tile { tile, .. } => vec![(**tile).clone()],
            StreamEvent::Snapshot(s) => s.tiles.clone(),
            _ => vec![],
        }
    };
    let map = |t: &TileSummary| t.path.ends_with(".dmap");
    let planned = |t: &TileSummary| map(t) && t.status == "ready" && plates_planned(t).is_some();
    // every state of the map the stream sends (the whole first, then each change) names one revision for its scene and its plan
    // a tile in a folder builds when it is chosen (the page asks when you open it): the stream is open first, then
    // the map is chosen, and the plates it plans are told on the stream.  (Waiting for them before the stream is
    // opened, as this test once did, is how it hung: nothing had asked, so nothing was ever planned to tell.)
    let seen = events_until(
        &h,
        || {
            assert!(
                h.shared.want(&h.id("town.dmap")),
                "the map was not asked for"
            );
        },
        |v| tiles_of(v).iter().any(planned),
    );
    let mut n = 0;
    for v in &seen {
        for t in tiles_of(v).iter().filter(|t| map(t)) {
            let Some(TilePlan::Plates(d)) = &t.plan else {
                continue;
            };
            if let Some(sc) = &d.scene {
                n += 1;
                assert_eq!(sc.doc_rev, d.doc_rev);
            }
        }
    }
    assert!(n > 0, "no state of the map was seen");
}
