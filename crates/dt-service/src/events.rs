//! The listeners and the changes they have been sent: the event stream's state, held by the owner alone.

use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;
use std::sync::mpsc::Sender;

use crate::host::Alarm;
use crate::tile::TileId;

/// How many changes the event log remembers for a stream that resumes.
pub const LOG: usize = 1024;

/// What changed, as the event log remembers it.
pub enum Change {
    Tile(TileId),
    /// A resource (places, target, printers, inventory, list) changed: see [`crate::resources`].
    Resource(&'static str),
}

/// What the pages were last told of each resource.
#[derive(Default)]
pub struct Told {
    /// Each resource's last event, as sent (what a page that resumes is sent again).
    pub events: BTreeMap<&'static str, String>,
    /// Each resource's value as text, to see whether it changed.
    pub values: BTreeMap<&'static str, String>,
    /// Each resource's own revision.
    pub revs: BTreeMap<&'static str, u64>,
    /// The reading each resource read on another thread was told from.
    pub read_at: BTreeMap<&'static str, u64>,
    /// The tiles listed, in order.
    pub ids: Vec<TileId>,
}

/// The listeners and the changes they have been sent.  Every change to the list or a tile takes the next revision;
/// progress events ride on the current one.  Held by the owner alone.
pub struct Events {
    pub rev: u64,
    pub log: VecDeque<(u64, Change)>,
    pub clients: Vec<Sender<String>>,
    /// What happened to each tile ([`crate::message`]): kept here so it is sent in step with the changes.
    pub messages: crate::message::Log,
    /// What the pages were last told of each resource.
    pub told: Told,
    /// Where an event that cannot be written is said.
    alarm: Arc<dyn Alarm>,
}

impl Events {
    #[must_use]
    pub fn new(alarm: Arc<dyn Alarm>) -> Self {
        Self {
            rev: 1,
            log: VecDeque::new(),
            clients: Vec::new(),
            messages: crate::message::Log::default(),
            told: Told::default(),
            alarm,
        }
    }

    pub fn send<T: serde::Serialize>(&mut self, msg: &T) {
        let msg = match serde_json::to_string(msg) {
            Ok(m) => m,
            Err(e) => {
                self.alarm
                    .alarm(&format!("error: preview: an event could not be written: {e}"));
                return;
            }
        };
        self.clients.retain(|tx| tx.send(msg.clone()).is_ok());
    }

    /// Note a change, and say which revision it is.
    pub fn bump(&mut self, change: Change) -> u64 {
        self.rev += 1;
        self.log.push_back((self.rev, change));
        if self.log.len() > LOG {
            self.log.pop_front();
        }
        self.rev
    }
}
