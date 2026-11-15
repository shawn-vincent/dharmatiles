//! The storage seam: a save, a rename and a trash with its undo, run through the owner and the documents with no
//! disk at all.  What the editor does to files goes through [`Storage`], so a memory store stands in for the disk.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::super::docs::Docs;
use super::super::owner::Command;
use dt_eval::Options;
use dt_service::storage::{MemStorage, Storage};

const TILE: &str = "tile size=\"1x1\" seed=1 {\n    stack {\n        dirt\n    }\n}\n";

#[test]
fn a_save_a_rename_and_a_trash_then_undo_need_no_disk() {
    let a = PathBuf::from("/memory/My tiles/a.dtile");
    let b = PathBuf::from("/memory/My tiles/b.dtile");
    let bin = PathBuf::from("/memory/Trash");
    let mem = Arc::new(MemStorage::with(a.to_str().unwrap(), TILE));
    let owner = super::super::owner::start(
        Vec::new(),
        Vec::new(),
        Options::default(),
        None,
        "test".into(),
        mem.clone(),
    );
    let docs = Docs::telling(owner.clone(), mem.clone());

    // a save: a revision, and the text is in the store (nothing is on the disk)
    let path = a.to_str().unwrap();
    let (_, first) = docs.read(path).expect("the tile reads");
    let Ok((edited, ())) = docs.edit(path, Some(first), |t| Ok((format!("{t}// saved\n"), ())))
    else {
        panic!("the save was refused");
    };
    assert_eq!(edited.rev, first + 1);
    assert_eq!(mem.read_text(&a).unwrap(), format!("{TILE}// saved\n"));
    assert!(!Path::new("/memory").exists(), "the test touched the disk");

    // a rename never goes over a tile
    mem.write_text(&b, "other").unwrap();
    let clash = mem.move_no_clobber(&a, &b).unwrap_err();
    assert_eq!(clash.kind(), std::io::ErrorKind::AlreadyExists);
    let c = PathBuf::from("/memory/My tiles/c.dtile");
    mem.move_no_clobber(&a, &c).unwrap();
    assert!(!mem.exists(&a) && mem.exists(&c));

    // trashed: the owner numbers it, and the undo puts it back where it was
    let to = mem.trash(&c, &bin).unwrap();
    assert_eq!(to, bin.join("c.dtile"));
    assert!(!mem.exists(&c));
    let undo = owner.ask(|r| Command::Trash(c.clone(), to.clone(), r));
    let held = owner.ask(|r| Command::Trashed(undo, r));
    assert_eq!(held, Some((c.clone(), to.clone())));
    mem.restore(&to, &c).unwrap();
    assert_eq!(mem.read_text(&c).unwrap(), format!("{TILE}// saved\n"));
    assert!(!mem.exists(&to));

    // a second trash of the same name takes a free one beside it
    mem.write_text(&c, "again").unwrap();
    mem.write_text(&bin.join("c.dtile"), "kept").unwrap();
    assert_eq!(mem.trash(&c, &bin).unwrap(), bin.join("c 2.dtile"));
}

#[test]
fn a_scan_a_new_file_and_a_delete_need_no_disk() {
    use super::super::roots::{Root, RootKind};
    use super::super::storage::EntryKind;

    let home = PathBuf::from("/memory/scan/My tiles");
    let mem = MemStorage::with(home.join("a.dtile").to_str().unwrap(), TILE);
    mem.write_text(&home.join("sub/b.dtile"), TILE).unwrap();
    // a part is not a tile; a hidden folder and build output are not listed
    mem.write_text(&home.join("part.dtile"), "define x { }\n")
        .unwrap();
    mem.write_text(&home.join(".hidden/c.dtile"), TILE).unwrap();
    mem.write_text(&home.join("out/d.dtile"), TILE).unwrap();
    let root = Root::new("My tiles", RootKind::Dir(home.clone()));
    let tiles = root
        .scan(&mem, &super::super::Program)
        .expect("a folder scans");
    assert_eq!(
        tiles,
        vec![
            home.join("a.dtile").to_string_lossy().into_owned(),
            home.join("sub/b.dtile").to_string_lossy().into_owned(),
        ]
    );
    assert_eq!(root.scan_folders(&mem), Some(vec!["sub".to_owned()]));
    assert_eq!(mem.stat(&home).unwrap().kind, EntryKind::Dir);

    // a new file is never over another, and a delete takes only the file named
    let dest = home.join("new.dtile");
    mem.create_new(&dest, TILE.as_bytes()).unwrap();
    let clash = mem.create_new(&dest, b"x").unwrap_err();
    assert_eq!(clash.kind(), std::io::ErrorKind::AlreadyExists);
    mem.remove_file(&dest).unwrap();
    assert!(!mem.exists(&dest) && mem.exists(&home.join("a.dtile")));
    assert_eq!(
        mem.remove_file(&dest).unwrap_err().kind(),
        std::io::ErrorKind::NotFound
    );
    assert!(!Path::new("/memory").exists(), "the test touched the disk");
}

/// The owner, the documents and all of `dt-service` reach files only through [`Storage`]: a scan of their source
/// finds no `std::fs`.
#[test]
fn the_owner_and_the_documents_make_no_std_fs_call() {
    let preview = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/preview");
    let service = Path::new(env!("CARGO_MANIFEST_DIR")).join("../dt-service/src");
    let mut files = vec![preview.join("owner.rs"), preview.join("docs.rs")];
    for entry in std::fs::read_dir(&service).expect("dt-service's source lists") {
        let path = entry.expect("an entry reads").path();
        if path.extension().is_some_and(|e| e == "rs") {
            files.push(path);
        }
    }
    assert!(files.len() > 10, "the scan found {files:?}");
    for file in files {
        let text = std::fs::read_to_string(&file).expect("a source file reads");
        // the tests at the foot of a file may use the disk
        let code = text
            .split("#[cfg(test)]\nmod tests")
            .next()
            .unwrap_or(&text);
        for (n, line) in code.lines().enumerate() {
            let body = line.trim_start();
            assert!(
                body.starts_with("//") || !(body.contains("std::fs") || body.contains("fs::")),
                "{}:{} reaches the disk, not Storage: {line}",
                file.display(),
                n + 1
            );
        }
    }
}
