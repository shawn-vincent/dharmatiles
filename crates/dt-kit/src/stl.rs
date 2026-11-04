//! The STL writer with the build record (feasibility page: "write a build
//! record — document hash, seed, engine version — into each STL").

use dt_geom::TriMesh;

/// Where a build came from. The front end writes it beside the STL in
/// full; the STL header carries a one-line summary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildRecord {
    /// Engine version, as `build_info.py` formats it (`v1.0.0-12-gabc1234`,
    /// `gabc1234-dirty`, …).
    pub engine: String,
    /// The document's path, such as `lib:tiles/test/1x1-flat.kdl`.
    pub document: String,
    /// 64-bit FNV-1a of the document's text.
    pub document_hash: u64,
    /// The tile's root seed.
    pub seed: u64,
    /// The connector system, `db` or `ol`.
    pub system: String,
}

impl BuildRecord {
    /// The 80-byte STL header: never starts with `solid` (that marks ASCII
    /// STL), padded with spaces.
    #[must_use]
    pub fn header(&self) -> [u8; 80] {
        let line = format!(
            "dharmatiles {} {} seed={} doc={:016x} {}",
            self.engine, self.system, self.seed, self.document_hash, self.document
        );
        let mut h = [b' '; 80];
        let n = line.len().min(80);
        h[..n].copy_from_slice(&line.as_bytes()[..n]);
        h
    }
}

/// **write_stl**: binary STL bytes of `mesh` (float32 positions, normals
/// from those positions).
#[must_use]
pub fn stl(mesh: &TriMesh, record: &BuildRecord) -> Vec<u8> {
    let mut out = Vec::with_capacity(84 + 50 * mesh.triangles.len());
    out.extend_from_slice(&record.header());
    out.extend_from_slice(
        &u32::try_from(mesh.triangles.len())
            .unwrap_or(u32::MAX)
            .to_le_bytes(),
    );
    for t in &mesh.triangles {
        #[allow(clippy::cast_possible_truncation)]
        let v = t.map(|i| mesh.positions[i as usize].map(|x| x as f32));
        let (a, b) = (
            [v[1][0] - v[0][0], v[1][1] - v[0][1], v[1][2] - v[0][2]],
            [v[2][0] - v[0][0], v[2][1] - v[0][1], v[2][2] - v[0][2]],
        );
        let n = [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ];
        let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        let n = if l > 0.0 { n.map(|x| x / l) } else { [0.0; 3] };
        for x in n.iter().chain(v.iter().flatten()) {
            out.extend_from_slice(&x.to_le_bytes());
        }
        out.extend_from_slice(&[0, 0]);
    }
    out
}
