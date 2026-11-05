//! The mesh backend behind [`crate::Mesh`]: a feature.

#[cfg(feature = "manifold-ffi")]
pub mod ffi;

use crate::TriMesh;

/// Interleave positions and extra properties into `MeshGL` vertex rows.
pub(crate) fn interleave(m: &TriMesh) -> (Vec<f64>, usize) {
    let np = 3 + m.num_extra;
    let mut props = Vec::with_capacity(m.positions.len() * np);
    for (i, p) in m.positions.iter().enumerate() {
        props.extend_from_slice(p);
        if m.num_extra > 0 {
            props.extend_from_slice(&m.extra[i * m.num_extra..(i + 1) * m.num_extra]);
        }
    }
    (props, np)
}

/// Split `MeshGL` vertex rows back into positions and extra properties.
pub(crate) fn deinterleave(props: &[f64], np: usize) -> (Vec<[f64; 3]>, Vec<f64>) {
    let np = np.max(3);
    let n = props.len() / np;
    let mut pos = Vec::with_capacity(n);
    let mut extra = Vec::with_capacity(n * (np - 3));
    for row in props.chunks_exact(np) {
        pos.push([row[0], row[1], row[2]]);
        extra.extend_from_slice(&row[3..]);
    }
    (pos, extra)
}

/// Narrow an index buffer that the kernel already limits to 32 bits.
pub(crate) fn narrow(v: impl IntoIterator<Item = u64>) -> Vec<u32> {
    v.into_iter()
        .map(|x| u32::try_from(x).unwrap_or(u32::MAX))
        .collect()
}
