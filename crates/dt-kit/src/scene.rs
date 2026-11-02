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
}

impl Material {
    /// From the material table's number.
    #[must_use]
    pub const fn from_id(id: u32) -> Option<Self> {
        match id {
            1 => Some(Self::Earth),
            2 => Some(Self::Connector),
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

/// Slots per build. A leaf's face IDs come from its slot's range, so this
/// is also `dt_geom::source::MAX_SLOTS`.
const SLOTS: u32 = dt_geom::source::MAX_SLOTS;

/// The scene of one build.
pub struct Scene {
    frame: Frame,
    seed: Seed,
    field: Grid,
    painted: Mask,
    stone: Mask,
    edge_mm: Option<f64>,
    notes: Vec<(String, f64)>,
    parts: Vec<Part>,
    block: IdBlock,
    next_slot: u32,
    materials: Materials,
    stages: Vec<Stage>,
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
            stone: Mask::new(nx, ny, false),
            edge_mm: None,
            notes: Vec::new(),
            parts: Vec::new(),
            block: IdBlock::reserve::<dt_geom::Solid>(SLOTS),
            next_slot: 0,
            materials: Materials::default(),
            stages: Vec::new(),
        }
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
        self.spread_paint();
        if let Some(edge) = self.edge_mm {
            self.hold_edge(edge);
        }
        self.record("surface", self.field.content_hash());
        let mesh = dt_geom::lift_adaptive(&self.field, LIFT_TOL_MM, LIFT_MAX_CELLS)
            .map_err(|e| KitError::Surface(e.to_string()))?;
        let slot = self.slots(1, Material::Earth)?.get(0);
        let solid = leaf(slot, &mesh)?;
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
    /// edge level, and nothing below it (rises only).
    fn hold_edge(&mut self, edge: f64) {
        #[allow(clippy::cast_possible_truncation)]
        let e = edge as f32;
        let (nx, ny) = (self.field.nx, self.field.ny);
        for j in 0..ny {
            for i in 0..nx {
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

    /// Add a solid a look built, and record its stage hash.
    pub fn add(&mut self, tag: Tag, solid: Solid) {
        let hash = solid.0.to_tri_mesh().content_hash();
        self.record(&format!("{}:{}", tag.kind, tag.owner), hash);
        self.parts.push(Part { tag, solid });
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
