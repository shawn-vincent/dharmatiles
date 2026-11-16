//! `dharmatiles inventory` from outside: the log is made at the first line,
//! counted by tile, seed, system and colour, and a list prints only what
//! it lacks.  Tiles are test tiles (`lib:tests/…`) and a copy in a scratch home.

#![expect(
    clippy::unwrap_used,
    reason = "a test: a failing unwrap is the failure"
)]

use std::path::{Path, PathBuf};
use std::process::Command;

// the inventory's state and rows are `dt_service::inventory`'s types: `list --json` writes them, and the tests read the
// output by deserializing them, never by key

// and for `plates --json`, the same way
#[allow(dead_code, reason = "used by one build configuration only")]
#[path = "../src/cli_wire.rs"]
mod cli_wire;

use cli_wire::PlatesChecked;
use dt_lang::Colour;
use dt_service::inventory::{InventoryRow, InventoryState};

const EXE: &str = env!("CARGO_BIN_EXE_dharmatiles");
const FLAT: &str = "lib:tests/ground/1x1-flat";

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("dt-inventory-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn exe(d: &Path) -> Command {
    let mut c = Command::new(EXE);
    // one build cache for the whole run (these tests are not about the cache): a tile is built once
    c.env(
        "DT_CACHE",
        std::env::temp_dir().join(format!("dt-inventory-cache-{}", std::process::id())),
    )
    .env("DT_HOME", d.join("home"))
    .env("DT_SETTINGS", d.join("settings"));
    c
}

fn run(d: &Path, args: &[&str]) -> (i32, String, String) {
    let o = exe(d).args(args).output().unwrap();
    (
        o.status.code().unwrap(),
        String::from_utf8_lossy(&o.stdout).into_owned(),
        String::from_utf8_lossy(&o.stderr).into_owned(),
    )
}

fn inventory_file(d: &Path) -> PathBuf {
    d.join("home").join("inventory.dinv")
}

fn list_json(d: &Path, extra: &[&str]) -> InventoryState {
    // `--json` belongs to `inventory`, before its action
    let mut args = vec!["inventory", "--json", "list"];
    args.extend_from_slice(extra);
    let (code, out, err) = run(d, &args);
    assert_eq!(code, 0, "{err}");
    serde_json::from_str(&out).unwrap()
}

/// The pieces `list --json` gives.
fn listed(d: &Path, extra: &[&str]) -> Vec<InventoryRow> {
    list_json(d, extra).pieces().to_vec()
}

#[test]
fn nothing_is_made_until_the_first_line_is_written() {
    let d = scratch("none");
    let (code, _, err) = run(&d, &["inventory"]);
    assert_eq!(code, 0, "{err}");
    assert!(err.contains("no inventory yet"), "{err}");
    assert!(!inventory_file(&d).exists(), "reading makes nothing");
    assert!(!d.join("home").exists(), "not even the folder");
    let j = list_json(&d, &[]);
    assert!(
        matches!(j, InventoryState::Absent { .. }),
        "no file yet: {j:?}"
    );
    assert!(j.pieces().is_empty());
}

#[test]
fn have_and_gone_add_lines_and_the_counts_follow_by_system_and_colour() {
    let d = scratch("log");
    let (code, _, err) = run(
        &d,
        &[
            "inventory",
            "have",
            FLAT,
            "--colour",
            "multicolour",
            "--count",
            "2",
            "--on",
            "2026-11-09",
            "--note",
            "first batch",
        ],
    );
    assert_eq!(code, 0, "{err}");
    let text = std::fs::read_to_string(inventory_file(&d)).unwrap();
    assert!(text.starts_with("dharmatiles version="), "{text}");
    assert_eq!(
        text.matches("print \"lib:tests/ground/1x1-flat\"").count(),
        2,
        "{text}"
    );
    assert!(
        text.contains("system=db colour=multicolour result=ok on=\"2026-11-09\" hash=\""),
        "{text}"
    );
    assert!(text.contains("note=\"first batch\""), "{text}");
    // another colour and another system are other pieces
    run(&d, &["inventory", "have", FLAT, "--colour", "single"]);
    run(
        &d,
        &[
            "inventory",
            "have",
            FLAT,
            "--system",
            "ol",
            "--colour",
            "multicolour",
            "--on",
            "2026-11-10",
        ],
    );
    let pieces = listed(&d, &["--system", "all"]);
    let have = |sys: &str, colour: Colour| -> u32 {
        pieces
            .iter()
            .filter(|p| p.system.word() == sys && p.colour == colour)
            .map(|p| p.have)
            .sum()
    };
    assert_eq!(
        (
            have("db", Colour::Multicolour),
            have("db", Colour::Single),
            have("ol", Colour::Multicolour)
        ),
        (2, 1, 1)
    );
    // the default view is one system, and says there is another
    let (_, _, err) = run(&d, &["inventory", "list"]);
    assert!(
        err.contains("DungeonBlocks") && err.contains("have 2") && err.contains("--system all"),
        "{err}"
    );
    // a loss takes one away, in the colour named; the log keeps the print lines
    let (code, _, err) = run(
        &d,
        &[
            "inventory",
            "gone",
            FLAT,
            "--colour",
            "multicolour",
            "--note",
            "dropped",
        ],
    );
    assert_eq!(code, 0, "{err}");
    let pieces = listed(&d, &["--colour", "multicolour"]);
    assert_eq!((pieces[0].have, pieces[0].gone), (1, 1));
    let text = std::fs::read_to_string(inventory_file(&d)).unwrap();
    assert_eq!(
        text.matches("print ").count(),
        4,
        "nothing is edited away: {text}"
    );
    assert!(
        text.contains("gone \"lib:tests/ground/1x1-flat\""),
        "{text}"
    );
    // nothing to take away is refused, and says what to look at
    let (code, _, err) = run(
        &d,
        &[
            "inventory",
            "gone",
            FLAT,
            "--colour",
            "single",
            "--system",
            "ol",
        ],
    );
    assert_eq!(code, 1);
    assert!(err.contains("you have no"), "{err}");
    // a failed print is kept and does not count
    let (code, _, _) = run(
        &d,
        &["inventory", "have", FLAT, "--colour", "single", "--failed"],
    );
    assert_eq!(code, 0);
    let pieces = listed(&d, &["--colour", "single"]);
    assert_eq!((pieces[0].have, pieces[0].failed), (1, 1));
}

#[test]
fn a_list_prints_only_what_the_inventory_lacks_and_all_ignores_it() {
    let d = scratch("plates");
    let l = d.join("kit.dmap");
    std::fs::write(
        &l,
        format!("dharmatiles version=\"1.0.0\"\nmap {{\n piece \"{FLAT}\" count=4\n}}\n"),
    )
    .unwrap();
    // the default mode of a list for the AMS printer is multicolour
    run(
        &d,
        &[
            "inventory",
            "have",
            FLAT,
            "--colour",
            "multicolour",
            "--count",
            "3",
        ],
    );
    let (code, out, err) = run(&d, &["plates", l.to_str().unwrap(), "--check", "--json"]);
    assert_eq!(code, 0, "{err}");
    let j: PlatesChecked = serde_json::from_str(&out).unwrap();
    let k = &j.plan.kinds[0];
    assert_eq!((k.want, k.have, k.count), (4, 3, 1), "{j:?}");
    let (_, _, err) = run(&d, &["plates", l.to_str().unwrap(), "--check"]);
    assert!(
        err.contains("3 pieces are already in your inventory") && err.contains("1 to print"),
        "{err}"
    );
    // --all prints every count
    let (_, out, _) = run(
        &d,
        &["plates", l.to_str().unwrap(), "--check", "--json", "--all"],
    );
    let j: PlatesChecked = serde_json::from_str(&out).unwrap();
    assert_eq!(j.plan.kinds[0].count, 4);
    // single colour: the multicolour prints are another piece, so all four print, and the plan says nothing is held
    let (_, out, _) = run(
        &d,
        &[
            "plates",
            l.to_str().unwrap(),
            "--check",
            "--json",
            "--colour",
            "single",
        ],
    );
    let j: PlatesChecked = serde_json::from_str(&out).unwrap();
    assert_eq!(j.plan.kinds[0].count, 4);
    assert_eq!(j.plan.kinds[0].other_colour, 3);
    // all of it on the shelf: refused with the way out
    run(&d, &["inventory", "have", FLAT, "--colour", "multicolour"]);
    let (code, _, err) = run(&d, &["plates", l.to_str().unwrap(), "--check"]);
    assert_eq!(code, 1);
    assert!(
        err.contains("already in your inventory") && err.contains("--all"),
        "{err}"
    );
    let (code, _, err) = run(&d, &["plates", l.to_str().unwrap(), "--check", "--all"]);
    assert_eq!(code, 0, "{err}");
}

#[test]
fn a_file_with_mistakes_is_shown_at_its_text_and_never_added_to() {
    let d = scratch("bad");
    std::fs::create_dir_all(d.join("home")).unwrap();
    let bad = "dharmatiles version=\"1.0.0\"\ninventory {\n    print \"lib:tests/ground/1x1-flat\" system=db colour=multicolour result=ok on=\"2026-11-09\"\n}\n";
    std::fs::write(inventory_file(&d), bad).unwrap();
    let (code, _, err) = run(&d, &["inventory", "list"]);
    assert_eq!(code, 1);
    assert!(
        err.contains("needs `seed`") && err.contains("inventory.dinv"),
        "{err}"
    );
    let (code, _, err) = run(&d, &["inventory", "have", FLAT, "--colour", "multicolour"]);
    assert_eq!(code, 1);
    assert!(err.contains("needs `seed`"), "{err}");
    assert_eq!(
        std::fs::read_to_string(inventory_file(&d)).unwrap(),
        bad,
        "not a byte changed"
    );
    // a list that looks in it does not lay out against a log it cannot read, unless --all
    let l = d.join("kit.dmap");
    std::fs::write(
        &l,
        format!("dharmatiles version=\"1.0.0\"\nmap {{\n piece \"{FLAT}\"\n}}\n"),
    )
    .unwrap();
    let (code, _, err) = run(&d, &["plates", l.to_str().unwrap(), "--check"]);
    assert_eq!(code, 1);
    assert!(
        err.contains("needs `seed`") && err.contains("--all"),
        "{err}"
    );
    let (code, _, err) = run(&d, &["plates", l.to_str().unwrap(), "--check", "--all"]);
    assert_eq!(code, 0, "{err}");
}

#[test]
fn a_tile_that_moved_is_followed_by_a_renamed_line() {
    let d = scratch("relink");
    let home = d.join("home");
    std::fs::create_dir_all(&home).unwrap();
    let src =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../library/tests/ground/1x1-flat.dtile");
    std::fs::copy(&src, home.join("meadow.dtile")).unwrap();
    let (code, _, err) = run(
        &d,
        &["inventory", "have", "home:meadow", "--colour", "single"],
    );
    assert_eq!(code, 0, "{err}");
    std::fs::rename(home.join("meadow.dtile"), home.join("glade.dtile")).unwrap();
    // the path is not there any more: the list says so
    let pieces = listed(&d, &[]);
    assert_eq!(pieces[0].tile, "home:meadow");
    assert!(pieces[0].missing);
    let (code, _, err) = run(&d, &["inventory", "relink", "home:meadow", "home:glade"]);
    assert_eq!(code, 0, "{err}");
    let pieces = listed(&d, &[]);
    assert_eq!(pieces[0].tile, "home:glade");
    assert_eq!(pieces[0].have, 1);
    assert!(!pieces[0].missing);
    let text = std::fs::read_to_string(inventory_file(&d)).unwrap();
    assert!(
        text.contains("renamed \"home:meadow\" to=\"home:glade\""),
        "{text}"
    );
    assert!(
        text.contains("print \"home:meadow\""),
        "the log stays a log: {text}"
    );
    // a path the log never heard of is refused
    let (code, _, err) = run(&d, &["inventory", "relink", "home:nothing", "home:glade"]);
    assert_eq!(code, 1);
    assert!(err.contains("has nothing for"), "{err}");
    // edited since printing: information only, the piece still counts
    let mut t = std::fs::read_to_string(home.join("glade.dtile")).unwrap();
    t.push_str("\n// edited\n");
    std::fs::write(home.join("glade.dtile"), t).unwrap();
    let pieces = listed(&d, &[]);
    assert!(pieces[0].edited);
    assert_eq!(pieces[0].have, 1);
}

#[test]
fn the_old_words_for_colour_are_refused_and_fix_writes_the_new_ones() {
    let d = scratch("oldcolour");
    // an inventory with the old `multi` and a print list with the old `mode=`
    let inv = d.join("old.dinv");
    let list = d.join("old.dmap");
    std::fs::write(
        &inv,
        "dharmatiles version=1\ninventory {\n print \"lib:tests/ground/1x1-flat\" seed=1 system=db colour=multi result=ok on=\"2026-11-09\"\n}\n",
    )
    .unwrap();
    std::fs::write(
        &list,
        format!("dharmatiles version=1\nmap {{\n piece \"{FLAT}\"\n}}\nplates mode=multicolour\n"),
    )
    .unwrap();
    // `fix` shows, then `--write` writes, the new words
    let (code, out, err) = run(&d, &["fix", inv.to_str().unwrap(), list.to_str().unwrap()]);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("nothing written"), "{out}");
    let (code, _, err) = run(
        &d,
        &[
            "fix",
            "--write",
            inv.to_str().unwrap(),
            list.to_str().unwrap(),
        ],
    );
    assert_eq!(code, 0, "{err}");
    assert!(
        std::fs::read_to_string(&inv)
            .unwrap()
            .contains("colour=multicolour")
    );
    assert!(
        std::fs::read_to_string(&list)
            .unwrap()
            .contains("plates colour=multicolour")
    );
    // the flag `--mode` is refused with its replacement, and `--colour multi` with the word
    let (code, _, err) = run(
        &d,
        &[
            "plates",
            list.to_str().unwrap(),
            "--check",
            "--mode",
            "single",
        ],
    );
    assert_ne!(code, 0);
    assert!(err.contains("`--mode` is now `--colour`"), "{err}");
    let (code, _, err) = run(&d, &["inventory", "list", "--colour", "multi"]);
    assert_ne!(code, 0);
    assert!(err.contains("write `multicolour`"), "{err}");
}
