//! The places and what is done in them: every call refuses what is outside a
//! place the user owns, and does no more than it says.

use super::*;
use dt_eval::Options;

const TILE: &str =
    "dharmatiles version=1\ntile size=\"2x1\" {\n    stack {\n        dirt\n    }\n}\n";

/// A scratch folder of its own, empty.
fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("dt-places-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    // the real path: a temp dir may sit behind a link (macOS /var -> /private/var)
    std::fs::canonicalize(&d).unwrap()
}

/// A server over `home` (My tiles), with settings and a Trash of its own beside it.
fn serve(name: &str) -> (Shared, PathBuf, PathBuf, PathBuf) {
    let top = scratch(name);
    let home = top.join("home");
    std::fs::create_dir_all(&home).unwrap();
    let roots = vec![
        Root::new("My tiles", RootKind::Dir(home.clone())).with_role(Role::Home),
        Root::new("Library", RootKind::Library("tiles/".to_owned())),
    ];
    let settings = top.join("settings");
    let trash = top.join("trash");
    let shared = Shared::with_roots(roots, &[], &Options::default())
        .with_settings(settings.clone())
        .with_trash(trash.clone());
    (shared, home, settings, trash)
}

fn ok(r: &Reply) -> &Value {
    assert_eq!(r.0, "200 OK", "{}", r.1);
    &r.1
}

fn refused(r: &Reply, status: &str) {
    assert!(
        r.0.starts_with(status),
        "wanted {status}, got {} {}",
        r.0,
        r.1
    );
}

/// A call as the page makes it: a JSON body to the typed route, and its reply.
fn call(shared: &Shared, path: &str, v: &Value) -> Reply {
    match super::super::edit::post(shared, path, v.to_string().as_bytes()) {
        super::super::route::Reply::Json(status, body) => (status, body),
        _ => panic!("not a JSON reply"),
    }
}

fn snap_json(shared: &Shared) -> Value {
    serde_json::to_value(shared.snapshot()).unwrap()
}

fn add_place(shared: &Shared) -> Reply {
    call(shared, "/api/places/add", &json!({}))
}
fn remove_place(shared: &Shared, v: &Value) -> Reply {
    call(shared, "/api/places/remove", v)
}
fn new_folder(shared: &Shared, v: &Value) -> Reply {
    call(shared, "/api/folders/new", v)
}
fn rename_folder(shared: &Shared, v: &Value) -> Reply {
    call(shared, "/api/folders/rename", v)
}
fn trash_folder(shared: &Shared, v: &Value) -> Reply {
    call(shared, "/api/folders/trash", v)
}
fn undo_trash(shared: &Shared, v: &Value) -> Reply {
    call(shared, "/api/trash/undo", v)
}

/// A call under a tile that need not be listed: the handler, given the path.
fn tile_ref(path: &str) -> super::super::api::TileRef {
    super::super::api::TileRef {
        id: tile_id(path),
        path: path.to_owned(),
        files: BTreeMap::new(),
    }
}
fn trash_tile(shared: &Shared, path: &str) -> Reply {
    super::super::api::reply(super::trash_tile(shared, &tile_ref(path), Nothing {}))
}
fn duplicate_tile(shared: &Shared, path: &str) -> Reply {
    super::super::api::reply(super::duplicate_tile(shared, &tile_ref(path), Nothing {}))
}
fn move_tile(shared: &Shared, path: &str, v: &Value) -> Reply {
    match serde_json::from_value::<TileMoveReq>(v.clone()) {
        Ok(req) => super::super::api::reply(super::move_tile(shared, &tile_ref(path), req)),
        Err(e) => super::super::fail::refused("400 Bad Request", e.to_string()),
    }
}

fn listed_names(shared: &Shared, root: usize) -> Vec<String> {
    let mut v: Vec<String> = snap_json(shared)["tiles"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|t| t["root"] == root)
        .map(|t| t["path"].as_str().unwrap().to_owned())
        .collect();
    v.sort();
    v
}

fn folders_of(shared: &Shared, root: usize) -> Vec<String> {
    snap_json(shared)["folders"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["root"] == root)
        .map(|f| f["path"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn a_tile_document_says_its_size() {
    assert_eq!(size_in(TILE).as_deref(), Some("2x1"));
    assert_eq!(
        size_in("// c\ntile seed=1 size=3x2 { }\n").as_deref(),
        Some("3x2")
    );
    assert_eq!(size_in("grass meadow { }\n"), None);
    assert_eq!(size_in("tile seed=4 { }\n"), None);
}

#[test]
fn the_settings_keep_the_added_places_and_a_bad_file_is_told() {
    let d = scratch("settings").join("s");
    assert_eq!(
        load_places(&d).unwrap(),
        Vec::<PathBuf>::new(),
        "no file yet: none"
    );
    let a = PathBuf::from("/a/b");
    save_places(&d, &[a.clone(), PathBuf::from("/c")]).unwrap();
    assert_eq!(load_places(&d).unwrap(), vec![a, PathBuf::from("/c")]);
    std::fs::write(d.join("places.json"), "{ not json").unwrap();
    let e = load_places(&d).unwrap_err();
    assert!(e.contains("places.json") && e.contains("not valid"), "{e}");
    std::fs::write(d.join("places.json"), "{\"places\": [3]}").unwrap();
    assert!(
        load_places(&d).is_err(),
        "a place that is not a path is told, not skipped"
    );
}

#[test]
fn the_chooser_adds_a_place_that_is_kept_and_a_cancel_changes_nothing() {
    let (shared, _home, settings, _) = serve("pick");
    let outside = scratch("pick-out").join("Dungeon dressing");
    std::fs::create_dir_all(outside.join("sub")).unwrap();
    std::fs::write(outside.join("sub/x.dtile"), TILE).unwrap();

    // a cancel is an answer: nothing is added, nothing is said
    let cancelled = Shared::with_roots(shared.roots(), &[], &Options::default())
        .with_settings(settings.clone())
        .with_picker(Picker::Fixed(None));
    let r = add_place(&cancelled);
    assert_eq!(ok(&r)["cancelled"], true);
    assert!(!settings.join("places.json").exists());
    assert_eq!(cancelled.roots().len(), 2);

    let shared = shared.with_picker(Picker::Fixed(Some(outside.clone())));
    let r = add_place(&shared);
    let body = ok(&r);
    assert_eq!(body["added"], true);
    let i = usize::try_from(body["root"].as_u64().unwrap()).unwrap();
    assert_eq!(body["label"], "Dungeon dressing");
    let snap = snap_json(&shared);
    assert_eq!(snap["roots"][i]["role"], "added");
    assert_eq!(snap["roots"][i]["writable"], true);
    assert_eq!(folders_of(&shared, i), vec!["sub".to_owned()]);
    assert_eq!(listed_names(&shared, i).len(), 1);
    assert_eq!(load_places(&settings).unwrap(), vec![outside.clone()]);
    // its tile's size is read without a build
    let t = snap["tiles"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["root"] == i)
        .unwrap();
    assert_eq!(t["size"], "2x1");

    // the same folder, one inside it, and one that holds it: refused, with the reason
    refused(&add_place(&shared), "409");
    let inner = Shared::with_roots(shared.roots(), &[], &Options::default())
        .with_settings(settings.clone())
        .with_picker(Picker::Fixed(Some(outside.join("sub"))));
    refused(&add_place(&inner), "409");
    let holder = Shared::with_roots(shared.roots(), &[], &Options::default())
        .with_settings(settings.clone())
        .with_picker(Picker::Fixed(Some(outside.parent().unwrap().to_path_buf())));
    refused(&add_place(&holder), "409");
    // a path that is not a folder
    let file = Shared::with_roots(vec![], &[], &Options::default())
        .with_settings(settings.clone())
        .with_picker(Picker::Fixed(Some(outside.join("sub/x.dtile"))));
    refused(&add_place(&file), "400");
    assert_eq!(
        load_places(&settings).unwrap().len(),
        1,
        "a refusal keeps nothing"
    );

    // removing it takes it off the list and the settings, and the folder stays as it was
    let r = remove_place(&shared, &json!({ "root": i }));
    ok(&r);
    assert_eq!(shared.roots().len(), 2);
    assert!(load_places(&settings).unwrap().is_empty());
    assert!(outside.join("sub/x.dtile").exists());
    assert!(listed_names(&shared, i).is_empty());
    // My tiles and the library stay
    refused(&remove_place(&shared, &json!({ "root": 0 })), "403");
    refused(&remove_place(&shared, &json!({ "root": 1 })), "403");
    refused(&remove_place(&shared, &json!({ "root": 9 })), "404");
}

#[test]
fn removing_a_place_moves_the_ones_after_it_down() {
    let (shared, home, settings, _) = serve("reindex");
    let a = scratch("reindex-a");
    let b = scratch("reindex-b");
    std::fs::write(b.join("b.dtile"), TILE).unwrap();
    std::fs::write(home.join("h.dtile"), TILE).unwrap();
    let shared = shared.with_picker(Picker::Fixed(Some(a.clone())));
    let ra = ok(&add_place(&shared))["root"].as_u64().unwrap();
    let shared = shared.with_picker(Picker::Fixed(Some(b.clone())));
    let rb = ok(&add_place(&shared))["root"].as_u64().unwrap();
    assert_eq!((ra, rb), (2, 3));
    ok(&remove_place(&shared, &json!({ "root": ra })));
    // b is place 2 now, and its tile still says so
    assert_eq!(shared.roots()[2].dir(), Some(b.as_path()));
    assert_eq!(listed_names(&shared, 2).len(), 1);
    assert_eq!(load_places(&settings).unwrap(), vec![b]);
    assert_eq!(listed_names(&shared, 0).len(), 1);
}

#[test]
fn an_answer_names_the_revision_it_was_told_at_and_carries_no_state() {
    let (shared, _, _, _) = serve("rev");
    let before = shared.told();
    let r = new_folder(&shared, &json!({ "root": 0, "dir": "", "name": "dungeon" }));
    let body = ok(&r);
    assert!(
        body.get("snapshot").is_none(),
        "the change reaches the page on the stream, not in the answer: {body}"
    );
    let rev = body["rev"].as_u64().expect("the answer names a revision");
    assert!(rev > before, "the folder was told after {before}: {rev}");
    assert_eq!(rev, shared.told(), "nothing was left to tell");
}

#[test]
fn a_folder_is_made_renamed_and_trashed_inside_a_place_only() {
    let (shared, home, _, trash) = serve("folders");
    let r = new_folder(&shared, &json!({ "root": 0, "dir": "", "name": "dungeon" }));
    assert_eq!(ok(&r)["dir"], "dungeon");
    assert!(home.join("dungeon").is_dir());
    assert_eq!(
        folders_of(&shared, 0),
        vec!["dungeon".to_owned()],
        "listed at once"
    );
    ok(&new_folder(
        &shared,
        &json!({ "root": 0, "dir": "dungeon", "name": "combo" }),
    ));
    assert_eq!(
        folders_of(&shared, 0),
        vec!["dungeon".to_owned(), "dungeon/combo".to_owned()]
    );
    // never over what is there
    refused(
        &new_folder(&shared, &json!({ "root": 0, "dir": "", "name": "dungeon" })),
        "409",
    );
    // a name is a name: slashes become dashes, nothing hidden or empty
    let r = new_folder(&shared, &json!({ "root": 0, "dir": "", "name": "a/b" }));
    assert_eq!(ok(&r)["dir"], "a-b");
    refused(
        &new_folder(&shared, &json!({ "root": 0, "dir": "", "name": "   " })),
        "400",
    );
    let r = new_folder(&shared, &json!({ "root": 0, "dir": "", "name": ".." }));
    refused(&r, "400");
    refused(
        &new_folder(&shared, &json!({ "root": 0, "dir": "", "name": "out" })),
        "400",
    );

    let r = rename_folder(
        &shared,
        &json!({ "root": 0, "dir": "dungeon/combo", "name": "fights" }),
    );
    assert_eq!(ok(&r)["dir"], "dungeon/fights");
    assert!(home.join("dungeon/fights").is_dir() && !home.join("dungeon/combo").exists());
    // another folder's name is free in this one
    ok(&rename_folder(
        &shared,
        &json!({ "root": 0, "dir": "dungeon/fights", "name": "a-b" }),
    ));
    ok(&rename_folder(
        &shared,
        &json!({ "root": 0, "dir": "dungeon/a-b", "name": "fights" }),
    ));
    refused(
        &rename_folder(
            &shared,
            &json!({ "root": 0, "dir": "dungeon", "name": "a-b" }),
        ),
        "409",
    );
    refused(
        &rename_folder(&shared, &json!({ "root": 0, "dir": "", "name": "x" })),
        "400",
    );

    // trash: into the Trash, with an Undo that puts it back, and not over something there again
    let r = trash_folder(&shared, &json!({ "root": 0, "dir": "dungeon" }));
    let undo = ok(&r)["undo"].clone();
    assert!(!home.join("dungeon").exists());
    assert!(trash.join("dungeon/fights").is_dir());
    assert!(!folders_of(&shared, 0).contains(&"dungeon".to_owned()));
    ok(&undo_trash(&shared, &json!({ "undo": undo })));
    assert!(home.join("dungeon/fights").is_dir());
    refused(&undo_trash(&shared, &json!({ "undo": undo })), "409");
    refused(&undo_trash(&shared, &json!({ "undo": 99 })), "404");
    // the place itself is not trashed
    refused(
        &trash_folder(&shared, &json!({ "root": 0, "dir": "" })),
        "400",
    );
}

#[test]
fn a_call_refuses_a_place_it_may_not_touch_and_a_path_that_leaves_one() {
    let (shared, home, _, _) = serve("refuse");
    // the library is read only, whatever is asked; so is a place that is not there
    refused(
        &new_folder(&shared, &json!({ "root": 1, "dir": "", "name": "x" })),
        "403",
    );
    refused(
        &new_folder(&shared, &json!({ "root": 7, "dir": "", "name": "x" })),
        "404",
    );
    refused(
        &new_folder(&shared, &json!({ "root": "0", "dir": "", "name": "x" })),
        "400",
    );
    // names that climb out, start at the top, hide or are empty
    for dir in [
        "..", "../x", "a/../..", "/etc", "/", ".hidden", "a//b", "a\\b", "out", "a:b",
    ] {
        let r = new_folder(&shared, &json!({ "root": 0, "dir": dir, "name": "x" }));
        assert!(
            r.0.starts_with("400") || r.0.starts_with("404"),
            "{dir}: {} {}",
            r.0,
            r.1
        );
    }
    let outside = scratch("refuse-out");
    std::fs::write(outside.join("secret.dtile"), TILE).unwrap();
    assert!(!outside.join("x").exists());

    // a link inside the place that leads out of it is outside
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&outside, home.join("escape")).unwrap();
        let r = new_folder(&shared, &json!({ "root": 0, "dir": "escape", "name": "x" }));
        refused(&r, "403");
        assert!(
            !outside.join("x").exists(),
            "nothing was made outside the place"
        );
        refused(
            &trash_folder(&shared, &json!({ "root": 0, "dir": "escape" })),
            "403",
        );
        refused(
            &rename_folder(&shared, &json!({ "root": 0, "dir": "escape", "name": "y" })),
            "403",
        );
        assert!(outside.join("secret.dtile").exists());
        // and a tile in it is not a tile of the place
        let through = home.join("escape/secret.dtile");
        refused(&trash_tile(&shared, &through.to_string_lossy()), "400");
        refused(&duplicate_tile(&shared, &through.to_string_lossy()), "400");
        refused(
            &move_tile(
                &shared,
                &through.to_string_lossy(),
                &json!({ "root": 0, "dir": "" }),
            ),
            "400",
        );
        assert!(outside.join("secret.dtile").exists());
    }
}

#[test]
fn a_tile_is_moved_copied_and_trashed_only_when_it_is_one_of_the_places() {
    let (shared, home, _, trash) = serve("tiles");
    std::fs::create_dir_all(home.join("ponds")).unwrap();
    std::fs::write(home.join("Pond.dtile"), TILE).unwrap();
    std::fs::write(home.join("notes.txt"), "x").unwrap();
    shared.rescan();
    let pond = home.join("Pond.dtile").to_string_lossy().into_owned();

    // move into a folder: the file goes, the list says so at once, nothing is overwritten
    let r = move_tile(&shared, &pond, &json!({ "root": 0, "dir": "ponds" }));
    let moved = home.join("ponds/Pond.dtile");
    assert_eq!(ok(&r)["path"], moved.to_string_lossy().as_ref());
    assert!(moved.exists() && !Path::new(&pond).exists());
    assert_eq!(
        listed_names(&shared, 0),
        vec![moved.to_string_lossy().into_owned()]
    );
    std::fs::write(home.join("Pond.dtile"), TILE).unwrap();
    shared.rescan();
    refused(
        &move_tile(&shared, &pond, &json!({ "root": 0, "dir": "ponds" })),
        "409",
    );
    assert!(Path::new(&pond).exists(), "the clash moved nothing");
    // a folder that is not there, one that climbs, the library
    refused(
        &move_tile(&shared, &pond, &json!({ "root": 0, "dir": "nope" })),
        "404",
    );
    refused(
        &move_tile(&shared, &pond, &json!({ "root": 0, "dir": "../" })),
        "400",
    );
    refused(
        &move_tile(&shared, &pond, &json!({ "root": 1, "dir": "" })),
        "403",
    );
    // to its own folder: nothing to do
    ok(&move_tile(&shared, &pond, &json!({ "root": 0, "dir": "" })));

    // a copy beside it, then another
    let r = duplicate_tile(&shared, &pond);
    assert_eq!(
        ok(&r)["path"],
        home.join("Pond copy.dtile").to_string_lossy().as_ref()
    );
    assert_eq!(
        std::fs::read_to_string(home.join("Pond copy.dtile")).unwrap(),
        TILE
    );
    let r = duplicate_tile(&shared, &pond);
    assert_eq!(
        ok(&r)["path"],
        home.join("Pond copy 2.dtile").to_string_lossy().as_ref()
    );

    // the Trash, never a delete; and the Undo
    let r = trash_tile(&shared, &pond);
    let undo = ok(&r)["undo"].clone();
    assert!(!Path::new(&pond).exists() && trash.join("Pond.dtile").exists());
    assert!(shared.summary(&tile_id(&pond)).is_none());
    let r = undo_trash(&shared, &json!({ "undo": undo }));
    assert!(ok(&r)["id"].is_string(), "{r:?}");
    assert!(Path::new(&pond).exists() && !trash.join("Pond.dtile").exists());
    // trashing again beside what is in the Trash already: its own name
    std::fs::write(trash.join("Pond.dtile"), "an older one").unwrap();
    ok(&trash_tile(&shared, &pond));
    assert!(trash.join("Pond 2.dtile").exists());
    assert_eq!(
        std::fs::read_to_string(trash.join("Pond.dtile")).unwrap(),
        "an older one"
    );

    // what is not a tile document, not in a place, or the library
    for p in [
        home.join("notes.txt").to_string_lossy().into_owned(),
        scratch("tiles-out")
            .join("x.dtile")
            .to_string_lossy()
            .into_owned(),
        format!("{}/../x.dtile", home.display()),
        "lib:tiles/ground/1x1-flat.dtile".to_owned(),
    ] {
        refused(&trash_tile(&shared, &p), "400");
        refused(&duplicate_tile(&shared, &p), "400");
        refused(
            &move_tile(&shared, &p, &json!({ "root": 0, "dir": "" })),
            "400",
        );
    }
    assert!(home.join("notes.txt").exists());
}

#[test]
fn without_a_trash_nothing_is_deleted() {
    let top = scratch("notrash");
    let home = top.join("home");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(home.join("a.dtile"), TILE).unwrap();
    let shared = Shared::with_roots(
        vec![Root::new("My tiles", RootKind::Dir(home.clone())).with_role(Role::Home)],
        &[],
        &Options::default(),
    );
    let r = trash_tile(&shared, &home.join("a.dtile").to_string_lossy());
    assert!(r.0.starts_with("500"), "{r:?}");
    assert!(home.join("a.dtile").exists());
}

#[test]
fn a_new_tile_can_be_made_in_a_folder_and_not_outside_a_place() {
    let (shared, home, _, _) = serve("newin");
    std::fs::create_dir_all(home.join("ponds")).unwrap();
    shared.rescan();
    let r = super::super::edit::post(
        &shared,
        "/api/new",
        json!({ "name": "Marsh", "size": "1x1", "root": 0, "dir": "ponds" })
            .to_string()
            .as_bytes(),
    );
    let super::super::route::Reply::Json(status, body) = r else {
        panic!("not a JSON reply")
    };
    assert_eq!(status, "200 OK", "{body}");
    assert!(home.join("ponds/Marsh.dtile").exists());
    let r = super::super::edit::post(
        &shared,
        "/api/new",
        json!({ "name": "Marsh", "root": 0, "dir": "../x" })
            .to_string()
            .as_bytes(),
    );
    let super::super::route::Reply::Json(status, _) = r else {
        panic!("not a JSON reply")
    };
    assert!(status.starts_with("400"), "{status}");
    let r = super::super::edit::post(
        &shared,
        "/api/new",
        json!({ "name": "Marsh", "root": 1, "dir": "" })
            .to_string()
            .as_bytes(),
    );
    let super::super::route::Reply::Json(status, _) = r else {
        panic!("not a JSON reply")
    };
    assert!(
        status.starts_with("403"),
        "the library takes no new tile: {status}"
    );
}

#[test]
fn the_folder_calls_are_routed_and_need_the_token() {
    let (shared, home, _, _) = serve("routed");
    let r = super::super::edit::post(
        &shared,
        "/api/folders/new",
        json!({ "root": 0, "dir": "", "name": "x" })
            .to_string()
            .as_bytes(),
    );
    assert!(matches!(r, super::super::route::Reply::Json("200 OK", _)));
    assert!(home.join("x").is_dir());
    // the token is the route's, for every POST under /api/ (see `route`): no call is outside it
    let req = super::super::http::Request::build("POST", "/api/places/add", None, b"{}");
    assert!(matches!(
        super::super::route::route(&shared, &req),
        super::super::route::Reply::Json("403 Forbidden", _)
    ));
}

#[test]
fn the_kinds_a_folder_lists_are_one_table() {
    use super::super::roots::{KINDS, kind_of};
    assert_eq!(
        KINDS.iter().map(|k| k.ext).collect::<Vec<_>>(),
        ["dtile", "dmap", "dinv", "stl"]
    );
    assert!(kind_of("a.DTILE").is_some_and(|k| k.actions));
    assert!(
        kind_of("a.stl").is_some_and(|k| !k.actions),
        "an .stl is listed, not managed"
    );
    assert!(kind_of("notes.txt").is_none());
    assert!(
        kind_of("dtile").is_none(),
        "an extension needs a name before it"
    );
}
