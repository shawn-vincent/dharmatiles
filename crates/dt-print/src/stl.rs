//! The binary STL writer.  The build record is `dt_kit::BuildRecord`; its
//! one-line summary is the STL header.

use dt_geom::TriMesh;
use dt_kit::BuildRecord;

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
