//! The program's view of the documents the editor writes: [`dt_service::docs::Docs`] (the revisions, the lock, the
//! conflict) with the program's errors, a [`Reply`], where the service's are a [`dt_service::call::Refusal`].  The
//! service's tile calls read `Shared::docs` through [`dt_service::tile_host::TileHost`]; the places, plates and
//! lists that are still the binary's read it through the methods here.

use std::sync::Arc;

#[cfg(test)]
use dt_service::docs::DocFail;

#[cfg(test)]
use super::api::Reply;
use super::storage::{DiskStorage, Storage};

/// The documents the editor has read or written.
pub(super) struct Docs(dt_service::docs::Docs);

/// What an edit did.
#[cfg(test)]
pub(super) use dt_service::docs::Edited;

impl Default for Docs {
    /// On the disk, and told to nobody.
    fn default() -> Self {
        Self(dt_service::docs::Docs::new(Arc::new(DiskStorage), None))
    }
}

impl Docs {
    /// Documents whose every write is told to the owner ([`super::owner::Command::Saved`]).
    pub(super) fn telling(owner: super::owner::Owner, storage: Arc<dyn Storage>) -> Self {
        Self(dt_service::docs::Docs::new(
            storage,
            Some(Box::new(move |path| {
                owner.send(super::owner::Command::Saved(path.to_owned()));
            })),
        ))
    }

    /// The service's documents, for the tile calls that moved there.
    pub(super) fn service(&self) -> &dt_service::docs::Docs {
        &self.0
    }

    /// The text of `path` and its revision, read together under the lock.
    ///
    /// # Errors
    /// The file cannot be read.
    pub(super) fn read(&self, path: &str) -> std::io::Result<(String, u64)> {
        self.0.read(path)
    }

    /// Edit `path` under its lock (see [`dt_service::docs::Docs::edit`]).
    ///
    /// # Errors
    /// The reply to send: the file cannot be read or written, the revision is stale, or `f` refused.
    #[cfg(test)]
    pub(super) fn edit<X>(
        &self,
        path: &str,
        expect: Option<u64>,
        f: impl FnOnce(&str) -> Result<(String, X), Reply>,
    ) -> Result<(Edited, X), Reply> {
        self.0.edit(path, expect, f).map_err(|e| match e {
            DocFail::Service(r) => super::api::refusal_reply(r),
            DocFail::Caller(reply) => reply,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str, text: &str) -> String {
        let dir = std::env::temp_dir().join(format!("dt-docs-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("t.dmap");
        std::fs::write(&p, text).unwrap();
        p.to_string_lossy().into_owned()
    }

    #[test]
    fn edits_from_many_threads_are_all_kept_and_each_is_a_revision() {
        let path = file("race", "");
        let docs = Arc::new(Docs::default());
        let (_, first) = docs.read(&path).unwrap();
        let threads: Vec<_> = (0..4)
            .map(|_| {
                let (docs, path) = (Arc::clone(&docs), path.clone());
                std::thread::spawn(move || {
                    for _ in 0..25 {
                        docs.edit(&path, None, |t| Ok((format!("{t}x"), ())))
                            .unwrap();
                    }
                })
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
        let (text, rev) = docs.read(&path).unwrap();
        assert_eq!(
            text.len(),
            100,
            "no edit read the file another was about to change"
        );
        assert_eq!(rev, first + 100);
    }

    #[test]
    fn a_change_from_outside_is_a_revision_and_an_edit_against_the_old_one_is_refused() {
        let path = file("outside", "a");
        let docs = Docs::default();
        let (_, rev) = docs.read(&path).unwrap();
        // the same text again is the same revision; an edit that changes nothing is not one
        assert_eq!(docs.read(&path).unwrap().1, rev);
        let same = docs
            .edit(&path, Some(rev), |t| Ok((t.to_owned(), ())))
            .unwrap()
            .0;
        assert_eq!(same.rev, rev);
        let made = docs
            .edit(&path, Some(rev), |_| Ok(("b".to_owned(), ())))
            .unwrap()
            .0;
        assert_eq!(made.rev, rev + 1);
        // another program writes the file: the next look at it is a later revision
        std::fs::write(&path, "c").unwrap();
        let (text, now) = docs.read(&path).unwrap();
        assert_eq!((text.as_str(), now), ("c", rev + 2));
        // an edit made against what the page last saw is refused, with the file as it is, and writes nothing
        let (status, body) = docs
            .edit(&path, Some(made.rev), |_| Ok(("d".to_owned(), ())))
            .err()
            .unwrap();
        assert_eq!(status, "409 Conflict");
        let body = body.failed().unwrap();
        assert_eq!(
            (body.text.as_deref(), body.doc_rev),
            (Some("c"), Some(rev + 2))
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "c");
        // a change nobody has read yet is still a revision when an edit finds it
        std::fs::write(&path, "e").unwrap();
        assert!(
            docs.edit(&path, Some(rev + 2), |_| Ok(("f".to_owned(), ())))
                .is_err()
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "e");
    }
}
