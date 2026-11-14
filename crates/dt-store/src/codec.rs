//! A finished [`Built`] as bytes and back, exactly: every float keeps its
//! bits, so a build read from the cache writes the same STL, hashes the same
//! and paints the same as the build that made it.
//!
//! Little-endian throughout; a string or a list is its length (u64) and
//! then its items.  The reader checks every length against the bytes that
//! are left, so a damaged file is an error in words, never a huge
//! allocation or a panic.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Mutex;

use dt_kit::Look;
use dt_kit::debug::{Column, Debug, Dot, Sheet, Trail};
use dt_kit::{
    Audit, BuildRecord, Dropped, EntryId, Export, Grid, Kind, Mask, Material, Stage, TriMesh,
};

use dt_eval::doc::Named;
use dt_eval::{Built, CacheUse, Product, Report};

/// Every material, to read one back by its number.
const MATERIALS: [Material; 9] = [
    Material::Earth,
    Material::Connector,
    Material::Rock,
    Material::Water,
    Material::Grass,
    Material::Flower,
    Material::Masonry,
    Material::Wood,
    Material::Leaf,
];

pub(super) struct Wr(pub(super) Vec<u8>);

impl Wr {
    pub(super) fn bool(&mut self, v: bool) {
        self.0.push(u8::from(v));
    }
    pub(super) fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    pub(super) fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    pub(super) fn u64(&mut self, v: u64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    pub(super) fn usize(&mut self, v: usize) {
        self.u64(v as u64);
    }
    pub(super) fn f32(&mut self, v: f32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    pub(super) fn f64(&mut self, v: f64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    pub(super) fn str(&mut self, s: &str) {
        self.usize(s.len());
        self.0.extend_from_slice(s.as_bytes());
    }
    fn opt(&mut self, some: bool) {
        self.bool(some);
    }
    fn f64s(&mut self, v: &[f64]) {
        self.usize(v.len());
        self.0.reserve(v.len() * 8);
        for x in v {
            self.f64(*x);
        }
    }
    fn f32s(&mut self, v: &[f32]) {
        self.usize(v.len());
        self.0.reserve(v.len() * 4);
        for x in v {
            self.f32(*x);
        }
    }
    fn u32s(&mut self, v: &[u32]) {
        self.usize(v.len());
        self.0.reserve(v.len() * 4);
        for x in v {
            self.u32(*x);
        }
    }
    fn u16s(&mut self, v: &[u16]) {
        self.usize(v.len());
        self.0.reserve(v.len() * 2);
        for x in v {
            self.0.extend_from_slice(&x.to_le_bytes());
        }
    }
    /// Flags, one byte each.
    fn bools(&mut self, v: &[bool]) {
        self.usize(v.len());
        self.0.extend(v.iter().map(|&b| u8::from(b)));
    }
    fn vec3s(&mut self, v: &[[f64; 3]]) {
        self.usize(v.len());
        self.0.reserve(v.len() * 24);
        for p in v {
            for x in p {
                self.f64(*x);
            }
        }
    }
    fn tris(&mut self, v: &[[u32; 3]]) {
        self.usize(v.len());
        self.0.reserve(v.len() * 12);
        for t in v {
            for x in t {
                self.u32(*x);
            }
        }
    }
}

pub(super) struct Rd<'a> {
    b: &'a [u8],
    p: usize,
}

type R<T> = Result<T, String>;

impl<'a> Rd<'a> {
    pub(super) fn new(b: &'a [u8]) -> Self {
        Self { b, p: 0 }
    }
    fn take(&mut self, n: usize) -> R<&'a [u8]> {
        let end = self
            .p
            .checked_add(n)
            .filter(|&e| e <= self.b.len())
            .ok_or_else(|| format!("ends early (wanted {n} bytes at {})", self.p))?;
        let s = &self.b[self.p..end];
        self.p = end;
        Ok(s)
    }
    fn arr<const N: usize>(&mut self) -> R<[u8; N]> {
        let mut a = [0u8; N];
        a.copy_from_slice(self.take(N)?);
        Ok(a)
    }
    pub(super) fn u8(&mut self) -> R<u8> {
        Ok(self.take(1)?[0])
    }
    pub(super) fn bool(&mut self) -> R<bool> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            n => Err(format!("a flag is {n}")),
        }
    }
    pub(super) fn u32(&mut self) -> R<u32> {
        Ok(u32::from_le_bytes(self.arr()?))
    }
    pub(super) fn u64(&mut self) -> R<u64> {
        Ok(u64::from_le_bytes(self.arr()?))
    }
    pub(super) fn f32(&mut self) -> R<f32> {
        Ok(f32::from_le_bytes(self.arr()?))
    }
    pub(super) fn f64(&mut self) -> R<f64> {
        Ok(f64::from_le_bytes(self.arr()?))
    }
    /// A length, checked against what is left at `item` bytes each.
    fn len(&mut self, item: usize) -> R<usize> {
        let n = usize::try_from(self.u64()?).map_err(|_| "a length too large".to_owned())?;
        let need = n.checked_mul(item.max(1)).ok_or("a length too large")?;
        if need > self.b.len() - self.p {
            return Err(format!("a list of {n} runs past the end"));
        }
        Ok(n)
    }
    pub(super) fn usize(&mut self) -> R<usize> {
        usize::try_from(self.u64()?).map_err(|_| "a number too large".to_owned())
    }
    pub(super) fn str(&mut self) -> R<String> {
        let n = self.len(1)?;
        String::from_utf8(self.take(n)?.to_vec()).map_err(|_| "text that is not UTF-8".to_owned())
    }
    fn f64s(&mut self) -> R<Vec<f64>> {
        let n = self.len(8)?;
        (0..n).map(|_| self.f64()).collect()
    }
    fn f32s(&mut self) -> R<Vec<f32>> {
        let n = self.len(4)?;
        (0..n).map(|_| self.f32()).collect()
    }
    fn u32s(&mut self) -> R<Vec<u32>> {
        let n = self.len(4)?;
        (0..n).map(|_| self.u32()).collect()
    }
    fn u16s(&mut self) -> R<Vec<u16>> {
        let n = self.len(2)?;
        (0..n)
            .map(|_| Ok(u16::from_le_bytes(self.arr()?)))
            .collect()
    }
    fn bools(&mut self) -> R<Vec<bool>> {
        let n = self.len(1)?;
        self.take(n)?
            .iter()
            .map(|&b| match b {
                0 => Ok(false),
                1 => Ok(true),
                n => Err(format!("a flag is {n}")),
            })
            .collect()
    }
    fn vec3s(&mut self) -> R<Vec<[f64; 3]>> {
        let n = self.len(24)?;
        (0..n)
            .map(|_| Ok([self.f64()?, self.f64()?, self.f64()?]))
            .collect()
    }
    fn tris(&mut self) -> R<Vec<[u32; 3]>> {
        let n = self.len(12)?;
        (0..n)
            .map(|_| Ok([self.u32()?, self.u32()?, self.u32()?]))
            .collect()
    }
    pub(super) fn done(&self) -> R<()> {
        if self.p == self.b.len() {
            Ok(())
        } else {
            Err(format!("{} bytes left over", self.b.len() - self.p))
        }
    }
}

/// A debug mark's class, which a build holds as a `&'static str`: read
/// back, the few distinct words are kept for the run (each leaked once).
fn intern(s: &str) -> &'static str {
    static POOL: Mutex<BTreeSet<&'static str>> = Mutex::new(BTreeSet::new());
    let mut pool = POOL
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(k) = pool.get(s) {
        return k;
    }
    let k: &'static str = Box::leak(s.to_owned().into_boxed_str());
    pool.insert(k);
    k
}

fn mesh_out(w: &mut Wr, m: &TriMesh) {
    w.vec3s(&m.positions);
    w.tris(&m.triangles);
    w.f64s(&m.extra);
    w.usize(m.num_extra);
    w.u32s(&m.merge_from);
    w.u32s(&m.merge_to);
    w.u32s(&m.run_index);
    w.u32s(&m.run_original_id);
    w.u32s(&m.face_id);
    w.f64(m.tolerance);
}

fn mesh_in(r: &mut Rd) -> R<TriMesh> {
    Ok(TriMesh {
        positions: r.vec3s()?,
        triangles: r.tris()?,
        extra: r.f64s()?,
        num_extra: r.usize()?,
        merge_from: r.u32s()?,
        merge_to: r.u32s()?,
        run_index: r.u32s()?,
        run_original_id: r.u32s()?,
        face_id: r.u32s()?,
        tolerance: r.f64()?,
    })
}

fn dropped_out(w: &mut Wr, v: &[Dropped]) {
    w.usize(v.len());
    for d in v {
        w.usize(d.triangles);
        w.f64(d.volume_mm3);
        for corner in d.bounds {
            for x in corner {
                w.f64(x);
            }
        }
    }
}

fn dropped_in(r: &mut Rd) -> R<Vec<Dropped>> {
    let n = r.len(8 * 8)?;
    (0..n)
        .map(|_| {
            Ok(Dropped {
                triangles: r.usize()?,
                volume_mm3: r.f64()?,
                bounds: [
                    [r.f64()?, r.f64()?, r.f64()?],
                    [r.f64()?, r.f64()?, r.f64()?],
                ],
            })
        })
        .collect()
}

fn audit_out(w: &mut Wr, a: &Audit) {
    w.usize(a.triangles);
    w.usize(a.vertices);
    w.f64(a.volume_mm3);
    w.usize(a.bodies);
    dropped_out(w, &a.dropped);
    dropped_out(w, &a.voids);
    dropped_out(w, &a.slivers);
    w.usize(a.pinches_moved);
    w.usize(a.collapsed);
    w.bool(a.watertight);
    w.f64(a.overhang_mm2);
    w.bools(&a.overhang);
    w.f64(a.overhang_budget_mm2);
    w.f64(a.band_mm2);
    w.f64(a.band_budget_mm2);
    w.bools(&a.band);
    w.usize(a.holes);
}

fn audit_in(r: &mut Rd) -> R<Audit> {
    Ok(Audit {
        triangles: r.usize()?,
        vertices: r.usize()?,
        volume_mm3: r.f64()?,
        bodies: r.usize()?,
        dropped: dropped_in(r)?,
        voids: dropped_in(r)?,
        slivers: dropped_in(r)?,
        pinches_moved: r.usize()?,
        collapsed: r.usize()?,
        watertight: r.bool()?,
        overhang_mm2: r.f64()?,
        overhang: r.bools()?,
        overhang_budget_mm2: r.f64()?,
        band_mm2: r.f64()?,
        band_budget_mm2: r.f64()?,
        band: r.bools()?,
        holes: r.usize()?,
    })
}

fn mask_out(w: &mut Wr, m: &Mask) {
    w.usize(m.nx);
    w.usize(m.ny);
    w.bools(&m.bits);
}

fn mask_in(r: &mut Rd) -> R<Mask> {
    Ok(Mask {
        nx: r.usize()?,
        ny: r.usize()?,
        bits: r.bools()?,
    })
}

fn field_out(w: &mut Wr, g: &Grid<f64>) {
    w.usize(g.nx);
    w.usize(g.ny);
    w.f64(g.step);
    w.f64s(&g.z);
}

fn field_in(r: &mut Rd) -> R<Grid<f64>> {
    Ok(Grid {
        nx: r.usize()?,
        ny: r.usize()?,
        step: r.f64()?,
        z: r.f64s()?,
    })
}

fn debug_out(w: &mut Wr, d: &Debug) {
    w.usize(d.dots.len());
    for p in &d.dots {
        w.str(p.owner.as_str());
        w.str(p.class);
        p.at.iter().for_each(|&x| w.f64(x));
    }
    w.usize(d.trails.len());
    for t in &d.trails {
        w.str(t.owner.as_str());
        w.str(t.class);
        w.vec3s(&t.points);
    }
    w.usize(d.sheets.len());
    for s in &d.sheets {
        w.str(s.owner.as_str());
        w.str(s.class);
        field_out(w, &s.z);
        mask_out(w, &s.region);
        w.f32s(&s.shade);
    }
    w.usize(d.columns.len());
    for c in &d.columns {
        w.str(c.owner.as_str());
        w.str(c.class);
        c.at.iter().for_each(|&x| w.f64(x));
        c.span.iter().for_each(|&x| w.f64(x));
        w.f64(c.rolled);
        c.shade.iter().for_each(|&x| w.f32(x));
    }
}

fn debug_in(r: &mut Rd) -> R<Debug> {
    let mut d = Debug::default();
    for _ in 0..r.len(8)? {
        d.dots.push(Dot {
            owner: EntryId::new(&r.str()?),
            class: intern(&r.str()?),
            at: [r.f64()?, r.f64()?, r.f64()?],
        });
    }
    for _ in 0..r.len(8)? {
        d.trails.push(Trail {
            owner: EntryId::new(&r.str()?),
            class: intern(&r.str()?),
            points: r.vec3s()?,
        });
    }
    for _ in 0..r.len(8)? {
        d.sheets.push(Sheet {
            owner: EntryId::new(&r.str()?),
            class: intern(&r.str()?),
            z: field_in(r)?,
            region: mask_in(r)?,
            shade: r.f32s()?,
        });
    }
    for _ in 0..r.len(8)? {
        d.columns.push(Column {
            owner: EntryId::new(&r.str()?),
            class: intern(&r.str()?),
            at: [r.f64()?, r.f64()?],
            span: [r.f64()?, r.f64()?],
            rolled: r.f64()?,
            shade: [r.f32()?, r.f32()?, r.f32()?],
        });
    }
    Ok(d)
}

/// What the cache keeps of a build, as bytes.  `reused` and the cache's own
/// note belong to the run that made it and are not kept.
pub(super) fn built_out(b: &Built) -> Vec<u8> {
    let mut w = Wr(Vec::with_capacity(
        b.export.mesh.positions.len() * 24 + b.export.mesh.triangles.len() * 40 + 4096,
    ));
    mesh_out(&mut w, &b.export.mesh);
    w.u32s(&b.export.source);
    w.usize(b.materials.len());
    w.0.extend(b.materials.iter().map(|&m| m as u8));
    audit_out(&mut w, &b.audit);
    w.usize(b.report.stages.len());
    for s in &b.report.stages {
        w.str(&s.name);
        w.u64(s.hash);
    }
    w.str(&b.record.engine);
    w.str(&b.record.document);
    w.u64(b.record.document_hash);
    w.u64(b.record.seed);
    w.str(&b.record.system);
    w.str(&b.record.printer);
    w.str(&b.out.to_string_lossy());
    w.f64(b.footprint[0]);
    w.f64(b.footprint[1]);
    w.opt(b.report.edge_mm.is_some());
    w.f64(b.report.edge_mm.unwrap_or(0.0));
    w.usize(b.report.notes.len());
    for (k, v) in &b.report.notes {
        w.str(k);
        w.f64(*v);
    }
    w.usize(b.report.left_out.len());
    for (k, n) in &b.report.left_out {
        w.str(k);
        w.usize(*n);
    }
    w.opt(b.report.surface.is_some());
    if let Some((g, m)) = &b.report.surface {
        w.usize(g.nx);
        w.usize(g.ny);
        w.f64(g.step);
        w.f32s(&g.z);
        mask_out(&mut w, m);
    }
    w.opt(b.report.covered.is_some());
    if let Some(m) = &b.report.covered {
        mask_out(&mut w, m);
    }
    w.usize(b.report.stage_secs.len());
    for (k, v) in &b.report.stage_secs {
        w.str(k);
        w.f64(*v);
    }
    w.bool(b.report.look.brick);
    w.u16s(&b.report.entries);
    w.u16s(&b.report.pieces);
    w.usize(b.report.stack.len());
    for n in &b.report.stack {
        w.str(n.id.as_str());
        w.str(&n.what);
        w.str(&n.element);
        w.str(&n.label);
    }
    w.opt(b.report.clouds.is_some());
    if let Some(d) = &b.report.clouds {
        debug_out(&mut w, d);
    }
    w.opt(b.report.debug.is_some());
    if let Some(d) = &b.report.debug {
        debug_out(&mut w, d);
    }
    w.usize(b.report.placed.len());
    for p in &b.report.placed {
        w.u8(Kind::ALL
            .iter()
            .position(|k| *k == p.kind)
            .and_then(|i| u8::try_from(i).ok())
            .unwrap_or(u8::MAX));
        w.f64(p.at[0]);
        w.f64(p.at[1]);
        w.f64(p.r);
    }
    w.bool(b.checked);
    w.0
}

/// A build from `bytes`, as [`built_out`] wrote it.
pub(super) fn built_in(bytes: &[u8]) -> R<Built> {
    let mut r = Rd::new(bytes);
    let mesh = mesh_in(&mut r)?;
    let source = r.u32s()?;
    let n = r.len(1)?;
    let materials = r
        .take(n)?
        .iter()
        .map(|&m| {
            MATERIALS
                .iter()
                .copied()
                .find(|k| *k as u8 == m)
                .ok_or_else(|| format!("no material numbered {m}"))
        })
        .collect::<R<Vec<_>>>()?;
    let audit = audit_in(&mut r)?;
    let stages = (0..r.len(8)?)
        .map(|_| {
            Ok(Stage {
                name: r.str()?,
                hash: r.u64()?,
            })
        })
        .collect::<R<Vec<_>>>()?;
    let record = BuildRecord {
        engine: r.str()?,
        document: r.str()?,
        document_hash: r.u64()?,
        seed: r.u64()?,
        system: r.str()?,
        printer: r.str()?,
    };
    let out = PathBuf::from(r.str()?);
    let footprint = [r.f64()?, r.f64()?];
    let has_edge = r.bool()?;
    let edge = r.f64()?;
    let notes = (0..r.len(16)?)
        .map(|_| Ok((r.str()?, r.f64()?)))
        .collect::<R<Vec<_>>>()?;
    let left_out = (0..r.len(16)?)
        .map(|_| Ok((r.str()?, r.usize()?)))
        .collect::<R<Vec<_>>>()?;
    let surface = if r.bool()? {
        let g = Grid {
            nx: r.usize()?,
            ny: r.usize()?,
            step: r.f64()?,
            z: r.f32s()?,
        };
        Some((g, mask_in(&mut r)?))
    } else {
        None
    };
    let covered = if r.bool()? {
        Some(mask_in(&mut r)?)
    } else {
        None
    };
    let stage_secs = (0..r.len(16)?)
        .map(|_| Ok((r.str()?, r.f64()?)))
        .collect::<R<Vec<_>>>()?;
    let brick = r.bool()?;
    let entries = r.u16s()?;
    let pieces = r.u16s()?;
    let stack = (0..r.len(32)?)
        .map(|_| {
            Ok(Named {
                id: EntryId::new(&r.str()?),
                what: r.str()?,
                element: r.str()?,
                label: r.str()?,
            })
        })
        .collect::<R<Vec<_>>>()?;
    let clouds = if r.bool()? {
        Some(debug_in(&mut r)?)
    } else {
        None
    };
    let debug = if r.bool()? {
        Some(debug_in(&mut r)?)
    } else {
        None
    };
    let placed = (0..r.len(25)?)
        .map(|_| {
            let k = r.u8()?;
            Ok(dt_kit::Placement {
                kind: Kind::ALL
                    .get(usize::from(k))
                    .copied()
                    .ok_or_else(|| format!("no kind numbered {k}"))?,
                at: [r.f64()?, r.f64()?],
                r: r.f64()?,
            })
        })
        .collect::<R<Vec<_>>>()?;
    let checked = r.bool()?;
    r.done()?;
    let tris = mesh.triangles.len();
    if source.len() != tris
        || materials.len() != tris
        || audit.overhang.len() != tris
        || audit.band.len() != tris
        || entries.len() != tris
        || pieces.len() != tris
    {
        return Err(format!(
            "its per-triangle lists do not match its {tris} triangles"
        ));
    }
    Ok(Built {
        product: Product {
            export: Export { mesh, source },
            materials,
            audit,
            record,
            out,
            footprint,
            checked,
        },
        report: Report {
            stages,
            edge_mm: has_edge.then_some(edge),
            notes,
            left_out,
            surface,
            covered,
            stage_secs,
            reused: 0,
            look: Look { brick },
            entries,
            pieces,
            stack,
            clouds,
            debug,
            placed,
            cache: CacheUse::default(),
        },
    })
}
