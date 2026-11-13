//! The inventory in the editor: the snapshot carries it, "I have this", the
//! sheet after a print, gone, a tile that moved, a reprint into a print list,
//! and what the plates make of it.  A scratch home of the test's own.

use super::*;

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
    let token: Value = serde_json::from_slice(&get(&r, "/api/session").1).unwrap();
    let token = token["token"].as_str().unwrap().to_owned();
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

    fn call(&self, route: &str, body: Value) -> (String, Value) {
        post(&self.r, route, Some(&self.token), None, &body.to_string())
    }

    /// The revision the program holds for the document `id` (what the page names its edits against).
    fn rev(&self, id: &str) -> u64 {
        let (_, body) = get(&self.r, &format!("/api/tiles/{id}/text"));
        serde_json::from_slice::<Value>(&body).unwrap()["docRev"]
            .as_u64()
            .unwrap()
    }

    /// `body` as the page sends an edit: made against the revision the program holds now.
    fn with_rev(&self, mut body: Value) -> Value {
        let id = body["list"]
            .as_str()
            .or_else(|| body["tile"].as_str())
            .unwrap()
            .to_owned();
        body["docRev"] = self.rev(&id).into();
        body
    }

    fn edit_call(&self, route: &str, body: Value) -> (String, Value) {
        self.call(route, self.with_rev(body))
    }

    fn edit(&self, route: &str, body: Value) -> Value {
        self.ok(route, self.with_rev(body))
    }

    fn ok(&self, route: &str, body: Value) -> Value {
        let (head, v) = self.call(route, body);
        assert!(head.starts_with("HTTP/1.1 200"), "{route}: {head} {v}");
        v
    }

    fn snapshot(&self) -> Value {
        tiles(&self.r)
    }

    fn inventory(&self) -> Value {
        self.snapshot()["inventory"].clone()
    }

    fn tile(&self, f: &str) -> Option<Value> {
        let id = self.id(f);
        self.snapshot()["tiles"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["id"] == id.as_str())
            .cloned()
    }

    fn wait(&self, f: &str, what: &str, mut ok: impl FnMut(&Value) -> bool) -> Value {
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
                "timed out waiting for {what}: {}",
                self.snapshot()
            );
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    fn text(&self) -> String {
        std::fs::read_to_string(self.dir.join("inventory.dinv")).unwrap()
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

const GLADE: &str =
    "dharmatiles version=1\ntile size=\"1x1\" seed=5 {\n    stack {\n        dirt\n    }\n}\n";
const KIT: &str = "dharmatiles version=1\nmap {\n    piece \"home:glade\" count=3\n}\nplates printer=\"sovol/sv06\" mode=\"single\"\n";

#[test]
fn i_have_this_makes_the_file_and_the_snapshot_and_the_pills_follow() {
    let h = start_home("have", &[("glade.dtile", GLADE)]);
    // no inventory yet: nothing is made
    assert_eq!(h.inventory()["exists"], false);
    assert!(!h.dir.join("inventory.dinv").exists());
    // a call needs the token
    let body = serde_json::json!({"tile": h.id("glade.dtile"), "colour": "single"}).to_string();
    assert!(
        post(&h.r, "/api/inventory/have", None, None, &body)
            .0
            .starts_with("HTTP/1.1 403")
    );
    // a bad colour is refused, nothing written
    let (head, _) = h.call(
        "/api/inventory/have",
        serde_json::json!({"tile": h.id("glade.dtile"), "colour": "rainbow"}),
    );
    assert!(head.starts_with("HTTP/1.1 400"), "{head}");
    assert!(!h.dir.join("inventory.dinv").exists());
    // a print list or the inventory is not a tile
    let v = h.ok(
        "/api/inventory/have",
        serde_json::json!({"tile": h.id("glade.dtile"), "colour": "multi", "on": "2026-11-12"}),
    );
    assert_eq!(v["lines"].as_array().unwrap().len(), 1);
    let text = h.text();
    assert!(
        text.contains(
            "print \"home:glade\" seed=5 system=db colour=multi result=ok on=\"2026-11-12\" hash=\""
        ),
        "{text}"
    );
    // the snapshot: one piece, held, at the tile's own seed
    let inv = h.inventory();
    assert_eq!(inv["exists"], true);
    assert_eq!(inv["pieces"][0]["tile"], "home:glade");
    assert_eq!(
        (
            inv["pieces"][0]["seed"].as_u64(),
            inv["pieces"][0]["have"].as_u64()
        ),
        (Some(5), Some(1))
    );
    assert_eq!(inv["pieces"][0]["edited"], false);
    assert_eq!(inv["lastColour"], "multi", "the colour the log last used");
    // the file is a tile of My tiles now, opened as an inventory
    let t = h.wait("inventory.dinv", "the inventory document", |t| {
        t["status"] == "ready" && t["plan"]["kind"] == "inventory"
    });
    assert_eq!(t["plan"]["inventory"]["pieces"][0]["have"], 1);
    assert!(
        t["plan"]["inventory"]["text"]
            .as_str()
            .unwrap()
            .contains("home:glade")
    );
    // the tile edited since it was printed: information only, it still counts
    std::fs::write(h.dir.join("glade.dtile"), format!("{GLADE}// edited\n")).unwrap();
    h.shared.rescan();
    let inv = h.inventory();
    assert_eq!(inv["pieces"][0]["edited"], true);
    assert_eq!(inv["pieces"][0]["have"], 1);
    // a second print of the same piece is a second piece; a mistake in the file is never added to
    h.ok(
        "/api/inventory/have",
        serde_json::json!({"tile": h.id("glade.dtile"), "colour": "multi"}),
    );
    assert_eq!(h.inventory()["pieces"][0]["have"], 2);
    let good = h.text();
    std::fs::write(
        h.dir.join("inventory.dinv"),
        good.replace("seed=5", "seed=\"x\""),
    )
    .unwrap();
    let (head, v) = h.call(
        "/api/inventory/have",
        serde_json::json!({"tile": h.id("glade.dtile"), "colour": "multi"}),
    );
    assert!(
        head.starts_with("HTTP/1.1 422") && v["errors"].is_array(),
        "{head} {v}"
    );
    assert!(
        v["errors"][0]["message"]
            .as_str()
            .unwrap()
            .contains("`seed` is a whole number"),
        "{v}"
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
        t["status"] == "ready" && t["plan"]["plates"]["planned"] == true
    });
    let p = &t["plan"]["plates"];
    assert_eq!(p["kinds"][0]["count"], 3);
    assert_eq!(p["kinds"][0]["canon"], "home:glade");
    assert_eq!(p["kinds"][0]["seedUsed"], 5);
    assert_eq!(p["layout"][0]["pieces"].as_array().unwrap().len(), 3);
    // all three come off plate 1, the second one failed
    let v = h.ok("/api/inventory/record", serde_json::json!({"list": h.id("kit.dmap"), "plate": 1, "failed": [1], "on": "2026-11-13"}));
    assert_eq!(v["lines"].as_array().unwrap().len(), 3);
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
    let inv = h.inventory();
    assert_eq!(
        (
            inv["pieces"][0]["have"].as_u64(),
            inv["pieces"][0]["failed"].as_u64()
        ),
        (Some(2), Some(1))
    );
    // the list is laid out again against the shelf: one to print, two in the inventory
    let t = h.wait("kit.dmap", "the plates against the shelf", |t| {
        t["status"] == "ready" && t["plan"]["plates"]["kinds"][0]["have"] == 2
    });
    let p = &t["plan"]["plates"];
    assert_eq!(
        (
            p["kinds"][0]["want"].as_u64(),
            p["kinds"][0]["count"].as_u64()
        ),
        (Some(3), Some(1))
    );
    assert_eq!(p["layout"][0]["pieces"].as_array().unwrap().len(), 1);
    // the switch: everything prints again, written in the list's plates line
    h.edit(
        "/api/plates/set",
        serde_json::json!({"list": h.id("kit.dmap"), "key": "inventory", "value": "ignore"}),
    );
    let t = h.wait("kit.dmap", "every piece", |t| {
        t["status"] == "ready" && t["plan"]["plates"]["kinds"][0]["count"] == 3
    });
    assert_eq!(t["plan"]["plates"]["checkInventory"], false);
    let kit = std::fs::read_to_string(h.dir.join("kit.dmap")).unwrap();
    assert!(
        kit.contains("inventory=ignore") || kit.contains("inventory=\"ignore\""),
        "{kit}"
    );
    // a plate that is not there, a piece that is not on it, and a list that is not one are refused
    for body in [
        serde_json::json!({"list": h.id("kit.dmap"), "plate": 9}),
        serde_json::json!({"list": h.id("kit.dmap"), "plate": 1, "failed": [7]}),
        serde_json::json!({"list": h.id("glade.dtile"), "plate": 1}),
    ] {
        let (head, _) = h.call("/api/inventory/record", body);
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
    h.ok("/api/inventory/have", serde_json::json!({"tile": h.id("glade.dtile"), "colour": "single", "count": 2, "on": "2026-11-09"}));
    let before = h.text();
    // gone: one of the piece; none to take away is refused
    let piece =
        serde_json::json!({"tile": "home:glade", "seed": 5, "system": "db", "colour": "single"});
    h.ok("/api/inventory/gone", {
        let mut b = piece.clone();
        b["on"] = "2026-11-10".into();
        b["note"] = "dropped on the floor".into();
        b
    });
    assert_eq!(h.inventory()["pieces"][0]["have"], 1);
    assert_eq!(h.inventory()["pieces"][0]["gone"], 1);
    let after = h.text();
    assert!(
        after.starts_with(before.trim_end().trim_end_matches('}')),
        "every earlier byte kept"
    );
    assert!(after.contains("gone \"home:glade\" seed=5 system=db colour=single on=\"2026-11-10\" note=\"dropped on the floor\""), "{after}");
    let none =
        serde_json::json!({"tile": "home:glade", "seed": 5, "system": "db", "colour": "multi"});
    assert!(
        h.call("/api/inventory/gone", none)
            .0
            .starts_with("HTTP/1.1 409")
    );
    // renaming the tile in the editor adds a renamed line, and the count follows
    let (head, _) = h.call(
        &format!("/api/tiles/{}/rename", h.id("glade.dtile")),
        serde_json::json!({"name": "meadow"}),
    );
    assert!(head.starts_with("HTTP/1.1 200"), "{head}");
    assert!(
        h.text()
            .contains("renamed \"home:glade\" to=\"home:meadow\""),
        "{}",
        h.text()
    );
    let inv = h.inventory();
    assert_eq!(
        (
            inv["pieces"][0]["tile"].as_str(),
            inv["pieces"][0]["have"].as_u64()
        ),
        (Some("home:meadow"), Some(1))
    );
    assert_eq!(inv["pieces"][0]["missing"], false);
    // a tile moved into a folder follows too (the move route)
    std::fs::create_dir_all(h.dir.join("sets")).unwrap();
    h.shared.rescan();
    let (head, _) = h.call(
        &format!("/api/tiles/{}/move", h.id("meadow.dtile")),
        serde_json::json!({"root": 0, "dir": "sets"}),
    );
    assert!(head.starts_with("HTTP/1.1 200"), "{head}");
    assert_eq!(h.inventory()["pieces"][0]["tile"], "home:sets/meadow");
    // moved by hand, outside the editor: the path is not there any more, and one click re-links it
    std::fs::rename(
        h.dir.join("sets/meadow.dtile"),
        h.dir.join("clearing.dtile"),
    )
    .unwrap();
    h.shared.rescan();
    let inv = h.inventory();
    assert_eq!(inv["pieces"][0]["missing"], true);
    h.ok("/api/inventory/relink", serde_json::json!({"from": "home:sets/meadow", "to": h.id("clearing.dtile"), "on": "2026-11-11"}));
    let inv = h.inventory();
    assert_eq!(
        (
            inv["pieces"][0]["tile"].as_str(),
            inv["pieces"][0]["missing"].clone()
        ),
        (Some("home:clearing"), serde_json::json!(false))
    );
    assert!(
        h.text()
            .contains("renamed \"home:sets/meadow\" to=\"home:clearing\" on=\"2026-11-11\""),
        "{}",
        h.text()
    );
    assert!(
        h.call(
            "/api/inventory/relink",
            serde_json::json!({"from": "home:never", "to": h.id("clearing.dtile")})
        )
        .0
        .starts_with("HTTP/1.1 409")
    );
    // renaming a folder carries the tiles in it
    std::fs::create_dir_all(h.dir.join("old")).unwrap();
    std::fs::write(h.dir.join("old/pond.dtile"), GLADE).unwrap();
    h.shared.rescan();
    h.ok(
        "/api/inventory/have",
        serde_json::json!({"tile": h.id("old/pond.dtile"), "colour": "single"}),
    );
    let (head, _) = h.call(
        "/api/folders/rename",
        serde_json::json!({"root": 0, "dir": "old", "name": "new"}),
    );
    assert!(head.starts_with("HTTP/1.1 200"), "{head}");
    assert!(
        h.text()
            .contains("renamed \"home:old/pond\" to=\"home:new/pond\""),
        "{}",
        h.text()
    );
    // reprint: a piece you still have goes into the list with print=always, its recorded seed and system
    let v = h.ok("/api/inventory/reprint", serde_json::json!({"tile": "home:clearing", "seed": 5, "system": "db", "colour": "single", "list": h.id("kit.dmap")}));
    assert_eq!(
        (v["always"].clone(), v["made"].clone()),
        (serde_json::json!(true), serde_json::json!(false))
    );
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
    h.ok(
        "/api/inventory/gone",
        serde_json::json!({"tile": "home:clearing", "seed": 5, "system": "db", "colour": "single"}),
    );
    let v = h.ok(
        "/api/inventory/reprint",
        serde_json::json!({"tile": "home:clearing", "seed": 5, "system": "db", "colour": "multi"}),
    );
    assert_eq!(v["made"], true);
    let made = std::fs::read_to_string(v["list"].as_str().unwrap()).unwrap();
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
    let v = h.ok("/api/inventory/reprint", serde_json::json!({"tile": "home:clearing", "seed": 5, "system": "db", "colour": "multi", "list": h.id("kit.dmap")}));
    assert!(
        v["warning"].as_str().unwrap().contains("single colour")
            || v["warning"].as_str().unwrap().contains("multicolour"),
        "{v}"
    );
}

#[test]
fn undo_is_refused_when_the_file_changed_since_the_edit() {
    let h = start_home("undo409", &[("glade.dtile", GLADE)]);
    h.call("/api/lists/new", serde_json::json!({"name": "Crossing"}));
    let file = h.dir.join("Crossing.dmap");
    let list = h.id("Crossing.dmap");
    let b = h.ok(
        "/api/lists/add",
        serde_json::json!({"tile": h.id("glade.dtile"), "list": list}),
    );
    let (before, rev) = (b["before"].clone(), b["docRev"].clone());
    // the file is changed behind the editor's back (another tab, a text editor)
    let other = std::fs::read_to_string(&file)
        .unwrap()
        .replace("count=1", "count=7");
    assert!(other.contains("count=7"), "{other}");
    std::fs::write(&file, &other).unwrap();
    let (head, v) = h.call(
        "/api/lists/restore",
        serde_json::json!({"list": list, "text": before, "docRev": rev}),
    );
    assert!(head.starts_with("HTTP/1.1 409"), "{head} {v}");
    assert!(
        v["docRev"].as_u64().unwrap() > rev.as_u64().unwrap(),
        "the file's change is a revision of its own: {v}"
    );
    assert_eq!(
        v["text"],
        other.as_str(),
        "the answer carries the file as it is"
    );
    assert!(
        v["error"]["message"]
            .as_str()
            .unwrap()
            .contains("changed since this edit"),
        "{v}"
    );
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        other,
        "nothing was written"
    );
    // a restore that says nothing of the revision it expects is not a restore
    let (head, _) = h.call(
        "/api/lists/restore",
        serde_json::json!({"list": list, "text": before}),
    );
    assert!(head.starts_with("HTTP/1.1 400"), "{head}");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), other);
}

#[test]
fn a_print_list_is_started_filled_edited_and_checked_from_the_editor() {
    let h = start_home("lists", &[("glade.dtile", GLADE)]);
    // a new list in My tiles: an empty map for the printer last chosen, never over a file
    let (head, v) = h.call("/api/lists/new", serde_json::json!({"name": "Crossing"}));
    assert!(head.starts_with("HTTP/1.1 200"), "{head} {v}");
    let text = std::fs::read_to_string(h.dir.join("Crossing.dmap")).unwrap();
    assert!(
        text.contains("map {") && text.contains("plates printer=\"bambu/x1c-ams\" system=db"),
        "{text}"
    );
    assert!(dt_doc::map::decode("c", &text).is_ok());
    let (_, v2) = h.call("/api/lists/new", serde_json::json!({"name": "Crossing"}));
    assert!(
        v2["path"].as_str().unwrap().ends_with("Crossing 2.dmap"),
        "{v2}"
    );
    // a name that climbs out of the folder is a name, not a path
    h.call(
        "/api/lists/new",
        serde_json::json!({"name": "../../escape"}),
    );
    assert!(!h.dir.join("../../escape.dmap").exists());
    let list = h.id("Crossing.dmap");
    // add a tile: a piece; again: its count; the program says what it replaced, for Undo
    let b = h.ok(
        "/api/lists/add",
        serde_json::json!({"tile": h.id("glade.dtile"), "list": list}),
    );
    assert_eq!(
        (b["count"].as_u64(), b["made"].clone()),
        (Some(1), serde_json::json!(false))
    );
    let b = h.ok(
        "/api/lists/add",
        serde_json::json!({"tile": h.id("glade.dtile"), "list": list}),
    );
    assert_eq!(b["count"], 2);
    let before = b["before"].as_str().unwrap().to_owned();
    let now = std::fs::read_to_string(h.dir.join("Crossing.dmap")).unwrap();
    assert!(now.contains("piece p1 \"home:glade\" count=2"), "{now}");
    assert!(before.contains("count=1"));
    // the plates are laid out again with the two
    h.wait("Crossing.dmap", "two pieces planned", |t| {
        t["status"] == "ready" && t["plan"]["plates"]["pieces"][0]["count"] == 2
    });
    // count and remove through the panel's calls, Undo by restore
    // the pieces are named by id (the editor wrote them), and nothing needs saying
    let c = h.edit(
        "/api/lists/count",
        serde_json::json!({"list": list, "id": "p1", "count": 5}),
    );
    assert!(c.get("note").is_none_or(Value::is_null), "{c}");
    assert!(
        std::fs::read_to_string(h.dir.join("Crossing.dmap"))
            .unwrap()
            .contains("count=5")
    );
    assert!(
        h.edit_call(
            "/api/lists/count",
            serde_json::json!({"list": list, "id": "p1", "count": 0})
        )
        .0
        .starts_with("HTTP/1.1 400")
    );
    let r = h.edit(
        "/api/lists/remove",
        serde_json::json!({"list": list, "id": "p1"}),
    );
    assert!(
        r.get("note").is_none_or(Value::is_null),
        "ids are written once: {r}"
    );
    assert!(
        !std::fs::read_to_string(h.dir.join("Crossing.dmap"))
            .unwrap()
            .contains("piece")
    );
    h.ok(
        "/api/lists/restore",
        serde_json::json!({"list": list, "text": r["before"], "docRev": r["docRev"]}),
    );
    assert!(
        std::fs::read_to_string(h.dir.join("Crossing.dmap"))
            .unwrap()
            .contains("count=5")
    );
    // a tile with no list named starts a new one holding it
    let b = h.ok(
        "/api/lists/add",
        serde_json::json!({"tile": h.id("glade.dtile")}),
    );
    assert_eq!(b["made"], true);
    assert!(
        std::fs::read_to_string(b["path"].as_str().unwrap())
            .unwrap()
            .contains("piece p1 \"home:glade\"")
    );
    // a list that does not read is shown as data and never written
    std::fs::write(
        h.dir.join("Crossing.dmap"),
        "dharmatiles version=1\nmap {\n piece\n}\n",
    )
    .unwrap();
    let (head, v) = h.call(
        "/api/lists/add",
        serde_json::json!({"tile": h.id("glade.dtile"), "list": list}),
    );
    assert!(
        head.starts_with("HTTP/1.1 422") && v["errors"].is_array(),
        "{head} {v}"
    );
    assert_eq!(
        std::fs::read_to_string(h.dir.join("Crossing.dmap")).unwrap(),
        "dharmatiles version=1\nmap {\n piece\n}\n"
    );
    // the source of a list is served, saved with the token, and checked by its decoder
    let (head, body) = get(&h.r, &format!("/api/tiles/{list}/text"));
    assert!(head.starts_with("HTTP/1.1 200"), "{head}");
    let served: Value = serde_json::from_slice(&body).unwrap();
    assert!(
        served["text"].as_str().unwrap().contains("map {"),
        "{served}"
    );
    let edited = "dharmatiles version=1\nmap {\n piece \"home:glade\" count=7\n}\n";
    let (head, v) = h.call(
        &format!("/api/tiles/{list}/save"),
        serde_json::json!({"text": edited, "docRev": served["docRev"]}),
    );
    assert!(head.starts_with("HTTP/1.1 200"), "{head} {v}");
    assert_eq!(
        std::fs::read_to_string(h.dir.join("Crossing.dmap")).unwrap(),
        edited
    );
    h.wait(
        "Crossing.dmap",
        "the plates planned again from the saved source",
        |t| t["status"] == "ready" && t["plan"]["plates"]["pieces"][0]["count"] == 7,
    );
    // the source of a list is checked by its decoder, mistakes at their text
    let (_, c) = h.call(
        &format!("/api/tiles/{list}/check"),
        serde_json::json!({"text": "dharmatiles version=1\nmap {\n piece \"x\" count=0\n}\n"}),
    );
    assert_eq!(c["ok"], false, "{c}");
    assert!(
        c["errors"][0]["message"]
            .as_str()
            .unwrap()
            .contains("count"),
        "{c}"
    );
    let (_, c) = h.call(
        &format!("/api/tiles/{list}/check"),
        serde_json::json!({"text": "dharmatiles version=1\nmap {\n piece \"x\"\n}\n"}),
    );
    assert_eq!(c["ok"], true, "{c}");
}

const TOWN: &str = "dharmatiles version=1\nmap {\n    grid cols=4 rows=3\n    let $step \"4dmm\"\n    piece p1 \"home:glade\" at=\"(0sq, 0sq)\" rot=0\n    piece p2 \"home:glade\" at=\"(1sq, 0sq)\" rot=90 seed=9 bottom=\"$step + 1mm\"\n    piece p3 \"home:glade\" at=\"(2sq, 1sq)\" rot=0 bottom=\"$wallHeight\"\n}\nplates printer=\"sovol/sv06\" mode=\"single\"\n";

#[test]
fn a_map_is_read_edited_pinned_and_given_to_the_page_as_a_scene() {
    let h = start_home("maps", &[("glade.dtile", GLADE), ("town.dmap", TOWN)]);
    let list = h.id("town.dmap");
    let file = h.dir.join("town.dmap");
    // opened, a map is a scene before anything is laid out: each piece's place in mm, its bottom by name
    let t = h.wait("town.dmap", "the scene", |t| {
        t["plan"]["scene"]["pieces"]
            .as_array()
            .is_some_and(|p| p.len() == 3)
    });
    let sc = &t["plan"]["scene"];
    assert_eq!(sc["map"], true);
    assert_eq!(sc["grid"]["cols"], 4);
    assert_eq!(sc["pieces"][1]["at"], serde_json::json!([35.0, 0.0]));
    assert_eq!(sc["pieces"][1]["rot"], 90);
    assert_eq!(sc["pieces"][1]["seed"], 9);
    // 4 dmm + 1 mm on DungeonBlocks, and a standard name: 33.1 dmm
    assert!((sc["pieces"][1]["bottomMm"].as_f64().unwrap() - 5.0).abs() < 1e-9);
    assert!((sc["pieces"][2]["bottomMm"].as_f64().unwrap() - 33.1).abs() < 1e-9);
    assert_eq!(sc["pieces"][0]["seedUsed"], 5, "the tile's own seed");
    assert!(
        sc["heights"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n["name"] == "wallHeight")
    );
    assert!(sc["docRev"].as_u64().unwrap() >= 1);
    assert_eq!(sc["pieces"][1]["id"], "p2");
    assert!(
        sc["pieces"][1].get("index").is_none(),
        "a piece is named by its id"
    );
    // a tile of the map is served at a tier, as the map builds it (its own seed here)
    let id = h.id("glade.dtile");
    let (head, v) = h.call(
        "/api/tiers/want",
        serde_json::json!({"scope": "t", "wants": [{"tile": id, "tier": "super-low", "seed": 9, "system": "db"}]}),
    );
    assert!(head.starts_with("HTTP/1.1 200"), "{head} {v}");
    let key = v["keys"][0].as_str().unwrap().to_owned();
    let (head, body) = wait_tier(&h.r, &key, None);
    assert!(head.starts_with("HTTP/1.1 200"), "{head}");
    assert_eq!(&body[..4], b"DTM1");
    let (head, _) = h.call(
        "/api/tiers/want",
        serde_json::json!({"scope": "t", "wants": [{"tile": id, "tier": "low", "system": "xx"}]}),
    );
    assert!(head.starts_with("HTTP/1.1 400"), "{head}");
    // add: a new piece with its own id, in the answer's scene at once
    let b = h.edit(
        "/api/maps/add",
        serde_json::json!({"list": list, "tile": id, "at": "(3sq, 2sq)", "rot": 180, "seed": 77}),
    );
    assert_eq!(b["id"].as_str(), Some("p4"));
    assert_eq!(
        b["scene"]["docRev"], b["docRev"],
        "the scene is of the revision the call made"
    );
    // an edit made against an older revision is refused with the file as it is
    let stale = b["docRev"].as_u64().unwrap() - 1;
    let (head, v) = h.call(
        "/api/maps/set",
        serde_json::json!({"list": list, "id": "p1", "set": {"rot": 90}, "docRev": stale}),
    );
    assert!(head.starts_with("HTTP/1.1 409"), "{head} {v}");
    assert_eq!(v["docRev"], b["docRev"]);
    assert_eq!(
        v["scene"]["pieces"].as_array().unwrap().len(),
        4,
        "the current state comes with it"
    );
    // and one that names no revision is not an edit
    let (head, _) = h.call(
        "/api/maps/set",
        serde_json::json!({"list": list, "id": "p1", "set": {"rot": 90}}),
    );
    assert!(head.starts_with("HTTP/1.1 400"), "{head}");
    assert_eq!(b["scene"]["pieces"].as_array().unwrap().len(), 4);
    assert_eq!(b["scene"]["pieces"][3]["seed"], 77);
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
    let b = h.edit(
        "/api/maps/set",
        serde_json::json!({"list": list, "id": "p2", "set": {"at": "(1sq, 1sq)", "rot": 270, "seed": null}}),
    );
    assert_eq!(b["before"], before.as_str());
    let l = dt_doc::map::decode("t", &std::fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(l.pieces[1].rot.as_ref().unwrap().value, 270);
    assert!(l.pieces[1].seed.is_none());
    // a word the document refuses is shown as data and writes nothing
    let was = std::fs::read_to_string(&file).unwrap();
    let (head, v) = h.edit_call(
        "/api/maps/set",
        serde_json::json!({"list": list, "id": "p2", "set": {"rot": 45}}),
    );
    assert!(
        head.starts_with("HTTP/1.1 422") && v["errors"].is_array(),
        "{head} {v}"
    );
    assert_eq!(std::fs::read_to_string(&file).unwrap(), was);
    // batch: several pieces in one change
    let b = h.edit(
        "/api/maps/batch",
        serde_json::json!({"list": list, "edits": [{"id": "p1", "set": {"seed": 3}}, {"id": "p3", "set": {"seed": 4, "print": "skip"}}]}),
    );
    assert_eq!(b["changed"], 2);
    assert_eq!(b["scene"]["pieces"][2]["print"], "skip");
    // duplicate: its own id, the original's seed, the place asked for; remove takes the piece and its pin
    let b = h.edit(
        "/api/maps/dup",
        serde_json::json!({"list": list, "id": "p1", "at": "(0sq, 2sq)"}),
    );
    assert_eq!(b["id"], "p5");
    assert_eq!(b["scene"]["pieces"][1]["id"], "p5");
    assert_eq!(b["scene"]["pieces"][1]["seed"], 3);
    // a pin names the piece by its id
    let b = h.edit(
        "/api/maps/pin",
        serde_json::json!({"list": list, "id": "p5", "plate": 1, "x": 12.34, "y": 20.0, "turn": true}),
    );
    assert_eq!(b["id"], "p5");
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(
        text.contains("pin \"p5\" plate=1 at=\"(12.3mm, 20mm)\" turn=90"),
        "{text}"
    );
    assert_eq!(b["scene"]["pieces"][1]["pinned"], true);
    h.edit(
        "/api/maps/unpin",
        serde_json::json!({"list": list, "id": "p5"}),
    );
    assert!(!std::fs::read_to_string(&file).unwrap().contains("pin "));
    h.edit(
        "/api/maps/pin",
        serde_json::json!({"list": list, "id": "p5", "plate": 1, "x": 5.0, "y": 5.0}),
    );
    h.edit(
        "/api/maps/remove",
        serde_json::json!({"list": list, "id": "p5"}),
    );
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(
        !text.contains("p5") && !text.contains("pin "),
        "the piece goes with its pin: {text}"
    );
    let u = h.edit("/api/maps/unpin-all", serde_json::json!({"list": list}));
    // undo through the print lists' restore, the scene answered at once
    let r = h.ok(
        "/api/lists/restore",
        serde_json::json!({"list": list, "text": TOWN, "docRev": u["docRev"]}),
    );
    assert_eq!(r["scene"]["pieces"].as_array().unwrap().len(), 3);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), TOWN);
    // a new map has a grid, so it opens as a map before a piece is placed
    let (head, v) = h.call(
        "/api/lists/new",
        serde_json::json!({"name": "Table", "map": true}),
    );
    assert!(head.starts_with("HTTP/1.1 200"), "{head} {v}");
    let made = std::fs::read_to_string(v["path"].as_str().unwrap()).unwrap();
    assert!(
        made.contains("grid cols=8 rows=6") && made.contains("about \"A map.\""),
        "{made}"
    );
    assert!(dt_doc::map::decode("m", &made).is_ok());
    // the plates see the placed pieces as pieces to print, one each
    let t = h.wait("town.dmap", "the plates", |t| {
        t["status"] == "ready" && t["plan"]["plates"]["planned"] == true
    });
    assert_eq!(
        t["plan"]["plates"]["layout"][0]["pieces"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
}

#[test]
fn the_first_edit_of_a_file_without_ids_writes_ids_on_every_piece_and_says_so_once() {
    let hand = "dharmatiles version=1\n// by hand\nmap {\n    grid cols=4 rows=3\n    piece \"home:glade\" at=\"(0sq, 0sq)\" rot=0\n    piece p1 \"home:glade\" at=\"(1sq, 0sq)\" rot=0\n    piece \"home:glade\" at=\"(2sq, 0sq)\" rot=0\n}\n";
    let h = start_home("noids", &[("glade.dtile", GLADE), ("hand.dmap", hand)]);
    let list = h.id("hand.dmap");
    // the scene names each piece by the id the first edit will write (p1 is taken: p2, then p1, then p3)
    let t = h.wait("hand.dmap", "the scene", |t| {
        t["plan"]["scene"]["pieces"]
            .as_array()
            .is_some_and(|p| p.len() == 3)
    });
    let ids: Vec<_> = t["plan"]["scene"]["pieces"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["id"].as_str().unwrap().to_owned(),
                p["written"].as_bool().unwrap(),
            )
        })
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
    let b = h.edit(
        "/api/maps/set",
        serde_json::json!({"list": list, "id": "p3", "set": {"rot": 90}}),
    );
    assert!(b["note"].as_str().unwrap().contains("an id"), "{b}");
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
    assert_eq!(b["scene"]["pieces"][2]["written"], true);
    // the next edit has nothing to say
    let b = h.edit(
        "/api/maps/set",
        serde_json::json!({"list": list, "id": "p2", "set": {"rot": 180}}),
    );
    assert!(b.get("note").is_none_or(Value::is_null), "{b}");
}

#[test]
fn the_scene_says_which_pieces_the_shelf_leaves_off_and_how_many_print() {
    let h = start_home("shelf", &[("glade.dtile", GLADE), ("town.dmap", TOWN)]);
    // no inventory: nothing is had, every piece prints
    let t = h.wait("town.dmap", "the scene", |t| {
        t["plan"]["scene"]["pieces"]
            .as_array()
            .is_some_and(|p| p.len() == 3)
    });
    let sc = &t["plan"]["scene"];
    assert_eq!(sc["shelfExists"], false);
    assert_eq!(
        (sc["have"].as_u64(), sc["toPrint"].as_u64()),
        (Some(0), Some(3))
    );
    assert_eq!(sc["pieces"][0]["shelf"]["state"], "need");
    // one good print of the tile's own seed: the first piece of that seed in the order written is the one you have
    h.ok(
        "/api/inventory/have",
        serde_json::json!({"tile": h.id("glade.dtile"), "colour": "single", "on": "2026-11-12"}),
    );
    let t = h.wait("town.dmap", "the shelf", |t| {
        t["plan"]["scene"]["shelfExists"] == true
    });
    let sc = &t["plan"]["scene"];
    let state = |i: usize| {
        sc["pieces"][i]["shelf"]["state"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    assert_eq!(
        (state(0), state(1), state(2)),
        ("have".to_owned(), "need".to_owned(), "need".to_owned()),
        "{sc}"
    );
    assert_eq!(
        (sc["have"].as_u64(), sc["toPrint"].as_u64()),
        (Some(1), Some(2))
    );
    // a piece left off by its own word is neither
    assert_eq!(sc["pieces"][0]["shelf"]["text"], "have");
}

/// The place of every piece of the scene in the answer of an edit.
fn spots(v: &Value) -> Vec<(String, f64, f64)> {
    v["scene"]["pieces"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|p| {
            let at = p["at"].as_array()?;
            Some((
                p["id"].as_str().unwrap().to_owned(),
                at[0].as_f64()?,
                at[1].as_f64()?,
            ))
        })
        .collect()
}

#[test]
fn two_edits_asking_for_the_same_square_end_on_two_squares_and_each_says_where_it_went() {
    let h = start_home("near", &[("glade.dtile", GLADE), ("town.dmap", TOWN)]);
    let list = h.id("town.dmap");
    let tile = h.id("glade.dtile");
    h.wait("town.dmap", "the scene", |t| {
        t["plan"]["scene"]["pieces"]
            .as_array()
            .is_some_and(|p| p.len() == 3)
    });
    let rev = h.rev(&list);
    // both are made against the same revision and both ask for the nearest free square to (10, 10)
    let asks = |h: &Home, rev: u64| {
        h.call(
            "/api/maps/add",
            serde_json::json!({"list": list, "tile": tile, "near": {"x": 10.0, "y": 10.0}, "rot": 0, "docRev": rev}),
        )
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
    assert!(won.0.starts_with("HTTP/1.1 200"), "{} {}", won.0, won.1);
    assert!(lost.0.starts_with("HTTP/1.1 409"), "{} {}", lost.0, lost.1);
    let again = asks(&h, lost.1["docRev"].as_u64().unwrap());
    assert!(
        again.0.starts_with("HTTP/1.1 200"),
        "{} {}",
        again.0,
        again.1
    );
    // the first got the square asked for; the second, made against a map with that square taken, another, and says so
    assert_eq!(won.1["shifted"], 0);
    assert_eq!(again.1["shifted"], 1, "{}", again.1);
    let first = spots(&won.1)
        .into_iter()
        .find(|(id, ..)| *id == won.1["id"].as_str().unwrap())
        .unwrap();
    let second = spots(&again.1)
        .into_iter()
        .find(|(id, ..)| *id == again.1["id"].as_str().unwrap())
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
    h.wait("town.dmap", "the scene", |t| {
        t["plan"]["scene"]["pieces"]
            .as_array()
            .is_some_and(|p| p.len() == 3)
    });
    // one good print of seed 5 is on the shelf: a fresh seed is not that one, nor 9 (p2), in any piece of one batch
    h.ok(
        "/api/inventory/have",
        serde_json::json!({"tile": h.id("glade.dtile"), "colour": "single", "on": "2026-11-12"}),
    );
    let b = h.edit(
        "/api/maps/batch",
        serde_json::json!({"list": list, "edits": [{"id": "p1", "set": {"freshSeed": true}}, {"id": "p3", "set": {"freshSeed": true}}]}),
    );
    let seeds: Vec<u64> = ["p1", "p3"]
        .iter()
        .map(|id| {
            b["scene"]["pieces"]
                .as_array()
                .unwrap()
                .iter()
                .find(|p| p["id"] == *id)
                .unwrap()["seedUsed"]
                .as_u64()
                .unwrap()
        })
        .collect();
    assert_ne!(seeds[0], seeds[1], "a seed repeated within an edit");
    assert!(seeds.iter().all(|s| ![5, 9].contains(s)), "{seeds:?}");
    // both an `at` and a `near`, or a seed and a fresh one, is a mistake
    let (head, _) = h.edit_call(
        "/api/maps/set",
        serde_json::json!({"list": list, "id": "p1", "set": {"at": "(1sq, 1sq)", "near": {"x": 1.0, "y": 1.0}}}),
    );
    assert!(head.starts_with("HTTP/1.1 400"), "{head}");
}

/// What the stream says, for as long as it says anything: the events (parsed) from a stream opened now, until one
/// that `stop` accepts.
fn events_until(h: &Home, stop: impl Fn(&Value) -> bool) -> Vec<Value> {
    let mut c = TcpStream::connect(h.r.addr).unwrap();
    write!(c, "GET /api/events HTTP/1.1\r\nHost: {}\r\n\r\n", h.r.addr).unwrap();
    c.set_read_timeout(Some(Duration::from_secs(60))).unwrap();
    let mut seen = Vec::new();
    for l in BufReader::new(c).lines() {
        let Ok(l) = l else { break };
        let Some(data) = l.strip_prefix("data: ") else {
            continue;
        };
        let v: Value = serde_json::from_str(data).unwrap();
        let done = stop(&v);
        seen.push(v);
        if done {
            break;
        }
    }
    seen
}

#[test]
fn a_change_is_told_as_the_resource_that_changed_and_not_as_the_whole() {
    let h = start_home("resources", &[("glade.dtile", GLADE), ("town.dmap", TOWN)]);
    let first = std::thread::scope(|s| {
        let reader =
            s.spawn(|| events_until(&h, |v| v["type"] == "resource" && v["name"] == "places"));
        // the stream is open once its snapshot is in; then a folder is made: that is the places, and only them
        std::thread::sleep(Duration::from_millis(400));
        let (head, v) = h.call(
            "/api/folders/new",
            serde_json::json!({"root": 0, "dir": "", "name": "dungeon"}),
        );
        assert!(head.starts_with("HTTP/1.1 200"), "{head} {v}");
        reader.join().unwrap()
    });
    assert_eq!(
        first[0]["type"], "snapshot",
        "the stream begins with the whole"
    );
    let told: Vec<&Value> = first[1..]
        .iter()
        .filter(|v| v["type"] == "resource")
        .collect();
    assert_eq!(told.len(), 1, "{told:?}");
    assert_eq!(told[0]["name"], "places");
    assert!(
        told[0]["value"]["folders"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["path"] == "dungeon"),
        "{}",
        told[0]
    );
    assert!(
        first[1..]
            .iter()
            .all(|v| v["type"] != "snapshot" && v["name"] != "inventory" && v["name"] != "list"),
        "a folder does not resend the inventory or the tiles"
    );
    // each resource has a revision of its own that rises when it changes
    assert!(told[0]["resRev"].as_u64().unwrap() >= 1);
}

#[test]
fn a_documents_scene_and_plates_are_of_one_revision_whenever_the_page_is_sent_them() {
    let h = start_home("onerev", &[("glade.dtile", GLADE), ("town.dmap", TOWN)]);
    let tiles_of = |v: &Value| -> Vec<Value> {
        match v["type"].as_str() {
            Some("tile") => vec![v["tile"].clone()],
            Some("snapshot") => v["tiles"].as_array().unwrap().clone(),
            _ => vec![],
        }
    };
    let map = |t: &Value| t["path"].as_str().unwrap_or("").ends_with(".dmap");
    let planned =
        |t: &Value| map(t) && t["status"] == "ready" && t["plan"]["plates"]["planned"] == true;
    // every state of the map the stream sends (the whole first, then each change) names one revision for its scene and its plan
    h.wait("town.dmap", "the plates", |t| {
        t["plan"]["plates"]["planned"] == true
    });
    let seen = events_until(&h, |v| tiles_of(v).iter().any(planned));
    let mut n = 0;
    for v in &seen {
        for t in tiles_of(v)
            .iter()
            .filter(|t| map(t) && !t["plan"]["scene"].is_null())
        {
            n += 1;
            assert_eq!(t["plan"]["scene"]["docRev"], t["plan"]["docRev"]);
        }
    }
    assert!(n > 0, "no state of the map was seen");
}
