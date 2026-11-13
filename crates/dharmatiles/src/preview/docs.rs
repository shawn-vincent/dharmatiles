//! The owner of every document file the editor writes: one [`Doc`] per path,
//! holding a revision (`docRev`, a number that only rises) and the lock that
//! every read-modify-write of the file takes.
//!
//! The file is the truth; the `Doc` is who may say what it holds.  Every
//! reader that needs text and its revision together ([`Docs::read`]) and
//! every writer ([`Docs::edit`]) goes through it, and each looks at the file
//! under the lock: a text that is not the one the `Doc` last wrote or read
//! was changed from outside (a text editor, another program), and that is a
//! new revision too.  So a revision names one text, and a higher one is
//! a later one, whoever made the change.  The worker's polling of modified
//! times reads a changed file through [`Docs::read`], which is how an
//! outside change gets its revision.
//!
//! An edit made against a revision (`expect`) is refused with a 409 (and the
//! current text and revision) when the file has moved on; an edit that names
//! no revision is for the few calls that add to a file without reading it
//! (a tile put on a print list from the tile's own level).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

use super::{fail, lock};

/// A reply that is not a success: the status and the body.
pub(super) type Reply = (&'static str, Value);

/// The documents the editor has read or written.
#[derive(Default)]
pub(super) struct Docs {
    docs: Mutex<HashMap<String, Arc<Doc>>>,
}

/// One document: its revision and what it last held, behind the lock.
#[derive(Default)]
pub(super) struct Doc {
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    rev: u64,
    hash: Option<u64>,
}

impl State {
    /// `text` is what the file holds now: a revision of its own unless it is what was last seen.
    fn observe(&mut self, text: &str) {
        let h = dt_eval::doc::text_hash(text);
        if self.hash != Some(h) {
            self.hash = Some(h);
            self.rev += 1;
        }
    }
}

/// What an edit did.
pub(super) struct Edited {
    /// The text it replaced.
    pub before: String,
    /// The text it wrote.
    pub after: String,
    /// The revision the file has now.
    pub rev: u64,
    /// What the edit's own function added to its answer.
    pub extra: Value,
}

impl Docs {
    fn doc(&self, path: &str) -> Arc<Doc> {
        Arc::clone(lock(&self.docs).entry(path.to_owned()).or_default())
    }

    /// The text of `path` and its revision, read together under the lock.
    ///
    /// # Errors
    /// The file cannot be read.
    pub(super) fn read(&self, path: &str) -> std::io::Result<(String, u64)> {
        let doc = self.doc(path);
        let mut st = lock(&doc.state);
        let text = std::fs::read_to_string(path)?;
        st.observe(&text);
        Ok((text, st.rev))
    }

    /// The revision of `path` now (what a reply that wrote nothing says).
    ///
    /// # Errors
    /// The reply to send: the file cannot be read.
    pub(super) fn rev(&self, path: &str) -> Result<u64, Reply> {
        self.read(path)
            .map(|(_, r)| r)
            .map_err(|e| read_failed(path, &e))
    }

    /// Edit `path` under its lock: `f` is given the text and answers the new
    /// text and what to add to the reply.  With `expect`, the edit is refused
    /// (409, the current text and revision) unless the file is at that
    /// revision.  The new text is written whole or not at all.
    ///
    /// # Errors
    /// The reply to send: the file cannot be read or written, the revision
    /// is stale, or `f` refused.
    pub(super) fn edit(
        &self,
        path: &str,
        expect: Option<u64>,
        f: impl FnOnce(&str) -> Result<(String, Value), Reply>,
    ) -> Result<Edited, Reply> {
        let doc = self.doc(path);
        let mut st = lock(&doc.state);
        let before = std::fs::read_to_string(path).map_err(|e| read_failed(path, &e))?;
        st.observe(&before);
        if let Some(want) = expect
            && want != st.rev
        {
            return Err(stale(&before, st.rev));
        }
        let (after, extra) = f(&before)?;
        // an edit that changes nothing is not a revision
        if after != before {
            dt_eval::atomic::write_atomic(std::path::Path::new(path), &after).map_err(|e| {
                fail::failure_from(
                    "500 Internal Server Error",
                    format!("cannot write {path}: {e}"),
                    &e,
                )
            })?;
            st.hash = Some(dt_eval::doc::text_hash(&after));
            st.rev += 1;
        }
        Ok(Edited {
            before,
            after,
            rev: st.rev,
            extra,
        })
    }
}

/// The 409 for an edit made against a revision the file has left.
pub(super) fn stale(now: &str, rev: u64) -> Reply {
    fail::refused_with(
        "409 Conflict",
        "The file changed since this edit was made (in another tab or your text editor); the edit would lose that change.",
        json!({ "text": now, "docRev": rev }),
    )
}

pub(super) fn read_failed(path: &str, e: &std::io::Error) -> Reply {
    fail::failure_from(
        "500 Internal Server Error",
        format!("cannot read {path}: {e}"),
        e,
    )
}

/// The revision an edit says it was made against: a call from the page names it.
///
/// # Errors
/// The call did not say (a 400).
pub(super) fn expected(v: &Value) -> Result<u64, Reply> {
    v["docRev"].as_u64().ok_or_else(|| {
        fail::refused(
            "400 Bad Request",
            "give the docRev the edit was made against",
        )
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
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
                        docs.edit(&path, None, |t| Ok((format!("{t}x"), json!({}))))
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
            .edit(&path, Some(rev), |t| Ok((t.to_owned(), json!({}))))
            .unwrap();
        assert_eq!(same.rev, rev);
        let made = docs
            .edit(&path, Some(rev), |_| Ok(("b".to_owned(), json!({}))))
            .unwrap();
        assert_eq!(made.rev, rev + 1);
        // another program writes the file: the next look at it is a later revision
        std::fs::write(&path, "c").unwrap();
        let (text, now) = docs.read(&path).unwrap();
        assert_eq!((text.as_str(), now), ("c", rev + 2));
        // an edit made against what the page last saw is refused, with the file as it is, and writes nothing
        let (status, body) = docs
            .edit(&path, Some(made.rev), |_| Ok(("d".to_owned(), json!({}))))
            .err()
            .unwrap();
        assert_eq!(status, "409 Conflict");
        assert_eq!(
            (body["text"].as_str(), body["docRev"].as_u64()),
            (Some("c"), Some(rev + 2))
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "c");
        // a change nobody has read yet is still a revision when an edit finds it
        std::fs::write(&path, "e").unwrap();
        assert!(
            docs.edit(&path, Some(rev + 2), |_| Ok(("f".to_owned(), json!({}))))
                .is_err()
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "e");
    }
}
