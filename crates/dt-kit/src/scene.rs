//! The scene: the tile's frame, its ground surface (a height grid painted
//! by area) and the tagged solids elements add, with the source-ID block and
//! material table of `dt_geom::source`.

use dt_geom::{BoolOp, Grid, IdBlock, Mask, Seed, TriMesh, batch_in_order};

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
/// at this square, and scales with the system.
pub const REFERENCE_SQUARE_MM: f64 = 35.0;

/// The OpenLOCK square (one inch).
pub const OPENLOCK_SQUARE_MM: f64 = 25.4;

impl Frame {
    /// A DungeonBlocks frame at the default resolution (128 cells per
    /// square).
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
    /// Flowers.
    Flower = 6,
    /// Wall and floor stone.
    Masonry = 10,
    /// Branches and trunks.
    Wood = 11,
    /// Foliage clumps and leaves.
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
            Self::Flower => "flower",
            Self::Masonry => "masonry",
            Self::Wood => "wood",
            Self::Leaf => "leaf",
        }
    }

    /// The material called `name`.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        [
            Self::Earth,
            Self::Connector,
            Self::Rock,
            Self::Water,
            Self::Grass,
            Self::Flower,
            Self::Masonry,
            Self::Wood,
            Self::Leaf,
        ]
        .into_iter()
        .find(|m| m.name() == name)
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
            Self::Flower => [0.85, 0.45, 0.62],
            Self::Masonry => [0.66, 0.64, 0.60],
            Self::Wood => [0.42, 0.29, 0.18],
            Self::Leaf => [0.33, 0.52, 0.24],
        }
    }
}

/// What a solid is, as other elements read it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Kind {
    /// The lifted surface.
    Ground,
    /// The base under z = 0.
    Connector,
    /// Rocks, stones, boulders and pebbles.
    Rock,
    /// Grass.
    Grass,
    /// Reeds.
    Reeds,
    /// Flowers.
    Flowers,
    /// Trees.
    Tree,
    /// Walls.
    Wall,
    /// Floors.
    Floor,
    /// Cliffs.
    Cliff,
    /// The bank at a cliff's foot.
    Bank,
}

impl Kind {
    /// Every kind.
    pub const ALL: [Self; 11] = [
        Self::Ground,
        Self::Connector,
        Self::Rock,
        Self::Grass,
        Self::Reeds,
        Self::Flowers,
        Self::Tree,
        Self::Wall,
        Self::Floor,
        Self::Cliff,
        Self::Bank,
    ];

    /// The word a document writes for it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Ground => "ground",
            Self::Connector => "connector",
            Self::Rock => "rock",
            Self::Grass => "grass",
            Self::Reeds => "reeds",
            Self::Flowers => "flowers",
            Self::Tree => "tree",
            Self::Wall => "wall",
            Self::Floor => "floor",
            Self::Cliff => "cliff",
            Self::Bank => "bank",
        }
    }

    /// The kind a word names, if any.
    #[must_use]
    pub fn from_name(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.name() == word)
    }
}

impl std::fmt::Display for Kind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// A solid's tag: what kind of thing it is and the path of the stack entry
/// that made it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tag {
    /// What other elements read it as.
    pub kind: Kind,
    /// The stack path that made it, such as `tile/flat/stack/0`.
    pub owner: String,
}

/// A solid inside the mesh kernel. It never leaves the kernel between
/// operations: elements combine solids only through kit operations.
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

/// One source slot: an original ID, of one material (`dt_geom::source`
/// rule 1). Copy it into parallel work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slot {
    pub(crate) id: u32,
    pub(crate) material: Material,
}

/// Consecutive slots, all of one material, handed out in sequence.
#[derive(Clone, Copy, Debug)]
pub struct Slots {
    block: IdBlock,
    first: u32,
    len: u32,
    material: Material,
}

impl Slot {
    /// Its original (source) ID: the `run_original_id` of the triangles
    /// it made, in any mesh built from it.
    #[must_use]
    pub fn id(&self) -> u32 {
        self.id
    }

    /// What its solids are made of: fixed when the slot was reserved.
    #[must_use]
    pub fn material(&self) -> Material {
        self.material
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
            material: self.material,
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

/// A named stage hash: equal hashes mean byte-identical stages.
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
/// reference square.
pub const BANK_MM: f64 = 7.0;

/// Slots per build.
const SLOTS: u32 = 1 << 16;

/// The ground surface of one build: every paint in order, the banks
/// between them, the water's fill and bed, the edge contract and the
/// support things stand on.  Painted first; read once settled.
#[derive(Clone)]
pub struct Surface {
    frame: Frame,
    field: Grid,
    painted: Mask,
    /// Each paint, in order: its area and the full field it painted from
    /// (shared, so a kept scene costs little).
    layers: Vec<std::sync::Arc<(Mask, Vec<f64>)>>,
    settled: bool,
    banked: bool,
    /// Nodes the water covers.
    wet: Mask,
    bed: Option<Grid>,
    /// The water's own top at every node (world mm), a millimetre under
    /// the ground where there is no water: the water layer of the lift.
    water_top: Option<Vec<f32>>,
    water_edge_mm: Option<f64>,
    /// Nodes the ground's edge rule leaves alone (water and its banks).
    no_edge: Mask,
    stone: Mask,
    edge_mm: Option<f64>,
    support: Option<Grid>,
}

impl Surface {
    fn new(frame: Frame) -> Self {
        let (nx, ny) = frame.nodes();
        Self {
            frame,
            field: Grid::filled(nx, ny, frame.step_mm(), 0.0),
            painted: Mask::new(nx, ny, false),
            layers: Vec::new(),
            settled: false,
            banked: false,
            wet: Mask::new(nx, ny, false),
            bed: None,
            water_top: None,
            water_edge_mm: None,
            no_edge: Mask::new(nx, ny, false),
            stone: Mask::new(nx, ny, false),
            edge_mm: None,
            support: None,
        }
    }

    /// Bank once (after the scene has culled what the paints cover).
    fn bank(&mut self) {
        self.settle();
        if !self.banked {
            self.banked = true;
            self.banks();
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
            .get_or_insert_with(|| Grid::filled(nx, ny, step, f32::NEG_INFINITY));
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
        self.layers.push(std::sync::Arc::new((
            area.clone(),
            vec![level_mm; self.field.z.len()],
        )));
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
        self.layers
            .push(std::sync::Arc::new((area.clone(), field.z.clone())));
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

    /// **fill**: lay the water over the ground. `bed` is the ground under
    /// the water (used where nothing painted any), `top` the water's own
    /// surface (world mm at every node; under the ground where there is no
    /// water), `wet` the nodes it covers and `no_edge` the nodes the
    /// ground's edge rule leaves alone (the water and its banks meet the
    /// edge on their own contract). The lift makes the water a layer of its own over the
    /// ground, so the shore runs exactly where the two surfaces meet; the
    /// shown surface readers see is the higher of the two.
    pub fn fill(&mut self, bed: &crate::Field, top: &crate::Field, wet: &[bool], no_edge: &[bool]) {
        self.settle();
        if self.painted.count() == 0 {
            // nothing was painted: the ground under the water is the water's own default
            for (o, &v) in self.field.z.iter_mut().zip(&bed.z) {
                #[allow(clippy::cast_possible_truncation)]
                let z = v as f32;
                *o = z;
            }
        }
        if self.bed.is_none() {
            self.bed = Some(self.field.clone());
        }
        let water = self
            .water_top
            .get_or_insert_with(|| vec![f32::NEG_INFINITY; top.z.len()]);
        for k in 0..self.field.z.len() {
            #[allow(clippy::cast_possible_truncation)]
            let z = top.z[k] as f32;
            water[k] = water[k].max(z);
            self.field.z[k] = self.field.z[k].max(z);
            self.wet.bits[k] |= wet[k];
            self.painted.bits[k] |= wet[k];
            self.no_edge.bits[k] |= no_edge[k];
        }
    }

    /// Spread the paint, once.
    fn settle(&mut self) {
        if !self.settled {
            self.settled = true;
            self.spread_paint();
        }
    }

    /// Banks: where an area painted higher meets one painted lower, the
    /// higher ground runs down to the lower over a bank as wide as the
    /// step needs (no steeper than the ground's hills, 24° from 1.5 times
    /// the step, between 1 mm and [`BANK_MM`] at the reference square),
    /// cross-fading the two surfaces with a smoothstep. The lower side is
    /// untouched, so a pool keeps its bed and the bank is all on the land. Fields are the full ones each
    /// paint was given, so texture fades across the bank.
    fn banks(&mut self) {
        let n = self.layers.len();
        if n < 2 {
            return;
        }
        let k_all = self.field.z.len();
        // the last paint owning each node
        let mut owner = vec![usize::MAX; k_all];
        for (li, layer) in self.layers.iter().enumerate() {
            let area = &layer.0;
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

    /// The finished ground surface and its stone marks, for paint
    /// ([`crate::SurfacePaint::new`]); `None` when nothing painted it.
    #[must_use]
    pub fn surface(&self) -> Option<(dt_geom::Grid, Mask)> {
        (self.painted.count() > 0).then(|| (self.field.clone(), self.stone.clone()))
    }

    /// The edge contract, by construction: perimeter nodes exactly at the
    /// edge level, and nothing below it (rises only), except where the
    /// water and its banks meet the edge on their own contract.
    fn hold_edge(&mut self, edge: f64) {
        let mut z = std::mem::take(&mut self.field.z);
        self.hold(&mut z, edge);
        self.field.z = z;
    }

    /// [`Surface::hold_edge`] on any ground heights `z` of this lattice.
    fn hold(&self, z: &mut [f32], edge: f64) {
        #[allow(clippy::cast_possible_truncation)]
        let e = edge as f32;
        let (nx, ny) = (self.field.nx, self.field.ny);
        for j in 0..ny {
            for i in 0..nx {
                if self.no_edge.bits[j * nx + i] {
                    continue;
                }
                let z = &mut z[j * nx + i];
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
}

/// Something standing in the scene, by its bounds: where it is and how
/// far it reaches across.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Standing {
    /// What it is.
    pub kind: Kind,
    /// The middle of its bounds, mm.
    pub at: [f64; 2],
    /// Half its wider side across, mm.
    pub r: f64,
    /// Its bounds.
    pub lo: [f64; 3],
    /// See `lo`.
    pub hi: [f64; 3],
}

/// The solids of one build, in the order they were added, with the slots
/// and materials their triangles trace back to.  Append only (but for
/// what a later paint covers).
#[derive(Clone)]
pub struct Parts {
    frame: Frame,
    parts: Vec<Part>,
    /// For each part, how many paints came before it. A surface places its
    /// own solids (dirt's stones) and then paints, so a part belongs to the
    /// paint after it; a paint after that one over it removes it
    /// ([`Scene::bank`], [`Scene::lift`]).
    placed_at: Vec<usize>,
    /// For each part, the original IDs it carries: which part a triangle
    /// of the union came from ([`Scene::trace_parts`]).
    sources: Vec<Vec<u32>>,
    block: IdBlock,
    next_slot: u32,
    /// Each slot's material, by slot.
    materials: Vec<Material>,
}

impl Parts {
    fn new(frame: Frame) -> Self {
        Self {
            frame,
            parts: Vec::new(),
            placed_at: Vec::new(),
            sources: Vec::new(),
            block: IdBlock::reserve(SLOTS),
            next_slot: 0,
            materials: Vec::new(),
        }
    }

    /// **what stands here**: every part of the given kinds, by its bounds,
    /// in build order.  Grass, flowers, water and rocks ask this (or its
    /// rasters, [`Parts::footprint`] and [`Parts::standing`]) for what is
    /// already there.
    #[must_use]
    pub fn stands(&self, kinds: &[Kind]) -> Vec<Standing> {
        self.parts
            .iter()
            .filter(|p| kinds.contains(&p.tag.kind))
            .filter_map(|p| {
                let (lo, hi) = p.solid.bounds()?;
                Some(Standing {
                    kind: p.tag.kind,
                    at: [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0],
                    r: 0.5 * (hi[0] - lo[0]).max(hi[1] - lo[1]),
                    lo,
                    hi,
                })
            })
            .collect()
    }

    /// Add a part placed after `paints` paints; its mesh's hash.
    fn add(&mut self, tag: Tag, solid: Solid, paints: usize) -> u64 {
        let mesh = solid.0.to_tri_mesh();
        let hash = mesh.content_hash();
        let mut ids = mesh.run_original_id;
        ids.sort_unstable();
        ids.dedup();
        self.sources.push(ids);
        self.parts.push(Part { tag, solid });
        self.placed_at.push(paints);
        hash
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
            material,
        };
        self.next_slot += n;
        self.materials.extend((0..n).map(|_| material));
        Ok(s)
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

    /// Which part (an index into [`Scene::parts`]) each triangle of `mesh`
    /// (the [`Scene::union`]) came from, by its original ID; `u32::MAX`
    /// when none did.  The editor finds a triangle's stack entry this way.
    #[must_use]
    pub fn trace_parts(&self, mesh: &TriMesh) -> Vec<u32> {
        let mut of = std::collections::BTreeMap::new();
        for (p, ids) in self.sources.iter().enumerate() {
            for &id in ids {
                of.entry(id).or_insert(u32::try_from(p).unwrap_or(u32::MAX));
            }
        }
        mesh.triangle_original_ids().map_or_else(
            || vec![u32::MAX; mesh.triangle_count()],
            |ids| {
                ids.iter()
                    .map(|id| of.get(id).copied().unwrap_or(u32::MAX))
                    .collect()
            },
        )
    }

    /// Every part's mesh from `first` on, side by side, not joined, with
    /// each triangle's material: from 0, a draft's mesh; from a stage's
    /// first part, a sketch of what it added while the build runs.
    /// Overlaps stay overlapping; the view hides them, a printer would not.
    ///
    /// # Errors
    /// [`KitError::Untraced`] if a triangle's material is unknown.
    pub fn soup_since(&self, first: usize) -> Result<(TriMesh, Vec<Material>), KitError> {
        use rayon::prelude::*;
        let meshes: Vec<TriMesh> = self.parts[first.min(self.parts.len())..]
            .par_iter()
            .map(|p| p.solid.0.to_tri_mesh())
            .collect();
        let mut mesh = TriMesh::default();
        for m in meshes {
            mesh.append(&m);
        }
        let materials = self.material_of(&mesh)?;
        Ok((mesh, materials))
    }

    /// Each triangle's material, traced through the kernel's ids.
    fn material_of(&self, mesh: &TriMesh) -> Result<Vec<Material>, KitError> {
        let ids = mesh.triangle_original_ids().ok_or(KitError::Untraced)?;
        ids.iter()
            .map(|&id| {
                let slot = self.block.slot(id)?;
                self.materials.get(slot as usize).copied()
            })
            .collect::<Option<Vec<_>>>()
            .ok_or(KitError::Untraced)
    }

    /// The nodes inside every solid of `kind` at height `z`: their
    /// footprint, from a slice of each (the query "footprint by tag").
    #[must_use]
    pub fn footprint(&self, kind: Kind, z: f64) -> Mask {
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
    pub fn standing(&self, kinds: &[Kind], above_mm: f64) -> Mask {
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
}

/// The scene of one build: its [`Surface`], painted first, and the
/// [`Parts`] that stand on it.  A clone shares its solids and paints with
/// the original, so keeping one after each step (a build cache) is cheap.
#[derive(Clone)]
pub struct Scene {
    frame: Frame,
    seed: Seed,
    surface: Surface,
    built: Parts,
    notes: Vec<(String, f64)>,
    stages: Vec<Stage>,
    progress: crate::Progress,
}

impl Scene {
    /// An empty scene. Reserves its source-ID block, so call it in
    /// sequence, not from parallel tasks.
    #[must_use]
    pub fn new(frame: Frame, seed: Seed) -> Self {
        Self {
            frame,
            seed,
            surface: Surface::new(frame),
            built: Parts::new(frame),
            notes: Vec::new(),
            stages: Vec::new(),
            progress: crate::Progress::none(),
        }
    }

    /// The ground surface.
    #[must_use]
    pub const fn ground_surface(&self) -> &Surface {
        &self.surface
    }

    /// The parts so far, with the "what stands here" query.
    #[must_use]
    pub const fn built(&self) -> &Parts {
        &self.built
    }

    /// **what stands here**: see [`Parts::stands`].
    #[must_use]
    pub fn stands(&self, kinds: &[Kind]) -> Vec<Standing> {
        self.built.stands(kinds)
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

    /// **stamp**: a solid's footprint and top (world mm): the support
    /// under it rises to `top_mm` (never falls). Things that stand on
    /// others (a wall on a floor) and things that grow round them read
    /// it through [`Scene::support`].
    pub fn stamp(&mut self, footprint: &Mask, top_mm: f64) {
        self.surface.stamp(footprint, top_mm)
    }

    /// The highest stamped top at each node (world mm, `-∞` where nothing
    /// is stamped), or `None` before any stamp.
    #[must_use]
    pub fn support(&self) -> Option<&Grid> {
        self.surface.support()
    }

    /// The ground surface as painted (world mm at every node), before the
    /// lift spreads paint to unpainted nodes; `None` before any paint.
    #[must_use]
    pub fn surface_field(&self) -> Option<&Grid> {
        self.surface.surface_field()
    }

    /// **paint**: set the ground surface to `level_mm` (already scaled) over
    /// `area`. Later paints win, so stack order decides which surface paints
    /// over which.
    pub fn paint(&mut self, area: &Mask, level_mm: f64) {
        self.surface.paint(area, level_mm)
    }

    /// **paint** a field: set the ground surface to `field` (world mm) over
    /// `area`. Later paints win.
    pub fn paint_field(&mut self, area: &Mask, field: &crate::Field) {
        self.surface.paint_field(area, field)
    }

    /// The ground surface (world mm): every paint, spread to the nodes no
    /// area painted (and banked, once [`Scene::bank`] has run). Read it
    /// after the surfaces are painted (the scheduler paints every surface
    /// before any solid); `None` when nothing painted it.
    pub fn ground(&mut self) -> Option<crate::Field> {
        self.surface.ground()
    }

    /// **fill**: lay the water over the ground ([`Surface::fill`]).
    pub fn fill(&mut self, bed: &crate::Field, top: &crate::Field, wet: &[bool], no_edge: &[bool]) {
        self.surface.fill(bed, top, wet, no_edge)
    }

    /// Mark surface nodes as stone (pebbles) for the painted preview;
    /// unmark the rest of `area`.
    pub fn mark_stone(&mut self, area: &Mask, stone: &[bool]) {
        self.surface.mark_stone(area, stone)
    }

    /// The edge contract: the level (world mm) every ground surface meets
    /// the tile edge at, and never dips below. `None`: no contract.
    pub const fn set_edge(&mut self, level_mm: Option<f64>) {
        self.surface.set_edge(level_mm)
    }

    /// The edge level, world mm, if the tile has an edge contract.
    #[must_use]
    pub const fn edge(&self) -> Option<f64> {
        self.surface.edge()
    }

    /// The level (world mm) the water meets every tile edge at, before
    /// its tide profile (the edge contract's `water` rule).
    pub const fn set_water_edge(&mut self, level_mm: Option<f64>) {
        self.surface.set_water_edge(level_mm)
    }

    /// See [`Scene::set_water_edge`].
    #[must_use]
    pub const fn water_edge(&self) -> Option<f64> {
        self.surface.water_edge()
    }

    /// The ground as it was before water filled it (world mm): the bed
    /// under the water and the bank round it; `None` without water.
    #[must_use]
    pub const fn bed(&self) -> Option<&Grid> {
        self.surface.bed()
    }

    /// The nodes the water covers (empty without water).
    #[must_use]
    pub const fn wet(&self) -> &Mask {
        self.surface.wet()
    }

    /// The finished ground surface and its stone marks, for paint
    /// ([`crate::SurfacePaint::new`]); `None` when nothing painted it.
    #[must_use]
    pub fn surface(&self) -> Option<(dt_geom::Grid, Mask)> {
        self.surface.surface()
    }

    /// Reserve `n` slots of one material, in sequence.
    ///
    /// # Errors
    /// [`KitError::Slots`] when the build's block is used up.
    pub fn slots(&mut self, n: u32, material: Material) -> Result<Slots, KitError> {
        self.built.slots(n, material)
    }

    /// **top**: the highest z of every solid so far (0 if none).
    #[must_use]
    pub fn top(&self) -> f64 {
        self.built.top()
    }

    /// The parts so far, in build order.
    #[must_use]
    pub fn parts(&self) -> &[Part] {
        self.built.parts()
    }

    /// Which part (an index into [`Scene::parts`]) each triangle of `mesh`
    /// (the [`Scene::union`]) came from, by its original ID; `u32::MAX`
    /// when none did.  The editor finds a triangle's stack entry this way.
    #[must_use]
    pub fn trace_parts(&self, mesh: &TriMesh) -> Vec<u32> {
        self.built.trace_parts(mesh)
    }

    /// Every part's mesh from `first` on, side by side, not joined, with
    /// each triangle's material: from 0, a draft's mesh; from a stage's
    /// first part, a sketch of what it added while the build runs.
    /// Overlaps stay overlapping; the view hides them, a printer would not.
    ///
    /// # Errors
    /// [`KitError::Untraced`] if a triangle's material is unknown.
    pub fn soup_since(&self, first: usize) -> Result<(TriMesh, Vec<Material>), KitError> {
        self.built.soup_since(first)
    }

    /// The nodes inside every solid of `kind` at height `z`: their
    /// footprint, from a slice of each (the query "footprint by tag").
    #[must_use]
    pub fn footprint(&self, kind: Kind, z: f64) -> Mask {
        self.built.footprint(kind, z)
    }

    /// The nodes inside any solid of the given kinds `above_mm` over its
    /// own lowest point: where things stand (a rock's outline where it
    /// meets the ground, a trunk, a wall's foot). Grass roots no blade
    /// there and stops its blades against it.
    #[must_use]
    pub fn standing(&self, kinds: &[Kind], above_mm: f64) -> Mask {
        self.built.standing(kinds, above_mm)
    }

    /// **bank** (field blend): run the higher areas down to the lower ones,
    /// once. The water calls it before it reads the land, so a shore is a
    /// bank; plain ground keeps its steps (Phase 2).
    pub fn bank(&mut self) {
        self.cull_covered();
        self.surface.bank();
    }

    /// Attach a progress channel (the evaluator does, for a front end that
    /// wants one).  Never read by geometry: a build is the same without it.
    pub fn set_progress(&mut self, progress: crate::Progress) {
        self.progress = progress;
    }

    /// The progress channel, to report through (an element with a natural loop
    /// takes `scene.progress().counter(..)` and ticks it).  Reports nothing
    /// unless a front end attached one.
    #[must_use]
    pub fn progress(&self) -> crate::Progress {
        self.progress.clone()
    }

    /// Record an element's number for the build record (counts and the like).
    pub fn note(&mut self, name: &str, value: f64) {
        self.notes.push((name.to_owned(), value));
    }

    /// The numbers elements noted, in order.
    #[must_use]
    pub fn notes(&self) -> &[(String, f64)] {
        &self.notes
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

    /// **lift**: the painted surface as one solid from z = 0 (the uniform
    /// lift), tagged `ground`. Nodes no area painted take the level of the
    /// nearest painted node (breadth first, so drawn lines take their
    /// neighbours' level). Does nothing if nothing was painted.
    ///
    /// # Errors
    /// [`KitError`] if the surface is not above 0 or the kernel refuses it.
    pub fn lift(&mut self, owner: &str) -> Result<(), KitError> {
        if self.surface.painted.count() == 0 {
            return Ok(());
        }
        self.cull_covered();
        self.surface.settle();
        if let Some(edge) = self.surface.edge_mm {
            self.surface.hold_edge(edge);
        }
        self.record("surface", self.surface.field.content_hash());
        let slot = self.built.slots(1, Material::Earth)?.get(0);
        let field = &self.surface.field;
        // the earth: the ground as it was before the water filled it, held
        // to the edge contract (the water and its banks excepted)
        let earth = match &self.surface.bed {
            Some(bed) => {
                let mut e = bed.z.clone();
                if let Some(edge) = self.surface.edge_mm {
                    self.surface.hold(&mut e, edge);
                }
                e
            }
            None => field.z.clone(),
        };
        // the water: a layer of its own over the earth, wherever its top is
        // above it
        let water = match &self.surface.water_top {
            Some(w) if self.surface.wet.count() > 0 => {
                Some((w.clone(), self.built.slots(1, Material::Water)?.get(0)))
            }
            _ => None,
        };
        let mut layers = vec![dt_geom::Layer {
            top: &earth,
            id: slot.id,
        }];
        if let Some((w, ws)) = &water {
            layers.push(dt_geom::Layer { top: w, id: ws.id });
        }
        // one closed mesh, a run per layer; vertices rounded to float32 once
        // so the terrain alone survives the STL reload
        let mesh = dt_geom::lift_stack(
            &dt_geom::Lattice {
                nx: field.nx,
                ny: field.ny,
                step: field.step,
                tol: LIFT_TOL_MM,
                max_cells: LIFT_MAX_CELLS,
                round: dt_geom::Round::F32,
            },
            &layers,
        )
        .map_err(|e| KitError::Surface(e.to_string()))?;
        let solid = Solid(dt_geom::Solid::from_tri_mesh(&mesh).map_err(KitError::Mesh)?);
        self.add(
            Tag {
                kind: Kind::Ground,
                owner: owner.to_owned(),
            },
            solid,
        );
        Ok(())
    }

    /// Remove each solid a surface placed (before a later paint) that a
    /// later paint covers within the solid's reach plus a bank's width:
    /// the ground it sat on is gone, so it would float or bury. Solids
    /// placed after every paint are never removed.
    fn cull_covered(&mut self) {
        let n = self.surface.layers.len();
        if self.built.placed_at.iter().all(|&p| p + 1 >= n) {
            return;
        }
        let (nx, ny) = self.frame.nodes();
        let step = self.frame.step_mm();
        let margin = BANK_MM * self.frame.scale();
        let mut keep = Vec::with_capacity(self.built.parts.len());
        for (part, &at) in self.built.parts.iter().zip(&self.built.placed_at) {
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
                            self.surface.layers[at + 1..]
                                .iter()
                                .any(|layer| layer.0.bits[j * nx + i])
                        })
                    })
                });
            keep.push(!covered);
        }
        let mut it = keep.iter();
        self.built.parts.retain(|_| *it.next().unwrap_or(&true));
        let mut it = keep.iter();
        self.built.placed_at.retain(|_| *it.next().unwrap_or(&true));
        let mut it = keep.iter();
        self.built.sources.retain(|_| *it.next().unwrap_or(&true));
    }

    /// Add a solid an element built, and record its stage hash.
    pub fn add(&mut self, tag: Tag, solid: Solid) {
        let name = format!("{}:{}", tag.kind, tag.owner);
        let hash = self.built.add(tag, solid, self.surface.layers.len());
        self.record(&name, hash);
    }

    /// **union**: every part combined in build order (`batch_in_order`),
    /// exported once, with each triangle's material. Ends the scene's
    /// time in the kernel; the export is what the audit takes.
    ///
    /// # Errors
    /// [`KitError::Untraced`] if a triangle has no material, or the
    /// kernel's status.
    pub fn union(&mut self) -> Result<(TriMesh, Vec<Material>), KitError> {
        let solids: Vec<dt_geom::Solid> =
            self.built.parts.iter().map(|p| p.solid.0.clone()).collect();
        let all = batch_in_order(&solids, BoolOp::Union);
        all.status().map_err(KitError::Mesh)?;
        let mesh = all.to_tri_mesh();
        self.record("union", mesh.content_hash());
        let materials = self.built.material_of(&mesh)?;
        Ok((mesh, materials))
    }
}

/// A leaf solid from a mesh, relabelled with its slot (rule 2).
pub(crate) fn leaf(slot: Slot, mesh: &TriMesh) -> Result<Solid, KitError> {
    let s = dt_geom::Solid::from_tri_mesh(mesh).map_err(KitError::Mesh)?;
    relabel(slot, &s)
}

pub(crate) fn relabel(slot: Slot, s: &dt_geom::Solid) -> Result<Solid, KitError> {
    Ok(Solid(s.with_original_id(slot.id).map_err(KitError::Mesh)?))
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
