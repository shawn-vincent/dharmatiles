//! The storage seam: a save, a rename and a trash with its undo, run through the owner and the documents with no
//! disk at all.  What the editor does to files goes through [`Storage`], so a memory store stands in for the disk.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::super::docs::Docs;
use super::super::owner::{Command, Owner};
use super::super::storage::{MemStorage, Storage};
use dt_eval::Options;

const TILE: &str = "tile size=\"1x1\" seed=1 {\n    stack {\n        dirt\n    }\n}\n";

#[test]
fn a_save_a_rename_and_a_trash_then_undo_need_no_disk() {
    let a = PathBuf::from("/memory/My tiles/a.dtile");
    let b = PathBuf::from("/memory/My tiles/b.dtile");
    let bin = PathBuf::from("/memory/Trash");
    let mem = Arc::new(MemStorage::with(a.to_str().unwrap(), TILE));
    let owner = Owner::start(
        Vec::new(),
        Vec::new(),
        Options::default(),
        None,
        "test".into(),
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
