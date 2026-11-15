//! The owner of every document file the editor writes: one [`Doc`] per path, holding a revision (`docRev`, a number
//! that only rises) and the lock that every read-modify-write of the file takes.
//!
//! The file is the truth; the `Doc` is who may say what it holds.  Every reader that needs text and its revision
//! together ([`Docs::read`]) and every writer ([`Docs::edit`]) goes through it, and each looks at the file under the
//! lock: a text that is not the one the `Doc` last wrote or read was changed from outside (a text editor, another
//! program), and that is a new revision too.  So a revision names one text, and a higher one is a later one, whoever
//! made the change.  A write the service makes is told to whoever listens ([`Docs::new`]'s `told`): the program tells
//! its owner, so the tiles that read the file are built again at once.
//!
//! An edit made against a revision (`expect`) is refused with a conflict (and the current text and revision) when the
//! file has moved on; an edit that names no revision is for the few calls that add to a file without reading it.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use dt_value::LockExt as _;

use crate::call::{Refusal, Status};
use crate::storage::Storage;

/// Told the path of every file the service writes.
pub type Told = Box<dyn Fn(&str) + Send + Sync>;

/// The documents the editor has read or written.
pub struct Docs {
    docs: Mutex<HashMap<String, Arc<Doc>>>,
    told: Option<Told>,
    storage: Arc<dyn Storage>,
}

/// One document: its revision and what it last held, behind the lock.
#[derive(Default)]
struct Doc {
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
pub struct Edited {
    /// The text it replaced.
    pub before: String,
    /// The text it wrote.
    pub after: String,
    /// The revision the file has now.
    pub rev: u64,
}

/// Why an edit was not made: the service's refusal (a stale revision, a file that cannot be read or written), or the
/// refusal of the caller's own function `f`, which is its own type.
#[derive(Debug)]
pub enum DocFail<E> {
    Service(Refusal),
    Caller(E),
}

impl DocFail<Refusal> {
    /// Either side, as the one refusal.
    #[must_use]
    pub fn into_refusal(self) -> Refusal {
        match self {
            Self::Service(r) | Self::Caller(r) => r,
        }
    }
}

impl Docs {
    /// Documents on `storage`; every write is told to `told`.
    #[must_use]
    pub fn new(storage: Arc<dyn Storage>, told: Option<Told>) -> Self {
        Self {
            docs: Mutex::default(),
            told,
            storage,
        }
    }

    fn doc(&self, path: &str) -> Arc<Doc> {
        Arc::clone(
            self.docs
                .lock_unpoisoned()
                .entry(path.to_owned())
                .or_default(),
        )
    }

    /// The text of `path` and its revision, read together under the lock.
    ///
    /// # Errors
    /// The file cannot be read.
    pub fn read(&self, path: &str) -> std::io::Result<(String, u64)> {
        let doc = self.doc(path);
        let mut st = doc.state.lock_unpoisoned();
        let text = self.storage.read_text(Path::new(path))?;
        st.observe(&text);
        Ok((text, st.rev))
    }

    /// Is there a file at `path`?
    ///
    /// # Errors
    /// The store cannot say.
    pub fn exists(&self, path: &str) -> std::io::Result<bool> {
        match self.storage.stat(Path::new(path)) {
            Ok(_) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// Make the file at `path` with `text` (its folder too, when it is not there): never over a file that is there.
    /// The text is written whole or not at all, and told like any write.
    ///
    /// # Errors
    /// The file is there already (a document is added to with [`Docs::edit`]), or cannot be written.
    pub fn create(&self, path: &str, text: &str) -> std::io::Result<()> {
        let doc = self.doc(path);
        let mut st = doc.state.lock_unpoisoned();
        if self.exists(path)? {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                format!("{path} is there already"),
            ));
        }
        if let Some(dir) = Path::new(path)
            .parent()
            .filter(|d| !d.as_os_str().is_empty())
        {
            self.storage.create_dir_all(dir)?;
        }
        self.storage.write_text(Path::new(path), text)?;
        st.observe(text);
        if let Some(told) = &self.told {
            told(path);
        }
        Ok(())
    }

    /// The revision of `path` now (what a reply that wrote nothing says).
    ///
    /// # Errors
    /// The file cannot be read.
    pub fn rev(&self, path: &str) -> Result<u64, Refusal> {
        self.read(path)
            .map(|(_, r)| r)
            .map_err(|e| read_failed(path, &e))
    }

    /// Edit `path` under its lock: `f` is given the text and answers the new text and what to add to the reply.  With
    /// `expect`, the edit is refused (a conflict, the current text and revision) unless the file is at that revision.
    /// The new text is written whole or not at all.
    ///
    /// # Errors
    /// [`DocFail`]: the file cannot be read or written, the revision is stale, or `f` refused.
    pub fn edit<X, E>(
        &self,
        path: &str,
        expect: Option<u64>,
        f: impl FnOnce(&str) -> Result<(String, X), E>,
    ) -> Result<(Edited, X), DocFail<E>> {
        let doc = self.doc(path);
        let mut st = doc.state.lock_unpoisoned();
        let before = self
            .storage
            .read_text(Path::new(path))
            .map_err(|e| DocFail::Service(read_failed(path, &e)))?;
        st.observe(&before);
        if let Some(want) = expect
            && want != st.rev
        {
            return Err(DocFail::Service(stale(&before, st.rev)));
        }
        let (after, extra) = f(&before).map_err(DocFail::Caller)?;
        // an edit that changes nothing is not a revision
        if after != before {
            self.storage
                .write_text(Path::new(path), &after)
                .map_err(|e| {
                    DocFail::Service(
                        Refusal::failure(Status::Internal, format!("cannot write {path}: {e}"))
                            .caused_by(&e),
                    )
                })?;
            st.hash = Some(dt_eval::doc::text_hash(&after));
            st.rev += 1;
            if let Some(told) = &self.told {
                told(path);
            }
        }
        Ok((
            Edited {
                before,
                after,
                rev: st.rev,
            },
            extra,
        ))
    }
}

/// The conflict for an edit made against a revision the file has left.
#[must_use]
pub fn stale(now: &str, rev: u64) -> Refusal {
    let mut r = Refusal::refused(
        Status::Conflict,
        "The file changed since this edit was made (in another tab or your text editor); the edit would lose that change.",
    );
    r.conflict = Some(Box::new(crate::call::Conflict {
        text: now.to_owned(),
        doc_rev: rev,
        scene: None,
    }));
    r
}

/// A file that cannot be read.
#[must_use]
pub fn read_failed(path: &str, e: &std::io::Error) -> Refusal {
    Refusal::failure(Status::Internal, format!("cannot read {path}: {e}")).caused_by(e)
}
