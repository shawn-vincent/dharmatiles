//! A plan and its layout as data: the one record of each, read by the page's panel and printed by `dharmatiles plates --json`.
//!
//! Every type is made once, here, from the planner's own: the page adds what only it shows (the printers to choose from, the
//! list's pieces) around them, and the command line adds the files it wrote.

use super::{Kind, Layout, Plan};
use dt_kit::Material;

fn names(v: &[Material]) -> Vec<String> {
    v.iter().map(|m| m.name().to_owned()).collect()
}

/// A piece the inventory already holds, left off.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct PlanHeld {
    pub name: String,
    pub have: u32,
}

/// What a plan is laid out with, and what it leaves off: everything of a plan but its distinct pieces.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct PlanSettings {
    pub title: String,
    pub printer: String,
    pub label: String,
    pub bed: Option<[f64; 2]>,
    pub excluded: Vec<[f64; 4]>,
    pub slots: u32,
    pub verify: Option<String>,
    pub system: dt_lang::System,
    pub mode: String,
    pub group: String,
    pub gap: f64,
    pub margin: f64,
    pub turn: bool,
    pub skipped: Vec<String>,
    pub held: Vec<PlanHeld>,
    pub check_inventory: bool,
}

impl PlanSettings {
    /// The settings of `plan`.
    #[must_use]
    pub fn of(plan: &Plan) -> Self {
        let s = &plan.settings;
        Self {
            title: plan.title.clone(),
            printer: s.machine.name.clone(),
            label: s.machine.label(),
            bed: s.machine.bed,
            excluded: s.machine.excluded.clone(),
            slots: s.machine.slot_count(),
            verify: s.machine.verify.clone(),
            system: s.system,
            mode: s.mode.word().to_owned(),
            group: s.group.word().to_owned(),
            gap: s.gap,
            margin: s.margin,
            turn: s.turn,
            skipped: plan.skipped.clone(),
            held: plan
                .held
                .iter()
                .map(|(name, have)| PlanHeld {
                    name: name.clone(),
                    have: *have,
                })
                .collect(),
            check_inventory: s.check_inventory,
        }
    }
}

/// One distinct piece of a plan, before anything is built.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct PlannedKind {
    pub tile: String,
    pub name: String,
    pub count: u32,
    pub want: u32,
    pub have: u32,
    pub other_colour: u32,
    /// The tile as the inventory names it.
    pub canon: String,
    pub seed_used: u64,
    pub seed: Option<u64>,
    pub system: dt_lang::System,
}

impl PlannedKind {
    /// The piece `kind` of a plan.
    #[must_use]
    pub fn of(kind: &Kind) -> Self {
        Self {
            tile: kind.tile.clone(),
            name: kind.name.clone(),
            count: kind.count,
            want: kind.want,
            have: kind.have,
            other_colour: kind.other_colour,
            canon: kind.canon.clone(),
            seed_used: kind.seed_used,
            seed: kind.seed,
            system: kind.system,
        }
    }
}

/// A plan before any build: its settings and its distinct pieces (`plates --check --json`).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PlanOnly {
    #[serde(flatten)]
    pub settings: PlanSettings,
    pub kinds: Vec<PlannedKind>,
}

impl PlanOnly {
    /// The plan, as it stands before any build.
    #[must_use]
    pub fn of(plan: &Plan) -> Self {
        Self {
            settings: PlanSettings::of(plan),
            kinds: plan.kinds.iter().map(PlannedKind::of).collect(),
        }
    }
}

/// One plate: the filaments it needs and the pieces on it.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct PlanPlate {
    /// From 1.
    pub plate: usize,
    pub filaments: Vec<String>,
    pub pieces: Vec<PlanPlaced>,
}

/// A piece on a plate, where it stands.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct PlanPlaced {
    /// Which of the list's kinds.
    pub kind: usize,
    pub copy: usize,
    pub name: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub d: f64,
    pub turned: bool,
    /// The list piece this copy is (an index into the list's pieces).
    pub piece: Option<usize>,
    pub pinned: bool,
    /// That list piece's id, as an edit names it.
    pub id: Option<String>,
}

impl PlanPlate {
    /// The plates of `layout`; `ids` are the list's piece ids, so each placed copy names its piece as an edit does.
    #[must_use]
    pub fn of_layout(plan: &Plan, layout: &Layout, ids: &[String]) -> Vec<Self> {
        layout
            .plates
            .iter()
            .enumerate()
            .map(|(i, on)| Self {
                plate: i + 1,
                filaments: names(&layout.filaments[i]),
                pieces: on
                    .iter()
                    .map(|p| {
                        let piece = plan.kinds[p.kind].copies.get(p.copy).copied();
                        PlanPlaced {
                            kind: p.kind,
                            copy: p.copy,
                            name: plan.kinds[p.kind].name.clone(),
                            x: p.at.x,
                            y: p.at.y,
                            w: p.at.w,
                            d: p.at.d,
                            turned: p.at.turned,
                            piece,
                            pinned: layout.pinned.contains(&(p.kind, p.copy)),
                            id: piece.and_then(|i| ids.get(i)).cloned(),
                        }
                    })
                    .collect(),
            })
            .collect()
    }
}

/// The filaments of a set of materials, by name (the words a plan and a plate show).
#[must_use]
pub fn filament_names(v: &[Material]) -> Vec<String> {
    names(v)
}
