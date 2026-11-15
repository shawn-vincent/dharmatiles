//! The owner of the editor's state is the service's ([`dt_service::owner`]); this is the program's instance of it:
//! its host is [`Program`], so a side lane's job is a [`super::queue::SideJob`], an alarm goes to stderr and the
//! owner's thread is the program's.

use std::sync::Arc;

use dt_service::clock::MonotonicClock;

use super::*;

pub(super) type Command = dt_service::owner::Command<Program>;
pub(super) type Owner = dt_service::owner::Owner<Program>;
pub(super) use dt_service::owner::{Begin, Outside, Subscription};

/// The owner of the program's tiles, started on its own thread.
pub(super) fn start(
    tiles: Vec<Slot>,
    roots: Vec<Root>,
    target: Options,
    home: Option<String>,
    epoch: String,
    storage: Arc<dyn storage::Storage>,
) -> Owner {
    Owner::start(
        dt_service::owner::Setup {
            tiles,
            roots,
            target,
            home,
            epoch,
        },
        storage,
        Arc::new(Program),
        Arc::new(MonotonicClock::default()),
    )
}

/// An event as the stream sends it.  One that cannot be written is said, loudly, and not sent.
pub(super) fn encode(event: &api::StreamEvent) -> Option<String> {
    dt_service::owner::encode(event, &Program)
}
