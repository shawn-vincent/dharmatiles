//! What the page is told besides a tile's own state, as resources: the places (the roots and their folders), the
//! build target, the printers, the inventory and the list of tiles.  Each has its own revision, and the program says
//! a resource on the event stream only when *it* changed (`{type: "resource", name, resRev, value}`): a new folder does
//! not resend the inventory, an inventory change does not resend the tiles, and the page takes each into its own part
//! of its state.
//!
//! One function, [`Shared::publish`], knows how: it builds each resource that may have changed, compares it with what
//! the pages were last told and sends only the ones that differ.  The first event of a stream, and the answer of
//! `GET /api/tiles`, is the whole ([`api::Snapshot`]), built from the same values.

use super::plan::PrinterRow;
use super::*;
use dt_service::inventory::InventoryState;

pub use dt_service::resources::*;

impl Shared {
    pub(super) fn printers_value(&self) -> Vec<PrinterRow> {
        plates::printer_rows(&dt_eval::OnDisk {
            home: self.home.as_deref(),
        })
    }

    pub(super) fn inventory_value(&self) -> InventoryState {
        inventory::snapshot(self.home.as_deref())
    }

    /// The resources the owner cannot read itself (it reads no disk), as they are now: the inventory (and the printers
    /// when `printers`: they are read from disk, so only a call that changed them asks), with the number of the
    /// reading.  The owner tells the ones that differ from what the pages were last told, and none that was read
    /// before one it has told already.
    pub(super) fn resources_read(&self, printers: bool) -> (u64, Vec<Resource>) {
        let seq = self.read_seq.fetch_add(1, Ordering::SeqCst);
        let mut now = vec![Resource::Inventory(self.inventory_value())];
        if printers {
            now.push(Resource::Printers(self.printers_value()));
        }
        (seq, now)
    }

    /// Say what changed among the places, the target, the inventory and the list of tiles (and the printers when
    /// `printers`).  Only a resource whose value differs from what the pages were last told is sent.
    pub(super) fn publish(&self, printers: bool) {
        let (seq, read) = self.resources_read(printers);
        self.owner.send(owner::Command::Publish { seq, read });
    }

    /// Every tile as the queue sees it ([`Row`]).
    pub(super) fn rows(&self) -> Vec<Row> {
        self.owner.ask(owner::Command::Rows)
    }
}
