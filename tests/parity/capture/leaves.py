"""P5 leaves capture: the organic leaf placer's inputs and result, from outside.

Builds the Tree of ``src/tiles/ground/1x1-grass-tree.tile.py`` in memory
(skeleton → wood → clumps → leaves, the library code unchanged) at a given
tree seed, and records what ``place_leaves_organic`` was given and what it
returned, so the Rust placer (``crates/dt-looks/src/tree/leaves.rs``) can be
driven from the same inputs and compared:

* the foliage union the placer tiles (trimesh's manifold union of the clumps),
* the wood tubes (``avoid_meshes``, concatenated; one closed tube per body),
* the Python candidate and coverage-test samples (positions and smooth
  normals: numpy's PCG64 stream cannot be reproduced in Rust, so parity
  runs replay these),
* the Python result: per placed leaf its root and direction, every leaf
  solid, the counts the placer prints and its wall time.

With ``--rays`` it also records every Embree ray batch made during
placement (``rays.py``'s recorder) for the R3-style replay
``crates/dt-geom/examples/r3_rays.rs``.

Usage::

    python tests/parity/capture/leaves.py OUT_DIR SEED [--rays] [--clumps N] [--compact]

``--clumps N`` keeps only the first N clumps; ``--compact`` places without
the wood (no skewer cull), stores the samples as f32 and leaves out the leaf
solids (the small fixture committed under crates/dt-looks/tests/).  Writes
only into OUT_DIR (out/ is gitignored unless OUT_DIR says otherwise);
never writes an STL.

Format (little-endian), read by ``crates/dt-looks/examples/leaves_parity.rs``::

    leaves.bin  b"DTLF" u32 version=1, then sections, each
                u32 name_len, name, u32 kind (0=f64, 1=u32, 2=f32), u64 count, data
    meta.json   seed0, shape, counts, timing
"""

from __future__ import annotations

import io
import json
import pathlib
import re
import struct
import sys
import time
from contextlib import redirect_stdout

import numpy as np

REPO = pathlib.Path(__file__).resolve().parents[3]
sys.path.insert(0, str(REPO / "src"))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))

TILE = REPO / "src/tiles/ground/1x1-grass-tree.tile.py"


class Writer:
    def __init__(self, path: pathlib.Path):
        self.f = open(path, "wb")
        self.f.write(b"DTLF" + struct.pack("<I", 1))

    def f64(self, name: str, a) -> None:
        a = np.ascontiguousarray(a, "<f8").ravel()
        self._head(name, 0, len(a))
        self.f.write(a.tobytes())

    def f32(self, name: str, a) -> None:
        a = np.ascontiguousarray(a, "<f4").ravel()
        self._head(name, 2, len(a))
        self.f.write(a.tobytes())

    def u32(self, name: str, a) -> None:
        a = np.ascontiguousarray(a, "<u4").ravel()
        self._head(name, 1, len(a))
        self.f.write(a.tobytes())

    def _head(self, name: str, kind: int, n: int) -> None:
        nb = name.encode()
        self.f.write(struct.pack("<I", len(nb)) + nb + struct.pack("<IQ", kind, n))

    def close(self) -> None:
        self.f.close()


def main() -> None:
    out = pathlib.Path(sys.argv[1])
    seed = int(sys.argv[2])
    rays = "--rays" in sys.argv
    compact = "--compact" in sys.argv
    keep = None
    if "--clumps" in sys.argv:
        keep = int(sys.argv[sys.argv.index("--clumps") + 1])
    out.mkdir(parents=True, exist_ok=True)

    rec = None
    if rays:
        import rays as r3

        rec = r3.Recorder(out)

    from dharmatiles.spec import load_tile
    from dharmatiles.trees import placement_organic as po
    from dharmatiles.trees.layer import Tree
    from dharmatiles.trees.mesh import build_branch_mesh
    from dharmatiles.trees.skeleton import grow_skeleton

    tile = load_tile(TILE)[0]
    tree = next(
        layer for area in tile.areas for layer in area.layers if isinstance(layer, Tree)
    )

    captured: dict = {}
    orig = po.place_leaves_organic

    def wrapped(meshes, **kw):
        if keep is not None:
            meshes = list(meshes)[:keep]
            kw = dict(kw)
            kw["seeds"] = list(kw["seeds"])[:keep]
            kw["labels"] = list(kw["labels"])[:keep]
        if compact:
            # The committed fixture leaves the wood out (its tubes alone are
            # ~1 MB): the skewer cull is compared on the full captures.
            kw = dict(kw)
            kw["avoid_meshes"] = None
        if rec is not None:
            rec.install()
        buf = io.StringIO()
        t0 = time.perf_counter()
        with redirect_stdout(buf):
            parts, stats = orig(meshes, **kw)
        dt = time.perf_counter() - t0
        if rec is not None:
            from trimesh.ray import ray_pyembree as rp

            rp._EmbreeWrap.__init__ = _orig_init
            rp._EmbreeWrap.run = _orig_run
        captured.update(meshes=meshes, kw=kw, parts=parts, stats=stats,
                        seconds=dt, printed=buf.getvalue())
        return parts, stats

    from trimesh.ray import ray_pyembree as rp

    _orig_init, _orig_run = rp._EmbreeWrap.__init__, rp._EmbreeWrap.run
    po.place_leaves_organic = wrapped

    # Tree.scatter for one tree at the square's centre, seeded directly.
    x = y = 17.5
    tz = 5.0
    tree_rng = np.random.default_rng(seed)
    env = tree._sample_envelope(x, y, tz, tree_rng)
    from dharmatiles.dist import sample

    gw = float(sample(tree.group_width_mm, tree_rng)) if tree.group_width_mm is not None else None
    gh = float(sample(tree.group_height_mm, tree_rng)) if tree.group_height_mm is not None else None
    t0 = time.perf_counter()
    nodes, parents, radii, in_dirs, out_dirs, _attr, _gl = grow_skeleton(
        env, tree_rng,
        n_attractors=tree.n_attractors,
        segment_length_mm=tree.segment_length_mm,
        min_radius_mm=tree.min_radius_mm,
        branch_split_angle_deg=tree.branch_split_angle_deg,
        target_fdm_angle_deg=tree.target_fdm_angle_deg,
        max_branches_per_step=tree.max_branches_per_step,
        branch_exponent=tree.branch_exponent,
        smoothing_alpha=tree.smoothing_alpha,
        group_width_mm=gw, group_height_mm=gh,
        foliage_bulge_mm=tree.foliage_bulge_mm,
        branch_split_eagerness=tree.branch_split_eagerness,
        branch_target=tree.branch_target,
        branch_fork_balance=tree.branch_fork_balance,
    )
    build_branch_mesh(
        nodes, parents, radii, in_dirs, out_dirs,
        terrain_z=tz,
        strict_fdm_angle_deg=tree.strict_fdm_angle_deg,
        foliage_cluster_radius_mm=tree.foliage_cluster_radius_mm,
        foliage_cluster_length_mm=tree.foliage_cluster_length_mm,
        bark=tree.bark, bark_seed=seed,
        leaves=True,
        leaf_length_mm=tree.leaf_length_mm,
        leaf_width_mm=tree.leaf_width_mm,
        leaf_thickness_mm=tree.leaf_thickness_mm,
        leaf_fold_angle_deg=tree.leaf_fold_angle_deg,
        leaf_inner_curve=tree.leaf_inner_curve,
        leaf_outer_curve=tree.leaf_outer_curve,
        leaf_curl_deg=tree.leaf_curl_deg,
    )
    tree_s = time.perf_counter() - t0
    po.place_leaves_organic = orig

    meshes, kw = captured["meshes"], captured["kw"]
    seeds_list = kw["seeds"]
    seed0 = int(seeds_list[0]) if isinstance(seeds_list, list) else int(seeds_list)
    L, W = float(kw["length_mm"]), float(kw["width_mm"])
    spacing = max(po._ORGANIC_SPACING_FRAC * W, 0.3)

    # Re-derive the union and the two sample sets exactly as the placer did.
    union = po._union_surface(meshes)
    area = float(union.area)
    rng = np.random.default_rng(seed0 & 0xFFFFFFFF)
    n_cand = int(max(64, po._ORGANIC_CANDIDATE_FACTOR * area / (spacing ** 2)))
    cpts, cnrm = po._surface_points_with_normals(union, n_cand, rng)
    n_test = int(max(64, area / (po._ORGANIC_VERIFY_RES_MM ** 2)))
    tpts, tnrm = po._surface_points_with_normals(
        union, n_test, np.random.default_rng((seed0 ^ 0x5EED) & 0xFFFFFFFF),
    )

    wood = kw.get("avoid_meshes") or []
    wv, wf, off = [], [], 0
    for m in wood:
        wv.append(np.asarray(m.vertices))
        wf.append(np.asarray(m.faces) + off)
        off += len(m.vertices)

    leaves = [s for p in captured["parts"] for s in p]
    bases = np.array([b for s in captured["stats"] for b in s.base_positions]).reshape(-1, 3)
    dirs = np.array([t for s in captured["stats"] for t in s.base_tangents]).reshape(-1, 3)
    lv = np.concatenate([np.asarray(s.vertices) for s in leaves]) if leaves else np.zeros((0, 3))
    lnv = np.array([len(s.vertices) for s in leaves], np.uint32)
    lf = np.concatenate([np.asarray(s.faces) for s in leaves]) if leaves else np.zeros((0, 3))
    lnf = np.array([len(s.faces) for s in leaves], np.uint32)

    w = Writer(out / "leaves.bin")
    w.f64("foliage/v", union.vertices)
    w.u32("foliage/f", union.faces)
    w.f64("wood/v", np.concatenate(wv) if wv else np.zeros((0, 3)))
    w.u32("wood/f", np.concatenate(wf) if wf else np.zeros((0, 3)))
    put = w.f32 if compact else w.f64
    put("cand/p", cpts)
    put("cand/n", cnrm)
    put("test/p", tpts)
    put("test/n", tnrm)
    w.f64("py/base", bases)
    w.f64("py/dir", dirs)
    if not compact:
        w.f64("py/leaf_v", lv)
        w.u32("py/leaf_nv", lnv)
        w.u32("py/leaf_f", lf)
        w.u32("py/leaf_nf", lnf)
    w.u32("seed0", [seed0 & 0xFFFFFFFF, seed0 >> 32])

    m = re.search(
        r"(\d+) placed / (\d+) roots.*?build-fail=(\d+), branch-cull=(\d+), "
        r"uncovered-test-pts=(\d+)\)\s+([\d.]+)s",
        captured["printed"], re.S,
    )
    meta = {
        "tile": str(TILE.relative_to(REPO)),
        "tree_seed": seed,
        "clumps": len(meshes),
        "seed0": seed0,
        "shape": {k: float(kw[k]) for k in (
            "length_mm", "width_mm", "thickness_mm", "fold_angle_deg",
            "inner_curve", "outer_curve", "curl_deg")},
        "union_tris": len(union.faces),
        "union_area_mm2": area,
        "wood_tubes": len(wood),
        "candidates": len(cpts),
        "tests": len(tpts),
        "placed": int(m.group(1)) if m else len(leaves),
        "roots": int(m.group(2)) if m else None,
        "build_fail": int(m.group(3)) if m else None,
        "branch_cull": int(m.group(4)) if m else None,
        "uncovered": int(m.group(5)) if m else None,
        "placer_s_printed": float(m.group(6)) if m else None,
        "placer_s": captured["seconds"],
        "tree_s": tree_s,
    }
    w.u32("py/counts", [meta[k] or 0 for k in (
        "placed", "roots", "build_fail", "branch_cull", "uncovered")])
    w.f64("py/seconds", [meta["placer_s"]])
    w.f64("shape", [meta["shape"][k] for k in (
        "length_mm", "width_mm", "thickness_mm", "fold_angle_deg",
        "inner_curve", "outer_curve", "curl_deg")])
    w.close()
    if rec is not None:
        rec.close({"source": f"{meta['tile']} tree seed {seed}, leaf placement only"})
    (out / "meta.json").write_text(json.dumps(meta, indent=1))
    print(json.dumps(meta, indent=1))


if __name__ == "__main__":
    main()
