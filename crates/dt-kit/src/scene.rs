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

/// A frame is a key as it stands (its square's size by its bits), for what
/// is kept under the frame it was made for.
impl std::hash::Hash for Frame {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.cols.hash(state);
        self.rows.hash(state);
        self.square_mm.to_bits().hash(state);
        self.cells_per_square.hash(state);
    }
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

    /// The nodes inside the box `lo..hi` (mm) grown by `grow` mm, as the
    /// column and row ranges: `(i0..i1, j0..j1)`, empty where the box is
    /// off the tile.
    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    pub fn node_range(
        &self,
        lo: [f64; 2],
        hi: [f64; 2],
        grow: f64,
    ) -> (std::ops::Range<usize>, std::ops::Range<usize>) {
        let (nx, ny) = self.nodes();
        let step = self.step_mm();
        let range = |a: f64, b: f64, n: usize| {
            let first = ((a - grow) / step).ceil().max(0.0).min(n as f64) as usize;
            let end = (((b + grow) / step).floor().max(-1.0) + 1.0).min(n as f64) as usize;
            first..end.max(first)
        };
        (range(lo[0], hi[0], nx), range(lo[1], hi[1], ny))
    }

    /// How much an author's mm grow or shrink on this system.
    #[must_use]
    pub fn scale(&self) -> f64 {
        self.square_mm / REFERENCE_SQUARE_MM
    }

    /// The plan round of the tile's four vertical corners on this system,
    /// mm ([`CORNER_ROUND_MM`] scaled).
    #[must_use]
    pub fn corner_round_mm(&self) -> f64 {
        CORNER_ROUND_MM * self.scale()
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
    Terrain,
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
        Self::Terrain,
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
            Self::Terrain => "terrain",
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

/// How far round a node the grain's greatest height is looked for, mm at the
/// 35 mm square (it scales with the system, like the grain it looks for).
const GRAIN_REACH_MM: f64 = 6.0;
/// The sigma that height is smoothed by, mm at the 35 mm square (scaled).
const GRAIN_SMOOTH_MM: f64 = 3.0;
/// The lift's error bound, mm: no surface node is further than this from
/// the triangulated top (a tenth of a 0.2 mm layer).
pub const LIFT_TOL_MM: f64 = 0.02;
/// The wavelength of the relief on the tile's sides, true mm: the columns
/// of a side panel (and of the top's rim) are this far apart.
pub const SIDE_WAVE_MM: f64 = 1.0;
/// The size of a lump of side relief, mm.
const SIDE_LUMP_MM: f64 = 1.8;
/// The tile's four vertical corners are rounded in plan by this, mm at the
/// 35 mm square; it scales with the system like every size
/// ([`Frame::corner_round_mm`]). The terrain's lift and the base share it.
pub const CORNER_ROUND_MM: f64 = 0.5;
/// The widest leaf of the adaptive lift, in cells.
const LIFT_MAX_CELLS: usize = 32;
/// Slots per build.
const SLOTS: u32 = 1 << 16;

/// Where a fluid layer is absent its top lies this far under the land, mm at
/// the 35 mm square (it scales with the system): the stack mesher puts the
/// layer's boundary where the top, interpolated to it, meets the layer
/// below's. Whether a layer shows is its `present` flags, not this gap.
pub const ABSENT_UNDER_MM: f64 = 1.0;

/// The most grain reaches under a surface over a layer `thick` mm thick,
/// mm: half the thickness, and never through the skin at the bottom.
fn grain_cap(thick: f64, skin_mm: f64) -> f64 {
    (0.5 * thick).min(thick - skin_mm).max(0.0)
}

/// One layer to lay on the terrain: what it is made of, where it exists and
/// the surface its body made inside its band.
#[derive(Clone, Copy)]
pub struct LayerSpec<'a> {
    /// The stack entry that owns it.
    pub owner: &'a str,
    /// Its material; the layer's slot carries it.
    pub material: Material,
    /// A fluid fills the empty space under its top, over the land laid
    /// before it; a land layer is the higher of what is below and its own top.
    pub fluid: bool,
    /// Where the layer exists (its region); elsewhere it is absent.
    pub mask: &'a Mask,
    /// The body's surface, inside its band by type.
    pub shape: &'a crate::Banded,
    /// The material's grain, set into the surface by the kit (a
    /// displacement of the one surface, not separate solids).
    pub grain: Option<&'a Relief<'a>>,
    /// The printer's skin, mm: the thinnest layer that prints as its colour.
    pub skin_mm: f64,
    /// The material's edge-round, mm: a drop of the top near the rim, cut
    /// from the sharp corner at the edge height, tapered to none on a layer
    /// thinner than the skin, mitred at corners. Zero for none.
    pub round_mm: f64,
    /// The relief of the tile's sides in this material, or `None` for a
    /// smooth face.
    pub side: Option<SideSpec>,
}

/// The relief a material gives the tile's sides, below the edge-round.
#[derive(Clone, Copy, Debug)]
pub struct SideSpec {
    /// How far a face is set back behind its plane at most, mm.
    pub depth_mm: f64,
    /// The relief's own seed.
    pub seed: Seed,
}

/// A material's grain as the kit sets it in: its relief over the surface, and where the surface
/// is set in to hold it.
///
/// The kit moves the surface in by the relief's greatest height and adds the
/// relief back, so the grain never rises above the surface the band holds.
/// It is capped at half the layer's thickness and at the printer's skin, and
/// is never taken into the layer below (the base for the first land layer).
/// Vertical displacement: the normal of gentle ground is near vertical.
#[derive(Clone, Copy)]
pub struct Relief<'a> {
    /// The relief at every node, mm: up where it stands, down where it is
    /// cut in. Zero where there is no grain.
    pub relief: &'a crate::Field,
    /// Where the surface is set in, 0 to 1: zero where the surface is held
    /// exactly (a pin, the rim) or covered by water.
    pub weight: &'a crate::Field,
    /// The grain never takes the surface under this (mm at every node, or
    /// `-inf`): where water will lie over the land, a little above the water,
    /// so dry ground stays dry.
    pub floor: Option<&'a crate::Field>,
}

/// One layer laid: its slot, its region and the top the lift takes
/// (float32 once): a land layer's is the land as it stood once the layer
/// was laid (the higher of every land layer so far inside its region), a
/// fluid's its own surface where it exists.
#[derive(Clone)]
pub(crate) struct Laid {
    slot: Slot,
    fluid: bool,
    side: Option<SideSpec>,
    mask: Mask,
    top: std::sync::Arc<Vec<f32>>,
}

/// The terrain of one build: the layers laid in order (land bodies, then
/// fluids), the land they make together, and the support things stand on.  Laid first; read once settled.
#[derive(Clone)]
pub struct Terrain {
    frame: Frame,
    layers: Vec<Laid>,
    /// The higher of every land layer laid, inside its region (zero at first).
    land: Grid,
    /// The surface shown: the land, and a fluid over it where it is higher.
    shown: Grid,
    any_land: bool,
    /// Nodes a fluid covers.
    wet: Mask,
    stone: Mask,
    support: Option<Grid>,
    /// How the support at each node is painted, set by the first stamp
    /// there.
    under: Vec<Under>,
    rest: Option<Rest>,
}

/// How the ground under what is stamped on it is painted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Under {
    /// The flat bedding colour (a floor's slabs stand on it).
    #[default]
    Bedding,
    /// The earth's own paint, sides and ground in its grain (a cliff's
    /// stones stand on earth that looks like earth).
    Earth,
}

/// What a pieces body's tops give a layer laid on it (a dirt film on
/// stone): the tops, and where a film may lie.
#[derive(Clone, Debug)]
pub struct Rest {
    /// The top of the pieces at every node (world mm, `-∞` where none).
    pub tops: Vec<f64>,
    /// Where it is rock under the steepest slope a film lies on, clear of
    /// the lips and faces.
    pub flat: Mask,
    /// Where the pieces' core stands: nothing is set down on it.
    pub core: Mask,
}

impl Terrain {
    fn new(frame: Frame) -> Self {
        let (nx, ny) = frame.nodes();
        Self {
            frame,
            layers: Vec::new(),
            land: Grid::filled(nx, ny, frame.step_mm(), 0.0),
            shown: Grid::filled(nx, ny, frame.step_mm(), 0.0),
            any_land: false,
            wet: Mask::new(nx, ny, false),
            stone: Mask::new(nx, ny, false),
            support: None,
            under: vec![Under::Bedding; nx * ny],
            rest: None,
        }
    }

    /// **rest**: what a pieces body's tops give the layers laid after it.
    pub fn set_rest(&mut self, rest: Rest) {
        self.rest = Some(rest);
    }

    /// **stamp**: a solid's footprint and top (world mm): the support
    /// under it rises to `top_mm` (never falls), painted as bedding. Things
    /// that stand on others (a wall on a floor) and things that grow round
    /// them read it through [`Scene::support`].
    pub fn stamp(&mut self, footprint: &Mask, top_mm: f64) {
        for (k, _) in footprint.bits.iter().enumerate().filter(|(_, b)| **b) {
            self.raise(k, top_mm, Under::Bedding);
        }
    }

    /// **stamp_heights**: a pieces body's tops, one per node (world mm,
    /// `-∞` where it has none): the support rises to each, never falls. A
    /// node nothing has stamped before takes `under` as its paint; a stamped
    /// one keeps the paint of the first stamp.
    pub fn stamp_heights(&mut self, tops: &[f64], under: Under) {
        for (k, &t) in tops.iter().enumerate().filter(|(_, t)| t.is_finite()) {
            self.raise(k, t, under);
        }
    }

    fn raise(&mut self, k: usize, top_mm: f64, under: Under) {
        let (nx, ny) = self.frame.nodes();
        let step = self.frame.step_mm();
        let s = self
            .support
            .get_or_insert_with(|| Grid::filled(nx, ny, step, f32::NEG_INFINITY));
        if s.z[k] == f32::NEG_INFINITY {
            self.under[k] = under;
        }
        #[allow(clippy::cast_possible_truncation)]
        let z = top_mm as f32;
        s.z[k] = s.z[k].max(z);
    }

    /// Where the support is painted as bedding: stamped, and not earth.
    #[must_use]
    pub fn bedded(&self) -> Option<Vec<bool>> {
        let s = self.support.as_ref()?;
        Some(
            s.z.iter()
                .zip(&self.under)
                .map(|(z, u)| z.is_finite() && *u == Under::Bedding)
                .collect(),
        )
    }

    /// The highest stamped top at each node (world mm, `-∞` where nothing
    /// is stamped), or `None` before any stamp.
    #[must_use]
    pub fn support(&self) -> Option<&Grid> {
        self.support.as_ref()
    }

    /// The surface shown (world mm at every node): the land, and a fluid
    /// over it where it is higher; `None` before any layer is laid.
    #[must_use]
    pub fn surface_field(&self) -> Option<&Grid> {
        (!self.layers.is_empty()).then_some(&self.shown)
    }

    /// The land alone (world mm): every land layer laid, the higher of
    /// them inside its region; `None` before any land is laid.
    #[must_use]
    pub fn land(&self) -> Option<crate::Field> {
        self.any_land.then(|| crate::Field::from(&self.land))
    }

    /// The land as it lies under the fluids (the bed under the water and
    /// the bank round it); `None` without a fluid.
    #[must_use]
    pub fn bed(&self) -> Option<&Grid> {
        self.layers.iter().any(|l| l.fluid).then_some(&self.land)
    }

    /// The nodes a fluid covers (empty without one).
    #[must_use]
    pub const fn wet(&self) -> &Mask {
        &self.wet
    }

    /// Mark surface nodes as stone (pebbles) for the painted preview;
    /// unmark the rest of `area`.
    pub fn mark_stone(&mut self, area: &Mask, stone: &[bool]) {
        for (k, _) in area.bits.iter().enumerate().filter(|(_, b)| **b) {
            self.stone.bits[k] = stone[k];
        }
    }

    /// The finished surface and its stone marks, for paint
    /// ([`crate::SurfacePaint::new`]); `None` when nothing is laid.
    #[must_use]
    pub fn surface(&self) -> Option<(Grid, Mask)> {
        (!self.layers.is_empty()).then(|| (self.shown.clone(), self.stone.clone()))
    }

    /// How far a layer's grain sets its surface in at every node, and the
    /// most the grain reaches under it (both mm, zero where it has none):
    /// the surface moves in by the relief's greatest height (where the
    /// weight is whole), capped.
    fn set_in(&self, spec: &LayerSpec<'_>) -> (Vec<f64>, Vec<f64>) {
        let n = self.land.z.len();
        let (mut off, mut cap) = (vec![0.0; n], vec![0.0; n]);
        let Some(g) = spec.grain else {
            return (off, cap);
        };
        let shape = spec.shape.field();
        // the grain's greatest height near each node (the highest within
        // GRAIN_REACH_MM, smoothed), so the surface is set in only as far as
        // the grain beside it stands: its high points meet the surface the
        // band holds everywhere, the rim included, and the land follows it
        let mut stands = g.relief.clone();
        for k in 0..n {
            if !(spec.mask.bits[k] && g.weight.z[k] > 0.5) {
                stands.z[k] = 0.0;
            }
        }
        let scale = self.frame.scale();
        let height = stands
            .dilate(GRAIN_REACH_MM * scale)
            .blur(GRAIN_SMOOTH_MM * scale);
        for k in (0..n).filter(|&k| spec.mask.bits[k]) {
            let s = shape.z[k];
            let below = if spec.fluid {
                0.0
            } else {
                f64::from(self.land.z[k])
            };
            cap[k] = grain_cap(s - below, spec.skin_mm);
            if let Some(f) = g.floor {
                cap[k] = cap[k].min((s - f.z[k]).max(0.0));
            }
            off[k] = height.z[k].min(cap[k]) * g.weight.z[k];
        }
        (off, cap)
    }

    /// **lay**: add a layer on top of those laid.  A land layer's surface is
    /// the higher of the land below and its own, inside its region; a
    /// fluid's is its own where it exists.
    #[allow(clippy::needless_range_loop)]
    fn lay(&mut self, slot: Slot, spec: &LayerSpec<'_>) {
        let n = self.land.z.len();
        let shape = spec.shape.field();
        let (off, cap) = self.set_in(spec);
        #[allow(clippy::cast_possible_truncation)]
        let mut top: Vec<f32> = (0..n)
            .map(|k| {
                let s = shape.z[k];
                spec.grain.filter(|_| spec.mask.bits[k]).map_or(s, |g| {
                    // never above the surface, never more than the cap under it
                    (s - off[k] + g.relief.z[k]).clamp(s - cap[k], s)
                }) as f32
            })
            .collect();
        if spec.round_mm > 0.0 && !spec.fluid {
            // the edge-round: a drop near the rim, cut from the sharp corner
            // (the distance to the nearest edge, so it is mitred at a corner)
            let rim = crate::rim(&self.frame);
            let r = spec.round_mm;
            #[allow(clippy::cast_possible_truncation)]
            for k in (0..n).filter(|&k| spec.mask.bits[k] && rim.z[k] < r) {
                let drop = dt_geom::rim_profile(rim.z[k], r);
                let thick = f64::from(top[k]) - f64::from(self.land.z[k]);
                let taper = crate::smooth01((thick - spec.skin_mm) / spec.skin_mm.max(1e-9));
                top[k] -= (drop * taper) as f32;
            }
        }
        if spec.fluid {
            for k in 0..n {
                let on = spec.mask.bits[k] && top[k] >= self.land.z[k];
                self.wet.bits[k] |= on;
                if on {
                    self.shown.z[k] = self.shown.z[k].max(top[k]);
                }
            }
        } else {
            for k in 0..n {
                if spec.mask.bits[k] {
                    self.land.z[k] = self.land.z[k].max(top[k]);
                    self.shown.z[k] = self.shown.z[k].max(top[k]);
                }
            }
            self.any_land = true;
        }
        // what the lift takes of a land layer is the land as it now stands
        let top = if spec.fluid { top } else { self.land.z.clone() };
        self.layers.push(Laid {
            slot,
            fluid: spec.fluid,
            side: spec.side,
            mask: spec.mask.clone(),
            top: std::sync::Arc::new(top),
        });
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

    /// Add a part; its mesh's hash.
    fn add(&mut self, tag: Tag, solid: Solid) -> u64 {
        let mesh = solid.0.to_tri_mesh();
        let hash = mesh.content_hash();
        let mut ids = mesh.run_original_id;
        ids.sort_unstable();
        ids.dedup();
        self.sources.push(ids);
        self.parts.push(Part { tag, solid });
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

/// The scene of one build: its [`Terrain`], laid first, and the
/// [`Parts`] that stand on it.  A clone shares its solids and layers with
/// the original, so keeping one after each step (a build cache) is cheap.
#[derive(Clone)]
pub struct Scene {
    frame: Frame,
    seed: Seed,
    terrain: Terrain,
    built: Parts,
    notes: Vec<(String, f64)>,
    left_out: Vec<(String, usize)>,
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
            terrain: Terrain::new(frame),
            built: Parts::new(frame),
            notes: Vec::new(),
            left_out: Vec::new(),
            stages: Vec::new(),
            progress: crate::Progress::none(),
        }
    }

    /// The terrain.
    #[must_use]
    pub const fn terrain(&self) -> &Terrain {
        &self.terrain
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
        self.terrain.stamp(footprint, top_mm)
    }

    /// **stamp_heights**: a pieces body's tops, one per node (`-∞` where it
    /// has none), and how the ground under them is painted; see
    /// [`Terrain::stamp_heights`].
    pub fn stamp_heights(&mut self, tops: &[f64], under: Under) {
        self.terrain.stamp_heights(tops, under);
    }

    /// Where the support is painted as bedding.
    #[must_use]
    pub fn bedded(&self) -> Option<Vec<bool>> {
        self.terrain.bedded()
    }

    /// The highest stamped top at each node (world mm, `-∞` where nothing
    /// is stamped), or `None` before any stamp.
    #[must_use]
    pub fn support(&self) -> Option<&Grid> {
        self.terrain.support()
    }

    /// The ground surface as painted (world mm at every node), before the
    /// lift spreads paint to unpainted nodes; `None` before any paint.
    #[must_use]
    pub fn surface_field(&self) -> Option<&Grid> {
        self.terrain.surface_field()
    }

    /// **rest**: a pieces body gives the layers laid after it its tops.
    pub fn set_rest(&mut self, rest: Rest) {
        self.terrain.set_rest(rest);
    }

    /// What the pieces bodies laid so far give a layer laid on them, if any
    /// does.
    #[must_use]
    pub fn rest(&self) -> Option<&Rest> {
        self.terrain.rest.as_ref()
    }

    /// **lay**: add a layer to the terrain, in stack order: land bodies
    /// first, fluids last.  Its slot (and so its material) is reserved here,
    /// single-threaded; its surface is the body's, inside its band by type.
    ///
    /// # Errors
    /// [`KitError::Slots`] when the build's block is used up.
    pub fn lay(&mut self, spec: &LayerSpec<'_>) -> Result<(), KitError> {
        let slot = self.built.slots(1, spec.material)?.get(0);
        self.terrain.lay(slot, spec);
        Ok(())
    }

    /// How far a layer's grain would set its surface in at every node, mm
    /// (see [`Relief`]): what lies on the grain (a stone bedded into the
    /// dirt) sits that much lower.
    #[must_use]
    pub fn grain_offset(&self, spec: &LayerSpec<'_>) -> crate::Field {
        let (off, _) = self.terrain.set_in(spec);
        <crate::Field as crate::OnFrame>::new(&self.frame, 0.0).with_z(off)
    }

    /// The land alone (world mm): every land layer laid so far. Read it
    /// after the land is laid (the scheduler lays every land layer before any
    /// solid); `None` when none is.
    #[must_use]
    pub fn land(&self) -> Option<crate::Field> {
        self.terrain.land()
    }

    /// **the land view**: the land as laid and what stands on it (see
    /// [`crate::Land`]); `None` before any land is laid.
    #[must_use]
    pub fn land_view(&self) -> Option<crate::Land<'_>> {
        let top = self.terrain.land()?;
        Some(crate::Land::new(self.frame, top, self.built.parts()))
    }

    /// Mark surface nodes as stone (pebbles) for the painted preview;
    /// unmark the rest of `area`.
    pub fn mark_stone(&mut self, area: &Mask, stone: &[bool]) {
        self.terrain.mark_stone(area, stone)
    }

    /// The ground as it was before water filled it (world mm): the bed
    /// under the water and the bank round it; `None` without water.
    #[must_use]
    pub fn bed(&self) -> Option<&Grid> {
        self.terrain.bed()
    }

    /// The nodes the water covers (empty without water).
    #[must_use]
    pub const fn wet(&self) -> &Mask {
        self.terrain.wet()
    }

    /// The finished ground surface and its stone marks, for paint
    /// ([`crate::SurfacePaint::new`]); `None` when nothing painted it.
    #[must_use]
    pub fn surface(&self) -> Option<(dt_geom::Grid, Mask)> {
        self.terrain.surface()
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

    /// Count `n` pieces an element left out because it could not make them
    /// (a hull or a band that came out degenerate). Counted by element;
    /// nothing is recorded for zero.
    pub fn left_out(&mut self, element: &str, n: usize) {
        if n == 0 {
            return;
        }
        match self.left_out.iter_mut().find(|(e, _)| e == element) {
            Some((_, c)) => *c += n,
            None => self.left_out.push((element.to_owned(), n)),
        }
    }

    /// What elements left out, by element, in the order first counted.
    #[must_use]
    pub fn left_outs(&self) -> &[(String, usize)] {
        &self.left_out
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

    /// **lift**: the terrain as one closed solid from z = 0 (the stack
    /// mesher), a run per layer, tagged `ground`. Does nothing if nothing is
    /// laid.
    ///
    /// # Errors
    /// [`KitError`] if the surface is not above 0 or the kernel refuses it.
    pub fn lift(&mut self, owner: &str, printer: &crate::Printer) -> Result<(), KitError> {
        if self.terrain.layers.is_empty() {
            return Ok(());
        }
        self.record("surface", self.terrain.shown.content_hash());
        let t = &self.terrain;
        let mesh = lift_layers(&t.layers, &t.land, self.frame, printer)?;
        let solid = Solid(dt_geom::Solid::from_tri_mesh(&mesh).map_err(KitError::Mesh)?);
        self.add(
            Tag {
                kind: Kind::Terrain,
                owner: owner.to_owned(),
            },
            solid,
        );
        Ok(())
    }

    /// Add a solid an element built, and record its stage hash.
    pub fn add(&mut self, tag: Tag, solid: Solid) {
        let name = format!("{}:{}", tag.kind, tag.owner);
        let hash = self.built.add(tag, solid);
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

/// The layers laid, as one closed mesh from z = 0: the land layers then the
/// fluids (each in the order laid), a run per layer, each layer's side
/// relief taken from the layer itself. `land` is the lattice (its size and
/// the land as laid, where a fluid lies under it where it is absent).
/// Vertices are rounded to float32 once so the terrain alone survives the
/// STL reload.
fn lift_layers(
    laid: &[Laid],
    land: &Grid,
    frame: Frame,
    printer: &crate::Printer,
) -> Result<TriMesh, KitError> {
    let ordered: Vec<&Laid> = laid
        .iter()
        .filter(|l| !l.fluid)
        .chain(laid.iter().filter(|l| l.fluid))
        .collect();
    // where a fluid is absent its top lies this far under the last land
    // laid: only where the stack mesher puts the layer's boundary on an
    // edge it crosses (what shows is the layer's `present`, not the gap)
    #[allow(clippy::cast_possible_truncation)]
    let under = (ABSENT_UNDER_MM * frame.scale()) as f32;
    let final_land: &[f32] = ordered
        .iter()
        .rfind(|l| !l.fluid)
        .map_or(&[], |l| l.top.as_slice());
    let tops: Vec<std::borrow::Cow<'_, [f32]>> = ordered
        .iter()
        .map(|l| {
            if l.fluid {
                std::borrow::Cow::Owned(
                    (0..land.z.len())
                        .map(|k| {
                            if l.mask.bits[k] {
                                l.top[k]
                            } else {
                                final_land.get(k).copied().unwrap_or(0.0) - under
                            }
                        })
                        .collect(),
                )
            } else {
                std::borrow::Cow::Borrowed(l.top.as_slice())
            }
        })
        .collect();
    type Shape = Box<dyn Fn([f64; 3]) -> f64 + Sync>;
    let shapes: Vec<Option<(f64, Shape)>> = ordered
        .iter()
        .map(|l| {
            l.side.map(|s| {
                let seed = s.seed;
                let f: Shape = Box::new(move |p| crumble(seed, p));
                (s.depth_mm, f)
            })
        })
        .collect();
    let layers: Vec<dt_geom::Layer<'_>> = ordered
        .iter()
        .zip(&tops)
        .zip(&shapes)
        .map(|((l, top), shape)| dt_geom::Layer {
            top,
            id: l.slot.id,
            present: Some(&l.mask.bits),
            relief: shape.as_ref().map(|(depth, f)| dt_geom::LayerRelief {
                depth: *depth,
                shape: f.as_ref(),
            }),
        })
        .collect();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let sides = dt_geom::Sides {
        column_nodes: (libm::round(SIDE_WAVE_MM / land.step) as usize).max(1),
        row_mm: 1.6 * printer.layer_mm,
        fade_mm: 2.0 * printer.layer_mm,
        overhang_deg: printer.overhang_deg,
        corner_round_mm: frame.corner_round_mm(),
        corner_blend_mm: dt_geom::CORNER_BLEND_MM,
    };
    dt_geom::lift_stack(
        &dt_geom::Lattice {
            nx: land.nx,
            ny: land.ny,
            step: land.step,
            tol: LIFT_TOL_MM,
            max_cells: LIFT_MAX_CELLS,
            round: dt_geom::Round::F32,
        },
        &layers,
        &sides,
    )
    .map_err(|e| KitError::Lift(e.to_string()))
}

/// The shape of the relief on a tile's side: 0 on the plane to 1 at its
/// depth, grain like the top's, a function of the point alone so it runs
/// on round the corners. A good share of the face stays near the plane.
fn crumble(seed: Seed, p: [f64; 3]) -> f64 {
    let s = seed.key("side").value();
    let n = dt_geom::noise::fbm3(s, SIDE_LUMP_MM, 3, p);
    crate::smooth01((n + 0.45) / 0.9)
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
    use super::Relief;
    use super::{Frame, LayerSpec, Material, Scene, SideSpec};
    use crate::{Band, Field, OnFrame, Seed};

    /// A flat layer at `top` over the whole tile, laid with a round of
    /// `round` mm, and the surface it makes.
    fn flat(top: f64, round: f64, skin: f64) -> (Frame, Field) {
        let frame = Frame::dungeonblocks(1, 1);
        let (nx, ny) = frame.nodes();
        let band = Band::flat(nx, ny, frame.step_mm(), top, top);
        let shape = band.fit(&Field::new(&frame, top));
        let mask = Scene::new(frame, Seed::new(1)).whole();
        let mut scene = Scene::new(frame, Seed::new(1));
        scene
            .lay(&LayerSpec {
                owner: "dirt",
                material: Material::Earth,
                fluid: false,
                mask: &mask,
                shape: &shape,
                grain: None,
                skin_mm: skin,
                round_mm: round,
                side: None::<SideSpec>,
            })
            .unwrap();
        let g = scene.surface_field().unwrap();
        (
            frame,
            Field::new(&frame, 0.0).with_z(g.z.iter().map(|&z| f64::from(z)).collect()),
        )
    }

    #[test]
    fn the_edge_round_is_cut_from_the_sharp_corner() {
        let (frame, f) = flat(3.4, 0.2, 0.6);
        let (nx, ny) = frame.nodes();
        for j in 0..ny {
            for i in 0..nx {
                let z = f.z[j * nx + i];
                let on_rim = i == 0 || j == 0 || i == nx - 1 || j == ny - 1;
                // the rim nodes carry the drop (mitred at a corner: the same),
                // and nothing else is touched
                let want = if on_rim { 3.2 } else { 3.4 };
                assert!((z - want).abs() < 1e-6, "({i}, {j}) is {z}");
            }
        }
    }

    #[test]
    fn the_edge_round_tapers_to_none_on_dirt_thinner_than_the_skin() {
        let (_, thin) = flat(0.5, 0.2, 0.6);
        assert!(thin.z.iter().all(|&z| (z - 0.5).abs() < 1e-6));
        let (_, thick) = flat(1.2, 0.2, 0.6);
        assert!((thick.z[0] - 1.0).abs() < 1e-6);
    }

    /// The dirt of a flat 3.4 mm shape with `relief` as its grain (weight
    /// `weight` at every node), as the scene lays it.
    fn grained(
        shape_z: impl Fn(f64, f64) -> f64,
        relief: impl Fn(f64, f64) -> f64,
        weight: f64,
        round: f64,
    ) -> (Scene, LayerSpecParts) {
        let frame = Frame::dungeonblocks(1, 1);
        let (nx, ny) = frame.nodes();
        let at = |f: &dyn Fn(f64, f64) -> f64| -> Field {
            Field::new(&frame, 0.0).with_z(
                (0..nx * ny)
                    .map(|k| {
                        f(
                            (k % nx) as f64 * frame.step_mm(),
                            (k / nx) as f64 * frame.step_mm(),
                        )
                    })
                    .collect(),
            )
        };
        let hi = at(&shape_z);
        let band = Band::new(hi.clone(), hi.clone()).unwrap();
        let shape = band.fit(&hi);
        let parts = LayerSpecParts {
            shape,
            relief: at(&relief),
            weight: at(&|_, _| weight),
        };
        let mut scene = Scene::new(frame, Seed::new(1));
        let mask = scene.whole();
        {
            let grain = Relief {
                relief: &parts.relief,
                weight: &parts.weight,
                floor: None,
            };
            scene
                .lay(&LayerSpec {
                    owner: "dirt",
                    material: Material::Earth,
                    fluid: false,
                    mask: &mask,
                    shape: &parts.shape,
                    grain: Some(&grain),
                    skin_mm: 0.6,
                    round_mm: round,
                    side: None,
                })
                .unwrap();
        }
        (scene, parts)
    }

    struct LayerSpecParts {
        shape: crate::Banded,
        relief: Field,
        weight: Field,
    }

    /// Grain never stands above the shape the band holds, at any node, the
    /// rim and the corners included, whatever the relief does there.
    #[test]
    fn laid_grain_never_rises_above_the_shape() {
        let wave = |x: f64, y: f64| 0.6 * (x * 0.9).sin() * (y * 1.3).cos() + 0.3 * (x * 4.0).sin();
        let shape_z = |x: f64, y: f64| 3.4 + 0.5 * (x * 0.2).sin() + 0.1 * y;
        for round in [0.0, 0.2] {
            let (scene, parts) = grained(shape_z, wave, 1.0, round);
            let top = scene.surface_field().unwrap();
            let shape = parts.shape.field();
            for k in 0..top.z.len() {
                assert!(
                    f64::from(top.z[k]) <= shape.z[k] + 1e-5,
                    "node {k}: top {} over shape {}",
                    top.z[k],
                    shape.z[k]
                );
            }
        }
    }

    /// The edge height is a ceiling: with grain that would stand over it
    /// everywhere, nothing stands over 3.4 mm, and the grain's high points
    /// reach it (the surface is set in just as far as the grain beside it
    /// stands, rim included).
    #[test]
    fn the_edge_height_is_a_ceiling_the_grain_reaches() {
        let bumps = |x: f64, y: f64| 0.5 * (0.5 + 0.5 * (x * 3.0).sin() * (y * 2.5).cos());
        let (scene, _) = grained(|_, _| 3.4, bumps, 1.0, 0.0);
        let top = scene.surface_field().unwrap();
        let max = top.z.iter().fold(f32::MIN, |a, &b| a.max(b));
        assert!(max <= 3.4 + 1e-5, "{max}");
        assert!(max >= 3.4 - 1e-3, "{max}");
        let (nx, ny) = Frame::dungeonblocks(1, 1).nodes();
        for k in (0..nx * ny)
            .filter(|k| k % nx == 0 || k % nx == nx - 1 || *k < nx || *k >= nx * (ny - 1))
        {
            assert!(top.z[k] <= 3.4 + 1e-5, "rim node {k}");
        }
    }

    /// The set-in is local: far from a stand of grain the surface is not
    /// moved, near it it is moved by about the grain's height.
    #[test]
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn the_set_in_is_local_to_the_grain() {
        let spike = |x: f64, y: f64| {
            if (x - 5.0).hypot(y - 5.0) < 1.5 {
                0.4
            } else {
                0.0
            }
        };
        let frame = Frame::dungeonblocks(1, 1);
        let (nx, ny) = frame.nodes();
        // asked before the layer is laid, as a bedded stone asks
        let (_, parts) = grained(|_, _| 3.4, spike, 1.0, 0.0);
        let scene = Scene::new(frame, Seed::new(1));
        let mask = scene.whole();
        let grain = Relief {
            relief: &parts.relief,
            weight: &parts.weight,
            floor: None,
        };
        let off = scene.grain_offset(&LayerSpec {
            owner: "dirt",
            material: Material::Earth,
            fluid: false,
            mask: &mask,
            shape: &parts.shape,
            grain: Some(&grain),
            skin_mm: 0.6,
            round_mm: 0.0,
            side: None,
        });
        let node = |x: f64, y: f64| {
            let s = frame.step_mm();
            (libm::round(y / s) as usize) * nx + libm::round(x / s) as usize
        };
        assert!(off.z[node(5.0, 5.0)] > 0.3, "{}", off.z[node(5.0, 5.0)]);
        assert!(
            off.z[node(30.0, 30.0)] < 1e-3,
            "{}",
            off.z[node(30.0, 30.0)]
        );
        assert_eq!(off.z.len(), nx * ny);
    }

    #[test]
    #[cfg(target_pointer_width = "64")]
    fn nodes_do_not_wrap() {
        assert_eq!(Frame::dungeonblocks(1, 1).nodes(), (129, 129));
        // 2^25 × 128 = 2^32: wrapped to 1 node in u32
        let (nx, _) = Frame::dungeonblocks(1 << 25, 1).nodes();
        assert_eq!(nx as u64, (1u64 << 32) + 1);
    }
}
