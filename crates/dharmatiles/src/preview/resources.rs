//! What the page is told besides a tile's own state, as resources: the places (the roots and their folders), the
//! build target, the printers, the inventory and the list of tiles.  Each has its own revision, and the program says
//! a resource on the event stream only when *it* changed (`{type: "resource", name, resRev, value}`): a new folder does
//! not resend the inventory, an inventory change does not resend the tiles, and the page takes each into its own part
//! of its state.
//!
//! One function, [`Shared::publish`], knows how: it builds each resource that may have changed, compares it with what
//! the pages were last told and sends only the ones that differ.  The first event of a stream, and the answer of
//! `GET /api/tiles`, is the whole ([`api::Snapshot`]), built from the same values.

use std::collections::BTreeMap;

use serde::Serialize;

use super::api::{self, FolderJson, Json, RootJson, TileSummary};
use super::*;

/// What the pages were last told of each resource.
#[derive(Default)]
pub(super) struct Told {
    /// Each resource's last event, as sent (what a page that resumes is sent again).
    events: BTreeMap<&'static str, String>,
    /// Each resource's value as text, to see whether it changed.
    values: BTreeMap<&'static str, String>,
    /// Each resource's own revision.
    revs: BTreeMap<&'static str, u64>,
    /// The tiles listed, in order.
    ids: Vec<TileId>,
}

/// The places: where tiles come from and the folders in them.
#[derive(Serialize, Clone)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct Places {
    pub roots: Vec<RootJson>,
    pub folders: Vec<FolderJson>,
    pub home: Option<String>,
}

/// The build target the page chose.
#[derive(Serialize, Clone)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Target {
    pub system: String,
    pub printer: String,
    pub overhang_deg: f64,
    pub steepest_deg: f64,
}

/// The tiles listed, in order, with the state of the ones that joined since the pages were last told.
#[derive(Serialize, Clone)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct List {
    pub ids: Vec<String>,
    pub added: Vec<TileSummary>,
}

/// One resource and its value.
#[derive(Serialize, Clone)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "name", content = "value", rename_all = "camelCase")]
pub enum Resource {
    Places(Places),
    Target(Target),
    Printers(Vec<Json>),
    Inventory(Json),
    List(List),
}

/// A resource on the event stream.
#[derive(Serialize, Clone)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct ResourceEvent {
    /// Always `resource`: the stream's event kind.
    #[serde(rename = "type")]
    pub kind: String,
    /// The stream's revision (for resuming).
    pub rev: u64,
    /// The resource's own revision: it rises when this resource changes and not otherwise.
    pub res_rev: u64,
    #[serde(flatten)]
    pub resource: Resource,
}

impl Resource {
    fn name(&self) -> &'static str {
        match self {
            Self::Places(_) => "places",
            Self::Target(_) => "target",
            Self::Printers(_) => "printers",
            Self::Inventory(_) => "inventory",
            Self::List(_) => "list",
        }
    }
}

impl Shared {
    pub(super) fn places_value(&self) -> Places {
        Places {
            roots: lock(&self.roots)
                .iter()
                .enumerate()
                .map(|(i, r)| r.json(i))
                .collect(),
            folders: lock(&self.folders)
                .iter()
                .flat_map(|(r, v)| {
                    v.iter().map(move |p| FolderJson {
                        root: *r,
                        path: p.clone(),
                    })
                })
                .collect(),
            home: std::env::var("HOME").ok(),
        }
    }

    pub(super) fn target_value(&self) -> Target {
        let t = self.options();
        Target {
            system: t.system.clone(),
            overhang_deg: t.printer.overhang_deg,
            steepest_deg: t.printer.steepest_deg,
            printer: t.printer.name.clone(),
        }
    }

    pub(super) fn printers_value(&self) -> Vec<Json> {
        plates::printers_json(self.home.as_deref())
            .iter()
            .map(api::camel_keys)
            .collect()
    }

    pub(super) fn inventory_value(&self) -> Json {
        api::camel_keys(&inventory::snapshot(self.home.as_deref()))
    }

    /// Say what changed among the places, the target, the inventory and the list of tiles (and the printers when
    /// `printers`: they are read from disk, so only a call that changed them asks).  Only a resource whose value
    /// differs from what the pages were last told is sent.
    pub(super) fn publish(&self, printers: bool) {
        let tiles = lock(&self.tiles);
        self.publish_locked(&tiles, printers);
    }

    /// [`Shared::publish`], with the tiles locked by the caller.
    pub(super) fn publish_locked(&self, tiles: &[Slot], printers: bool) {
        let mut now = vec![
            Resource::Places(self.places_value()),
            Resource::Target(self.target_value()),
            Resource::Inventory(self.inventory_value()),
        ];
        if printers {
            now.push(Resource::Printers(self.printers_value()));
        }
        let mut ev = lock(&self.events);
        let ids: Vec<TileId> = tiles.iter().map(|t| t.id.clone()).collect();
        if ids != ev.told.ids {
            let added = tiles
                .iter()
                .filter(|t| !ev.told.ids.contains(&t.id))
                .map(Slot::summary)
                .collect();
            now.push(Resource::List(List {
                ids: ids.clone(),
                added,
            }));
            ev.told.ids = ids;
        }
        for r in now {
            let name = r.name();
            // a list is told when its ids changed (decided above); the rest when their value did
            let text = match &r {
                Resource::List(l) => l.ids.join(","),
                other => serde_json::to_string(other).unwrap_or_default(),
            };
            if ev.told.values.get(name) == Some(&text) {
                continue;
            }
            ev.told.values.insert(name, text);
            let res_rev = {
                let n = ev.told.revs.entry(name).or_insert(0);
                *n += 1;
                *n
            };
            let rev = ev.bump(Change::Resource(name));
            let event = ResourceEvent {
                kind: "resource".to_owned(),
                rev,
                res_rev,
                resource: r,
            };
            match serde_json::to_string(&event) {
                Ok(s) => {
                    ev.told.events.insert(name, s);
                }
                Err(e) => tell!("error: preview: a resource could not be written: {e}"),
            }
            ev.send(&event);
        }
    }

    /// The events a page that resumes is sent again for the resources named: the last of each.
    pub(super) fn resource_events(ev: &Events, names: &[&'static str]) -> Vec<String> {
        names
            .iter()
            .filter_map(|n| ev.told.events.get(n).cloned())
            .collect()
    }
}
