//! The scene: the tile's frame, its ground surface (a height grid painted
//! by area) and the tagged solids looks add, with the source-ID block and
//! material table of `dt_geom::source`.

use dt_geom::{BoolOp, Grid, IdBlock, Mask, Materials, Mesh, Seed, TriMesh, batch_in_order};

use crate::KitError;

/// The tile's size and resolution.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    /// Squares across (x, east).
    pub cols: u32,
    /// Squares up (y, north).
    pub rows: u32,
    /// mm per square: 35 for DungeonBlocks, 25.4 for OpenLOCK.
    pub square_mm: f64,
    /// Surface cells per square side.
    pub cells_per_square: u32,
}

/// The DungeonBlocks reference square: every size an author writes is mm
/// at this square, and scales with the system (R6 decision D2).
pub const REFERENCE_SQUARE_MM: f64 = 35.0;

/// The OpenLOCK square (one inch).
pub const OPENLOCK_SQUARE_MM: f64 = 25.4;

impl Frame {
    /// A DungeonBlocks frame at the default resolution (128 cells per
    /// square, Python's `SurfaceConfig.cells_per_square`).
    #[must_use]
    pub const fn dungeonblocks(cols: u32, rows: u32) -> Self {
        Self {
            cols,
            rows,
            square_mm: REFERENCE_SQUARE_MM,
            cells_per_square: 128,
        }
    }

    /// An OpenLOCK frame: 25.4 mm (one inch) squares, at the same 128
    /// cells per square.
    #[must_use]
    pub const fn openlock(cols: u32, rows: u32) -> Self {
        Self {
            cols,
            rows,
            square_mm: OPENLOCK_SQUARE_MM,
            cells_per_square: 128,
        }
    }

    /// The frame for a connector system: `ol` is OpenLOCK, anything else
    /// DungeonBlocks.
    #[must_use]
    pub fn for_system(system: &str, cols: u32, rows: u32) -> Self {
        if system == "ol" {
            Self::openlock(cols, rows)
        } else {
            Self::dungeonblocks(cols, rows)
        }
    }

    /// Width and height in mm.
    #[must_use]
    pub fn size_mm(&self) -> [f64; 2] {
        [
            f64::from(self.cols) * self.square_mm,
            f64::from(self.rows) * self.square_mm,
        ]
    }

    /// Surface nodes across and up.  Computed in `usize` and saturating,
    /// so a huge frame asks for an impossible grid instead of wrapping to
    /// a tiny one (documents cap tiles at 64 squares a side).
    #[must_use]
    pub const fn nodes(&self) -> (usize, usize) {
        let per = self.cells_per_square as usize;
        (
            (self.cols as usize).saturating_mul(per).saturating_add(1),
            (self.rows as usize).saturating_mul(per).saturating_add(1),
        )
    }

    /// Node spacing in mm.
    #[must_use]
    pub fn step_mm(&self) -> f64 {
        self.square_mm / f64::from(self.cells_per_square)
    }

    /// The frame with enough cells per square for features of `cell_mm`
    /// (world mm), never fewer than it has.
    #[must_use]
    pub fn with_cell(mut self, cell_mm: f64) -> Self {
        if cell_mm > 0.0 {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let n = libm::ceil(self.square_mm / cell_mm - 1e-9).min(4096.0) as u32;
            self.cells_per_square = self.cells_per_square.max(n);
        }
        self
    }

    /// How much an author's mm grow or shrink on this system.
    #[must_use]
    pub fn scale(&self) -> f64 {
        self.square_mm / REFERENCE_SQUARE_MM
    }
}

/// What a solid is made of; decides its paint in renders (and, later,
/// its colour in 3MF).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Material {
    /// Ground and dirt.
    Earth = 1,
    /// The connector underneath.
    Connector = 2,
    /// Rocks and stones.
    Rock = 3,
    /// Water.
    Water = 4,
    /// Grass blades.
    Grass = 5,
    /// Wall and floor stone (P5).
    Masonry = 10,
    /// Branches and trunks (P5).
    Wood = 11,
    /// Foliage clumps and leaves (P5).
    Leaf = 12,
}

impl Material {
    /// Its name, as a 3MF material or a legend names it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Earth => "earth",
            Self::Connector => "connector",
            Self::Rock => "rock",
            Self::Water => "water",
            Self::Grass => "grass",
            Self::Masonry => "masonry",
            Self::Wood => "wood",
            Self::Leaf => "leaf",
        }
    }

    /// Its default colour (sRGB, 0 to 1): the preview's paint. The
    /// material-to-colour step (a user's own palette) comes later; this is
    /// what a first 3MF is painted with.
    #[must_use]
    pub const fn colour(self) -> [f64; 3] {
        match self {
            Self::Earth => [0.50, 0.38, 0.25],
            Self::Connector => [0.62, 0.62, 0.64],
            Self::Rock => [0.58, 0.58, 0.61],
            Self::Water => [0.24, 0.38, 0.56],
            Self::Grass => [0.36, 0.56, 0.24],
            Self::Masonry => [0.66, 0.64, 0.60],
            Self::Wood => [0.42, 0.29, 0.18],
            Self::Leaf => [0.33, 0.52, 0.24],
        }
    }

    /// From the material table's number.
    #[must_use]
    pub const fn from_id(id: u32) -> Option<Self> {
        match id {
            1 => Some(Self::Earth),
            2 => Some(Self::Connector),
            3 => Some(Self::Rock),
            4 => Some(Self::Water),
            5 => Some(Self::Grass),
            10 => Some(Self::Masonry),
            11 => Some(Self::Wood),
            12 => Some(Self::Leaf),
            _ => None,
        }
    }
}

/// A solid's tag: what kind of thing it is and the path of the stack entry
/// that made it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tag {
    /// `ground`, `connector`, …: what other looks read it as.
    pub kind: &'static str,
    /// The stack path that made it, such as `tile/flat/stack/0`.
    pub owner: String,
}

/// A solid inside the mesh kernel. It never leaves the kernel between
/// operations (R1): looks combine solids only through kit operations.
#[derive(Clone)]
pub struct Solid(pub(crate) dt_geom::Solid);

impl Solid {
    /// Highest z in mm, or `None` if empty.
    #[must_use]
    pub fn top(&self) -> Option<f64> {
        self.0.bounds().map(|b| b.max[2])
    }

    /// Enclosed volume in mm³.
    #[must_use]
    pub fn volume(&self) -> f64 {
        self.0.volume()
    }

    /// Axis-aligned bounds `(min, max)`, or `None` if empty.
    #[must_use]
    pub fn bounds(&self) -> Option<([f64; 3], [f64; 3])> {
        self.0.bounds().map(|b| (b.min, b.max))
    }

    /// Moved by `d` mm. Source IDs are kept.
    #[must_use]
    pub fn translate(&self, d: [f64; 3]) -> Self {
        Self(self.0.transform(&[
            1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, d[0], d[1], d[2],
        ]))
    }

    /// Turned `deg` degrees about the z axis through the origin (counter-
    /// clockwise seen from above), then scaled by `scale` along z.
    #[must_use]
    pub fn turn(&self, deg: f64, scale_z: f64) -> Self {
        let (s, c) = (libm::sin(deg.to_radians()), libm::cos(deg.to_radians()));
        Self(
            self.0
                .transform(&[c, s, 0.0, -s, c, 0.0, 0.0, 0.0, scale_z, 0.0, 0.0, 0.0]),
        )
    }

    /// Its cross-section at height `z`: closed polygons (mm).
    #[must_use]
    pub fn slice(&self, z: f64) -> Vec<Vec<[f64; 2]>> {
        self.0.slice(z)
    }

    /// Its surface as a triangle mesh, to place things on (leaves on a
    /// crown). A query: what is placed is built as its own form and
    /// combined with kit operations; the mesh is never re-imported.
    #[must_use]
    pub fn mesh(&self) -> TriMesh {
        self.0.to_tri_mesh()
    }
}

/// One source slot: an original ID and its own face-ID range
/// (`dt_geom::source` rule 1). Copy it into parallel work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slot {
    pub(crate) id: u32,
    pub(crate) face_base: u32,
}

/// Consecutive slots, all of one material, handed out in sequence.
#[derive(Clone, Copy, Debug)]
pub struct Slots {
    block: IdBlock,
    first: u32,
    len: u32,
}

impl Slot {
    /// Its original (source) ID: the `run_original_id` of the triangles
    /// it made, in any mesh built from it.
    #[must_use]
    pub fn id(&self) -> u32 {
        self.id
    }
}

impl Slots {
    /// The `i`-th slot.
    ///
    /// # Panics
    /// If `i` is out of range.
    #[must_use]
    pub fn get(&self, i: u32) -> Slot {
        assert!(i < self.len, "slot {i} of {}", self.len);
        let s = self.first + i;
        Slot {
            id: self.block.id(s),
            face_base: self.block.face_base(s),
        }
    }
}

/// A tagged solid in the scene.
#[derive(Clone)]
pub struct Part {
    /// What it is.
    pub tag: Tag,
    /// The solid.
    pub solid: Solid,
}

/// A named stage hash (R2): equal hashes mean byte-identical stages.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stage {
    /// The stage, such as `surface` or `connector:tile/flat/connector`.
    pub name: String,
    /// 64-bit FNV-1a of the stage's exact output.
    pub hash: u64,
}

/// The lift's error bound, mm: no surface node is further than this from
/// the triangulated top (a tenth of a 0.2 mm layer).
pub const LIFT_TOL_MM: f64 = 0.02;
/// The widest leaf of the adaptive lift, in cells.
const LIFT_MAX_CELLS: usize = 32;
/// The widest bank between areas painted at different levels, mm at the
/// reference square (waterlab's `BANK_W_MM`).
pub const BANK_MM: f64 = 7.0;

/// Slots per build. A leaf's face IDs come from its slot's range, so this
/// is also `dt_geom::source::MAX_SLOTS`.
const SLOTS: u32 = dt_geom::source::MAX_SLOTS;

/// The scene of one build.
pub struct Scene {
    frame: Frame,
    seed: Seed,
    field: Grid,
    painted: Mask,
    /// Each paint, in order: its area and the full field it painted from.
    layers: Vec<(Mask, Vec<f64>)>,
    settled: bool,
    banked: bool,
    /// Nodes the water covers (lifted as water).
    wet: Mask,
    bed: Option<Grid>,
    water_edge_mm: Option<f64>,
    /// Nodes the ground's edge rule leaves alone (water and its banks).
    no_edge: Mask,
    stone: Mask,
    edge_mm: Option<f64>,
    notes: Vec<(String, f64)>,
    parts: Vec<Part>,
    /// For each part, how many paints came before it. A surface places its
    /// own solids (dirt's stones) and then paints, so a part belongs to the
    /// paint after it; a paint after that one over it removes it
    /// ([`Scene::cull_covered`]).
    placed_at: Vec<usize>,
    block: IdBlock,
    next_slot: u32,
    materials: Materials,
    stages: Vec<Stage>,
    support: Option<Grid>,
}

impl Scene {
    /// An empty scene. Reserves its source-ID block, so call it in
    /// sequence, not from parallel tasks.
    #[must_use]
    pub fn new(frame: Frame, seed: Seed) -> Self {
        let (nx, ny) = frame.nodes();
        Self {
            frame,
            seed,
            field: Grid::new(nx, ny, frame.step_mm(), 0.0),
            painted: Mask::new(nx, ny, false),
            layers: Vec::new(),
            settled: false,
            banked: false,
            wet: Mask::new(nx, ny, false),
            bed: None,
            water_edge_mm: None,
            no_edge: Mask::new(nx, ny, false),
            stone: Mask::new(nx, ny, false),
            edge_mm: None,
            notes: Vec::new(),
            parts: Vec::new(),
            placed_at: Vec::new(),
            block: IdBlock::reserve::<dt_geom::Solid>(SLOTS),
            next_slot: 0,
            materials: Materials::default(),
            stages: Vec::new(),
            support: None,
        }
    }

    /// **stamp**: a solid's footprint and top (world mm): the support
    /// under it rises to `top_mm` (never falls). Things that stand on
    /// others (a wall on a floor) and things that grow round them read
    /// it through [`Scene::support`].
    pub fn stamp(&mut self, footprint: &Mask, top_mm: f64) {
        let (nx, ny) = self.frame.nodes();
        let step = self.frame.step_mm();
        let s = self
            .support
            .get_or_insert_with(|| Grid::new(nx, ny, step, f32::NEG_INFINITY));
        #[allow(clippy::cast_possible_truncation)]
        let z = top_mm as f32;
        for (k, _) in footprint.bits.iter().enumerate().filter(|(_, b)| **b) {
            s.z[k] = s.z[k].max(z);
        }
    }

    /// The highest stamped top at each node (world mm, `-∞` where nothing
    /// is stamped), or `None` before any stamp.
    #[must_use]
    pub fn support(&self) -> Option<&Grid> {
        self.support.as_ref()
    }

    /// The ground surface as painted (world mm at every node), before the
    /// lift spreads paint to unpainted nodes; `None` before any paint.
    #[must_use]
    pub fn surface_field(&self) -> Option<&Grid> {
        (self.painted.count() > 0).then_some(&self.field)
    }

    /// The frame.
    #[must_use]
    pub const fn frame(&self) -> &Frame {
        &self.frame
    }

    /// The tile's root seed.
    #[must_use]
    pub const fn seed(&self) -> Seed {
        self.seed
    }

    /// The whole tile as an area.
    #[must_use]
    pub fn whole(&self) -> Mask {
        let (nx, ny) = self.frame.nodes();
        Mask::new(nx, ny, true)
    }

    /// **paint**: set the ground surface to `level_mm` (already scaled) over
    /// `area`. Later paints win, so stack order decides which surface paints
    /// over which.
    pub fn paint(&mut self, area: &Mask, level_mm: f64) {
        #[allow(clippy::cast_possible_truncation)]
        let z = level_mm as f32;
        for (k, _) in area.bits.iter().enumerate().filter(|(_, b)| **b) {
            self.field.z[k] = z;
            self.painted.bits[k] = true;
        }
        self.layers
            .push((area.clone(), vec![level_mm; self.field.z.len()]));
    }

    /// **paint** a field: set the ground surface to `field` (world mm) over
    /// `area`. Later paints win.
    pub fn paint_field(&mut self, area: &Mask, field: &crate::Field) {
        for (k, _) in area.bits.iter().enumerate().filter(|(_, b)| **b) {
            #[allow(clippy::cast_possible_truncation)]
            let z = field.z[k] as f32;
            self.field.z[k] = z;
            self.painted.bits[k] = true;
        }
        self.layers.push((area.clone(), field.z.clone()));
    }

    /// The ground surface (world mm): every paint, spread to the nodes no
    /// area painted (and banked, once [`Scene::bank`] has run). Read it
    /// after the surfaces are painted (the scheduler paints every surface
    /// before any solid); `None` when nothing painted it.
    pub fn ground(&mut self) -> Option<crate::Field> {
        if self.painted.count() == 0 {
            return None;
        }
        self.settle();
        Some(crate::Field {
            nx: self.field.nx,
            ny: self.field.ny,
            step: self.field.step,
            z: self.field.z.iter().map(|&v| f64::from(v)).collect(),
        })
    }

    /// **fill**: set the surface to `top` (world mm) where `wet` (the
    /// water's own solid top, lifted as water), and leave the nodes in
    /// `no_edge` out of the ground's edge rule (the water and its banks
    /// meet the edge on their own contract).
    pub fn fill(&mut self, top: &crate::Field, wet: &[bool], no_edge: &[bool]) {
        self.settle();
        if self.bed.is_none() {
            self.bed = Some(self.field.clone());
        }
        for k in 0..self.field.z.len() {
            #[allow(clippy::cast_possible_truncation)]
            let z = top.z[k] as f32;
            self.field.z[k] = z;
            self.wet.bits[k] |= wet[k];
            self.painted.bits[k] |= wet[k];
            self.no_edge.bits[k] |= no_edge[k];
        }
    }

    /// The nodes inside every solid of `kind` at height `z`: their
    /// footprint, from a slice of each (the query "footprint by tag").
    #[must_use]
    pub fn footprint(&self, kind: &str, z: f64) -> Mask {
        let (nx, ny) = self.frame.nodes();
        let mut out = Mask::new(nx, ny, false);
        for p in self.parts.iter().filter(|p| p.tag.kind == kind) {
            let m = dt_geom::fill_polygons(nx, ny, self.frame.step_mm(), &p.solid.slice(z));
            out.bits.iter_mut().zip(m.bits).for_each(|(o, b)| *o |= b);
        }
        out
    }

    /// The nodes inside any solid of the given kinds `above_mm` over its
    /// own lowest point: where things stand (a rock's outline where it
    /// meets the ground, a trunk, a wall's foot). Grass roots no blade
    /// there and stops its blades against it.
    #[must_use]
    pub fn standing(&self, kinds: &[&str], above_mm: f64) -> Mask {
        let (nx, ny) = self.frame.nodes();
        let mut out = Mask::new(nx, ny, false);
        for p in self.parts.iter().filter(|p| kinds.contains(&p.tag.kind)) {
            let Some(b) = p.solid.bounds() else { continue };
            let m = dt_geom::fill_polygons(
                nx,
                ny,
                self.frame.step_mm(),
                &p.solid.slice(b.0[2] + above_mm),
            );
            out.bits.iter_mut().zip(m.bits).for_each(|(o, b)| *o |= b);
        }
        out
    }

    /// Spread the paint, once.
    fn settle(&mut self) {
        if !self.settled {
            self.settled = true;
            self.spread_paint();
        }
    }

    /// **bank** (field blend): run the higher areas down to the lower ones,
    /// once. The water calls it before it reads the land, so a shore is a
    /// bank; plain ground keeps its steps (Phase 2).
    pub fn bank(&mut self) {
        self.cull_covered();
        self.settle();
        if !self.banked {
            self.banked = true;
            self.banks();
        }
    }

    /// Banks: where an area painted higher meets one painted lower, the
    /// higher ground runs down to the lower over a bank as wide as the
    /// step needs (no steeper than the ground's hills, 24° from 1.5 times
    /// the step, between 1 mm and [`BANK_MM`] at the reference square),
    /// cross-fading the two surfaces with a smoothstep. The lower side is
    /// untouched, so a pool keeps its bed and the bank is all on the land
    /// (waterlab's bank, `Scene` :278). Fields are the full ones each
    /// paint was given, so texture fades across the bank.
    fn banks(&mut self) {
        let n = self.layers.len();
        if n < 2 {
            return;
        }
        let k_all = self.field.z.len();
        // the last paint owning each node
        let mut owner = vec![usize::MAX; k_all];
        for (li, (area, _)) in self.layers.iter().enumerate() {
            for (k, _) in area.bits.iter().enumerate().filter(|(_, b)| **b) {
                owner[k] = li;
            }
        }
        // nodes no area painted (a line's own) belong to the area whose
        // paint spread to them, as in `spread_paint`
        let (nx, ny) = (self.field.nx, self.field.ny);
        let mut queue: std::collections::VecDeque<usize> =
            (0..k_all).filter(|&k| owner[k] != usize::MAX).collect();
        while let Some(k) = queue.pop_front() {
            let (i, j) = (k % nx, k / nx);
            for m in [
                (i > 0).then(|| k - 1),
                (i + 1 < nx).then(|| k + 1),
                (j > 0).then(|| k - nx),
                (j + 1 < ny).then(|| k + nx),
            ]
            .into_iter()
            .flatten()
            {
                if owner[m] == usize::MAX {
                    owner[m] = owner[k];
                    queue.push_back(m);
                }
            }
        }
        let s = self.frame.scale();
        let (lo_w, hi_w) = (1.0 * s, BANK_MM * s);
        let tan = libm::tan(24f64.to_radians());
        let mut z: Vec<f64> = self.field.z.iter().map(|&v| f64::from(v)).collect();
        for q in 0..n {
            let mine: Vec<bool> = owner.iter().map(|&o| o == q).collect();
            if !mine.iter().any(|&b| b) || mine.iter().all(|&b| b) {
                continue;
            }
            let (d, _) = crate::distance(&self.frame, &mine);
            let fq = &self.layers[q].1;
            for k in 0..k_all {
                let p = owner[k];
                if p == q || p == usize::MAX || d.z[k] >= hi_w {
                    continue;
                }
                // a spread node's own height is the field's, not its paint's
                let fp = if self.layers[p].0.bits[k] {
                    self.layers[p].1[k]
                } else {
                    z[k]
                };
                let step = fp - fq[k];
                if step <= 0.0 {
                    continue;
                }
                let w = (1.5 * step / tan).clamp(lo_w, hi_w);
                if d.z[k] < w {
                    let t = crate::smooth01(d.z[k] / w);
                    z[k] = z[k].min(fq[k] + step * t);
                }
            }
        }
        for (o, v) in self.field.z.iter_mut().zip(z) {
            #[allow(clippy::cast_possible_truncation)]
            let f = v as f32;
            *o = f;
        }
    }

    /// Mark surface nodes as stone (pebbles) for the painted preview;
    /// unmark the rest of `area`.
    pub fn mark_stone(&mut self, area: &Mask, stone: &[bool]) {
        for (k, _) in area.bits.iter().enumerate().filter(|(_, b)| **b) {
            self.stone.bits[k] = stone[k];
        }
    }

    /// The edge contract: the level (world mm) every ground surface meets
    /// the tile edge at, and never dips below. `None`: no contract.
    pub const fn set_edge(&mut self, level_mm: Option<f64>) {
        self.edge_mm = level_mm;
    }

    /// The edge level, world mm, if the tile has an edge contract.
    #[must_use]
    pub const fn edge(&self) -> Option<f64> {
        self.edge_mm
    }

    /// The level (world mm) the water meets every tile edge at, before
    /// its tide profile (the edge contract's `water` rule).
    pub const fn set_water_edge(&mut self, level_mm: Option<f64>) {
        self.water_edge_mm = level_mm;
    }

    /// See [`Scene::set_water_edge`].
    #[must_use]
    pub const fn water_edge(&self) -> Option<f64> {
        self.water_edge_mm
    }

    /// The ground as it was before water filled it (world mm): the bed
    /// under the water and the bank round it; `None` without water.
    #[must_use]
    pub const fn bed(&self) -> Option<&Grid> {
        self.bed.as_ref()
    }

    /// The nodes the water covers (empty without water).
    #[must_use]
    pub const fn wet(&self) -> &Mask {
        &self.wet
    }

    /// Record a look's number for the build record (counts and the like).
    pub fn note(&mut self, name: &str, value: f64) {
        self.notes.push((name.to_owned(), value));
    }

    /// The numbers looks noted, in order.
    #[must_use]
    pub fn notes(&self) -> &[(String, f64)] {
        &self.notes
    }

    /// The finished ground surface and its stone marks, for paint
    /// ([`crate::SurfacePaint::new`]); `None` when nothing painted it.
    #[must_use]
    pub fn surface(&self) -> Option<(dt_geom::Grid, Mask)> {
        (self.painted.count() > 0).then(|| (self.field.clone(), self.stone.clone()))
    }

    /// Reserve `n` slots of one material, in sequence.
    ///
    /// # Errors
    /// [`KitError::Slots`] when the build's block is used up.
    pub fn slots(&mut self, n: u32, material: Material) -> Result<Slots, KitError> {
        if self.next_slot + n > SLOTS {
            return Err(KitError::Slots);
        }
        let s = Slots {
            block: self.block,
            first: self.next_slot,
            len: n,
        };
        self.next_slot += n;
        for i in 0..n {
            self.materials.set_id(s.get(i).id, material as u32);
        }
        Ok(s)
    }

    /// **lift**: the painted surface as one solid from z = 0 (the uniform
    /// lift), tagged `ground`. Nodes no area painted take the level of the
    /// nearest painted node (breadth first, so drawn lines take their
    /// neighbours' level). Does nothing if nothing was painted.
    ///
    /// # Errors
    /// [`KitError`] if the surface is not above 0 or the kernel refuses it.
    pub fn lift(&mut self, owner: &str) -> Result<(), KitError> {
        if self.painted.count() == 0 {
            return Ok(());
        }
        self.cull_covered();
        self.settle();
        if let Some(edge) = self.edge_mm {
            self.hold_edge(edge);
        }
        self.record("surface", self.field.content_hash());
        let mesh = dt_geom::lift_adaptive(&self.field, LIFT_TOL_MM, LIFT_MAX_CELLS)
            .map_err(|e| KitError::Surface(e.to_string()))?;
        let slot = self.slots(1, Material::Earth)?.get(0);
        let solid = leaf(slot, &mesh)?;
        if self.wet.count() > 0 {
            // R9: the top over the water is water, face by face
            let out = solid.0.to_tri_mesh();
            let (nx, step) = (self.field.nx, self.field.step);
            let wet = &self.wet;
            let face = |t: usize| {
                let [a, b, c] = out.triangles[t].map(|i| out.positions[i as usize]);
                let n = crate::shape::normal(a, b, c);
                let (x, y) = ((a[0] + b[0] + c[0]) / 3.0, (a[1] + b[1] + c[1]) / 3.0);
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let (i, j) = (
                    libm::round(x / step).max(0.0) as usize,
                    libm::round(y / step).max(0.0) as usize,
                );
                let k = j.min(wet.ny - 1) * nx + i.min(nx - 1);
                if n[2] > 0.05 && wet.bits[k] {
                    Material::Water as u32
                } else {
                    Material::Earth as u32
                }
            };
            let before = self.materials.conflicts;
            self.materials.set_faces(&out, face);
            let conflicts = self.materials.conflicts - before;
            if conflicts > 0 {
                // faces that straddle the water line: the first triangle's
                // material holds for the whole face (R9 stays whole)
                #[allow(clippy::cast_precision_loss)]
                self.note("material_conflicts", conflicts as f64);
            }
        }
        self.add(
            Tag {
                kind: "ground",
                owner: owner.to_owned(),
            },
            solid,
        );
        Ok(())
    }

    /// The edge contract, by construction: perimeter nodes exactly at the
    /// edge level, and nothing below it (rises only), except where the
    /// water and its banks meet the edge on their own contract.
    fn hold_edge(&mut self, edge: f64) {
        #[allow(clippy::cast_possible_truncation)]
        let e = edge as f32;
        let (nx, ny) = (self.field.nx, self.field.ny);
        for j in 0..ny {
            for i in 0..nx {
                if self.no_edge.bits[j * nx + i] {
                    continue;
                }
                let z = &mut self.field.z[j * nx + i];
                if i == 0 || j == 0 || i == nx - 1 || j == ny - 1 {
                    *z = e;
                } else {
                    *z = z.max(e);
                }
            }
        }
    }

    fn spread_paint(&mut self) {
        let (nx, ny) = (self.field.nx, self.field.ny);
        let mut queue: std::collections::VecDeque<usize> =
            (0..nx * ny).filter(|&k| self.painted.bits[k]).collect();
        while let Some(k) = queue.pop_front() {
            let (i, j) = (k % nx, k / nx);
            let mut n = [None; 4];
            if i > 0 {
                n[0] = Some(k - 1);
            }
            if i + 1 < nx {
                n[1] = Some(k + 1);
            }
            if j > 0 {
                n[2] = Some(k - nx);
            }
            if j + 1 < ny {
                n[3] = Some(k + nx);
            }
            for m in n.into_iter().flatten() {
                if !self.painted.bits[m] {
                    self.painted.bits[m] = true;
                    self.field.z[m] = self.field.z[k];
                    queue.push_back(m);
                }
            }
        }
    }

    /// Remove each solid a surface placed (before a later paint) that a
    /// later paint covers within the solid's reach plus a bank's width:
    /// the ground it sat on is gone, so it would float or bury. Solids
    /// placed after every paint are never removed.
    fn cull_covered(&mut self) {
        let n = self.layers.len();
        if self.placed_at.iter().all(|&p| p + 1 >= n) {
            return;
        }
        let (nx, ny) = self.frame.nodes();
        let step = self.frame.step_mm();
        let margin = BANK_MM * self.frame.scale();
        let mut keep = Vec::with_capacity(self.parts.len());
        for (part, &at) in self.parts.iter().zip(&self.placed_at) {
            let covered = at + 1 < n
                && part.solid.bounds().is_some_and(|(lo, hi)| {
                    let reach = |a: f64, b: f64| {
                        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                        let idx = |v: f64| libm::round(v / step).max(0.0) as usize;
                        (idx(a - margin), idx(b + margin))
                    };
                    let (i0, i1) = reach(lo[0], hi[0]);
                    let (j0, j1) = reach(lo[1], hi[1]);
                    (j0..=j1.min(ny - 1)).any(|j| {
                        (i0..=i1.min(nx - 1)).any(|i| {
                            self.layers[at + 1..]
                                .iter()
                                .any(|(m, _)| m.bits[j * nx + i])
                        })
                    })
                });
            keep.push(!covered);
        }
        let mut it = keep.iter();
        self.parts.retain(|_| *it.next().unwrap_or(&true));
        let mut it = keep.iter();
        self.placed_at.retain(|_| *it.next().unwrap_or(&true));
    }

    /// Add a solid a look built, and record its stage hash.
    pub fn add(&mut self, tag: Tag, solid: Solid) {
        let hash = solid.0.to_tri_mesh().content_hash();
        self.record(&format!("{}:{}", tag.kind, tag.owner), hash);
        self.parts.push(Part { tag, solid });
        self.placed_at.push(self.layers.len());
    }

    /// **top**: the highest z of every solid so far (0 if none).
    #[must_use]
    pub fn top(&self) -> f64 {
        self.parts
            .iter()
            .filter_map(|p| p.solid.top())
            .fold(0.0, f64::max)
    }

    /// The parts so far, in build order.
    #[must_use]
    pub fn parts(&self) -> &[Part] {
        &self.parts
    }

    /// The stage hashes so far, in order.
    #[must_use]
    pub fn stages(&self) -> &[Stage] {
        &self.stages
    }

    /// Record a stage hash.
    pub fn record(&mut self, name: &str, hash: u64) {
        self.stages.push(Stage {
            name: name.to_owned(),
            hash,
        });
    }

    /// **union**: every part combined in build order (`batch_in_order`),
    /// exported once, with each triangle's material (R9). Ends the scene's
    /// time in the kernel; the export is what the audit takes.
    ///
    /// # Errors
    /// [`KitError::Untraced`] if a triangle has no material, or the
    /// kernel's status.
    pub fn union(&mut self) -> Result<(TriMesh, Vec<Material>), KitError> {
        let solids: Vec<dt_geom::Solid> = self.parts.iter().map(|p| p.solid.0.clone()).collect();
        let all = batch_in_order(&solids, BoolOp::Union);
        all.status().map_err(KitError::Mesh)?;
        let mesh = all.to_tri_mesh();
        self.record("union", mesh.content_hash());
        let materials = self
            .materials
            .trace(&mesh)
            .into_iter()
            .map(|m| m.and_then(Material::from_id))
            .collect::<Option<Vec<_>>>()
            .ok_or(KitError::Untraced)?;
        Ok((mesh, materials))
    }
}

/// A leaf solid from a mesh, relabelled with its slot (rule 2).
pub(crate) fn leaf(slot: Slot, mesh: &TriMesh) -> Result<Solid, KitError> {
    let s = dt_geom::Solid::from_tri_mesh(mesh).map_err(KitError::Mesh)?;
    relabel(slot, &s)
}

pub(crate) fn relabel(slot: Slot, s: &dt_geom::Solid) -> Result<Solid, KitError> {
    Ok(Solid(
        s.with_original_id(slot.id, slot.face_base)
            .map_err(KitError::Mesh)?,
    ))
}

#[cfg(test)]
mod tests {
    use super::Frame;

    #[test]
    #[cfg(target_pointer_width = "64")]
    fn nodes_do_not_wrap() {
        assert_eq!(Frame::dungeonblocks(1, 1).nodes(), (129, 129));
        // 2^25 × 128 = 2^32: wrapped to 1 node in u32
        let (nx, _) = Frame::dungeonblocks(1 << 25, 1).nodes();
        assert_eq!(nx as u64, (1u64 << 32) + 1);
    }
}
