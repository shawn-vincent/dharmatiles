//! Reader for the leaf placer's parity captures
//! (`tests/parity/capture/leaves.py`): `b"DTLF"`, u32 version, then
//! sections `u32 name_len, name, u32 kind (0 = f64, 1 = u32, 2 = f32),
//! u64 count, data`, little-endian.
#![allow(dead_code, reason = "shared by the test and the example")]

use std::collections::HashMap;

use dt_kit::TriMesh;
use dt_looks::tree::leaves::{Draws, LeafShape, Samples};

pub enum Section {
    F64(Vec<f64>),
    U32(Vec<u32>),
}

pub struct Capture {
    pub sections: HashMap<String, Section>,
}

#[allow(clippy::cast_possible_truncation, reason = "section sizes")]
pub fn read(bytes: &[u8]) -> Result<Capture, String> {
    if bytes.get(..4) != Some(b"DTLF".as_slice()) {
        return Err("not a DTLF capture".into());
    }
    let mut i = 8;
    let take = |i: &mut usize, n: usize| -> Result<&[u8], String> {
        let s = bytes.get(*i..*i + n).ok_or("truncated capture")?;
        *i += n;
        Ok(s)
    };
    let u32_at = |s: &[u8]| u32::from_le_bytes([s[0], s[1], s[2], s[3]]);
    let mut sections = HashMap::new();
    while i < bytes.len() {
        let nl = u32_at(take(&mut i, 4)?) as usize;
        let name = String::from_utf8(take(&mut i, nl)?.to_vec()).map_err(|e| e.to_string())?;
        let kind = u32_at(take(&mut i, 4)?);
        let n = u64::from_le_bytes(take(&mut i, 8)?.try_into().map_err(|_| "count")?) as usize;
        let sec = match kind {
            0 => Section::F64(
                take(&mut i, 8 * n)?
                    .as_chunks::<8>()
                    .0
                    .iter()
                    .map(|c| f64::from_le_bytes(*c))
                    .collect(),
            ),
            1 => Section::U32(
                take(&mut i, 4 * n)?
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|c| u32::from_le_bytes(*c))
                    .collect(),
            ),
            2 => Section::F64(
                take(&mut i, 4 * n)?
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|c| f64::from(f32::from_le_bytes(*c)))
                    .collect(),
            ),
            k => return Err(format!("unknown section kind {k}")),
        };
        sections.insert(name, sec);
    }
    Ok(Capture { sections })
}

impl Capture {
    pub fn f64s(&self, name: &str) -> Vec<f64> {
        match self.sections.get(name) {
            Some(Section::F64(v)) => v.clone(),
            _ => Vec::new(),
        }
    }

    pub fn u32s(&self, name: &str) -> Vec<u32> {
        match self.sections.get(name) {
            Some(Section::U32(v)) => v.clone(),
            _ => Vec::new(),
        }
    }

    pub fn points(&self, name: &str) -> Vec<[f64; 3]> {
        self.f64s(name).as_chunks::<3>().0.to_vec()
    }

    pub fn mesh(&self, name: &str) -> TriMesh {
        TriMesh {
            positions: self.points(&format!("{name}/v")),
            triangles: self.u32s(&format!("{name}/f")).as_chunks::<3>().0.to_vec(),
            ..TriMesh::default()
        }
    }

    pub fn seed0(&self) -> u64 {
        let s = self.u32s("seed0");
        u64::from(s[0]) | (u64::from(s[1]) << 32)
    }

    pub fn shape(&self) -> LeafShape {
        let s = self.f64s("shape");
        LeafShape {
            length_mm: s[0],
            width_mm: s[1],
            thickness_mm: s[2],
            fold_angle_deg: s[3],
            inner_curve: s[4],
            outer_curve: s[5],
            curl_deg: s[6],
            overhang_deg: None,
        }
    }

    /// Python's draws: its samples and its `_hash01` seed.
    pub fn draws(&self) -> Draws {
        Draws::Python {
            seed0: self.seed0(),
            candidates: Samples {
                points: self.points("cand/p"),
                normals: self.points("cand/n"),
            },
            tests: Samples {
                points: self.points("test/p"),
                normals: self.points("test/n"),
            },
        }
    }

    /// Python's counts: placed, roots, failed builds, branch-culled,
    /// uncovered test points.
    pub fn py_counts(&self) -> [u32; 5] {
        let c = self.u32s("py/counts");
        [c[0], c[1], c[2], c[3], c[4]]
    }

    /// Python's placed leaf solids (vertex blocks per leaf).
    pub fn py_leaf_positions(&self) -> Vec<Vec<[f64; 3]>> {
        let v = self.points("py/leaf_v");
        let mut out = Vec::new();
        let mut o = 0;
        for n in self.u32s("py/leaf_nv") {
            out.push(v[o..o + n as usize].to_vec());
            o += n as usize;
        }
        out
    }
}
