"""R3 capture: record every Embree ray batch a build makes, from outside.

Phase 0 spike R3 (docs/analysis/rust-rewrite/plan.html).  Wraps trimesh's
``_EmbreeWrap`` (the thin layer between trimesh and embreex) so neither
``src/`` nor ``clifflab/`` is edited.  What is recorded is exactly what
Embree saw: the float32 vertices and faces of each scene (in trimesh's
scaled-to-box frame) and the float32 origins and directions of every
``run`` call, together with Embree's closest hit (primID, tfar).

Each ray call is labelled by the nearest dharmatiles/clifflab frame on the
stack, so leaf-placer batches (``placement_leaf`` / ``placement_organic``)
can be told apart from the rest of the build.

Usage::

    python tests/parity/capture/rays.py tree  OUT_DIR   # 2x2-grass-tree, DB scale
    python tests/parity/capture/rays.py cliff OUT_DIR   # clifflab rockpile 1x1, seed 1
    python tests/parity/capture/rays.py bench OUT_DIR   # Embree build/query timings

Output (little-endian), read by ``crates/dt-geom/examples/r3_rays.rs``::

    scenes.bin   repeated: b"DTSC" u32 id, u32 nv, u32 nf, f64 scale,
                 f32 v[nv*3], u32 f[nf*3]
    batches.bin  repeated: b"DTRB" u32 scene, u32 len, label[len], u32 n,
                 f32 o[n*3], f32 d[n*3], i32 prim[n], f32 tfar[n]
    manifest.json  counts per label, scene sizes
    embree.json    (bench) per-scene Embree build and query seconds
    embree.tsv     (bench) the same, as: scene  tris  rays  build_s  query_s

Never writes into the repository: OUT_DIR is the caller's scratch area.
No STL is written; the tile is built in memory only.
"""

from __future__ import annotations

import hashlib
import json
import pathlib
import struct
import sys
import time

import numpy as np

REPO = pathlib.Path(__file__).resolve().parents[3]


class Recorder:
    """Monkeypatches trimesh's embree wrapper and streams batches to disk."""

    def __init__(self, out: pathlib.Path):
        out.mkdir(parents=True, exist_ok=True)
        self.out = out
        self.scenes = open(out / "scenes.bin", "wb")
        self.batches = open(out / "batches.bin", "wb")
        self.ids: dict[bytes, int] = {}
        self.sizes: list[tuple[int, int]] = []
        self.labels: dict[str, dict] = {}

    def scene_id(self, verts32: np.ndarray, faces32: np.ndarray, scale: float) -> int:
        key = hashlib.blake2b(verts32.tobytes() + faces32.tobytes(), digest_size=16).digest()
        sid = self.ids.get(key)
        if sid is None:
            sid = len(self.ids)
            self.ids[key] = sid
            nv, nf = len(verts32), len(faces32)
            self.sizes.append((nv, nf))
            self.scenes.write(b"DTSC" + struct.pack("<IIId", sid, nv, nf, scale))
            self.scenes.write(np.ascontiguousarray(verts32, "<f4").tobytes())
            self.scenes.write(np.ascontiguousarray(faces32, "<u4").tobytes())
        return sid

    def batch(self, sid, label, o32, d32, prim, tfar):
        lb = label.encode()
        n = len(o32)
        self.batches.write(b"DTRB" + struct.pack("<II", sid, len(lb)) + lb + struct.pack("<I", n))
        for a, t in ((o32, "<f4"), (d32, "<f4"), (prim, "<i4"), (tfar, "<f4")):
            self.batches.write(np.ascontiguousarray(a, t).tobytes())
        s = self.labels.setdefault(label, {"calls": 0, "rays": 0, "hits": 0, "scenes": set()})
        s["calls"] += 1
        s["rays"] += n
        s["hits"] += int((prim >= 0).sum())
        s["scenes"].add(sid)

    def install(self):
        from trimesh.ray import ray_pyembree as rp

        rec = self
        orig_init = rp._EmbreeWrap.__init__

        def init(self, vertices, faces, scale):
            orig_init(self, vertices, faces, scale)
            v = ((np.asarray(vertices, np.float64) - self.origin) * self.scale).astype(np.float32)
            f = np.asarray(faces).astype(np.int32).astype(np.uint32)
            self._r3_sid = rec.scene_id(v, f, self.scale)

        def run(self, origins, normals, **kwargs):
            o = ((np.array(origins, dtype=np.float64) - self.origin) * self.scale).astype(np.float32)
            d = np.asarray(normals).astype(np.float32)
            res = self.scene.run(o, d, output=1)
            prim = np.asarray(res["primID"], np.int32)
            rec.batch(self._r3_sid, _label(), o, d, prim, np.asarray(res["tfar"], np.float32))
            if kwargs.get("output"):
                return res
            return prim

        rp._EmbreeWrap.__init__ = init
        rp._EmbreeWrap.run = run

    def close(self, extra: dict):
        self.scenes.close()
        self.batches.close()
        labels = {k: {**v, "scenes": len(v["scenes"])} for k, v in self.labels.items()}
        man = {
            **extra,
            "scenes": len(self.sizes),
            "max_triangles": max((f for _, f in self.sizes), default=0),
            "total_triangles": sum(f for _, f in self.sizes),
            "labels": labels,
        }
        (self.out / "manifest.json").write_text(json.dumps(man, indent=2))
        return man


def _label() -> str:
    """Nearest project frame (``module:function``) that led to this ray call."""
    f = sys._getframe(2)
    while f is not None:
        fn = f.f_code.co_filename
        if ("dharmatiles" in fn or "clifflab" in fn) and "/trimesh/" not in fn:
            return f"{pathlib.Path(fn).stem}:{f.f_code.co_name}"
        f = f.f_back
    return "other"


def capture_tree(out: pathlib.Path) -> dict:
    sys.path.insert(0, str(REPO / "src"))
    rec = Recorder(out)
    rec.install()
    from dharmatiles.terrains.tile import build_meshes_for_render

    t0 = time.perf_counter()
    meshes = build_meshes_for_render(REPO / "src/tiles/ground/2x2-grass-tree.tile.py", system="db")
    dt = time.perf_counter() - t0
    return rec.close({"source": "src/tiles/ground/2x2-grass-tree.tile.py (db)",
                      "build_s": dt, "parts": len(meshes)})


def capture_cliff(out: pathlib.Path) -> dict:
    sys.path.insert(0, str(REPO / "clifflab"))
    sys.path.insert(0, str(REPO / "src"))
    rec = Recorder(out)
    rec.install()
    import rockpile

    t0 = time.perf_counter()
    rockpile.build(seed=1, form="run", cols=1, rows=1)
    dt = time.perf_counter() - t0
    return rec.close({"source": "clifflab/rockpile.py build(seed=1, 1x1)", "build_s": dt})


def read_scenes(out: pathlib.Path):
    buf = (out / "scenes.bin").read_bytes()
    i, scenes = 0, {}
    while i < len(buf):
        assert buf[i:i + 4] == b"DTSC"
        sid, nv, nf, scale = struct.unpack_from("<IIId", buf, i + 4)
        i += 24
        v = np.frombuffer(buf, "<f4", nv * 3, i).reshape(nv, 3)
        i += nv * 12
        f = np.frombuffer(buf, "<u4", nf * 3, i).reshape(nf, 3)
        i += nf * 12
        scenes[sid] = (v, f, scale)
    return scenes


def read_batches(out: pathlib.Path):
    buf = (out / "batches.bin").read_bytes()
    i = 0
    while i < len(buf):
        assert buf[i:i + 4] == b"DTRB"
        sid, ln = struct.unpack_from("<II", buf, i + 4)
        i += 12
        label = buf[i:i + ln].decode()
        i += ln
        (n,) = struct.unpack_from("<I", buf, i)
        i += 4
        o = np.frombuffer(buf, "<f4", n * 3, i).reshape(n, 3)
        i += n * 12
        d = np.frombuffer(buf, "<f4", n * 3, i).reshape(n, 3)
        i += n * 12
        prim = np.frombuffer(buf, "<i4", n, i)
        i += n * 4
        tfar = np.frombuffer(buf, "<f4", n, i)
        i += n * 4
        yield sid, label, o, d, prim, tfar


def bench(out: pathlib.Path, reps: int = 5) -> dict:
    """Embree reference timings, replayed the way the Rust example replays.

    Per scene: build = construct + commit (forced by a one-ray run);
    query = every recorded ray for that scene in one ``run`` call.  Best of
    ``reps``.  Also re-checks that Embree's answers are reproducible.
    """
    from embreex import rtcore_scene
    from embreex.mesh_construction import TriangleMesh

    scenes = read_scenes(out)
    rays: dict[int, list] = {}
    for sid, _label_, o, d, prim, _t in read_batches(out):
        rays.setdefault(sid, []).append((o, d, prim))
    one_o = np.zeros((1, 3), np.float32)
    one_d = np.array([[0, 0, 1]], np.float32)
    res, mism = {}, 0
    for sid, (v, f, _scale) in scenes.items():
        if sid not in rays:
            continue
        o = np.concatenate([r[0] for r in rays[sid]])
        d = np.concatenate([r[1] for r in rays[sid]])
        p = np.concatenate([r[2] for r in rays[sid]])
        bt, qt = [], []
        for _ in range(reps):
            t0 = time.perf_counter()
            s = rtcore_scene.EmbreeScene()
            TriangleMesh(scene=s, vertices=np.ascontiguousarray(v), indices=f.astype(np.int32))
            s.run(one_o, one_d)
            bt.append(time.perf_counter() - t0)
            t0 = time.perf_counter()
            q = s.run(o, d)
            qt.append(time.perf_counter() - t0)
        mism += int((q != p).sum())
        res[str(sid)] = {"tris": len(f), "rays": len(o), "build_s": min(bt), "query_s": min(qt)}
    tot = {"build_s": sum(r["build_s"] for r in res.values()),
           "query_s": sum(r["query_s"] for r in res.values()),
           "rays": sum(r["rays"] for r in res.values()),
           "rerun_mismatches": mism}
    js = {"total": tot, "scenes": res}
    (out / "embree.json").write_text(json.dumps(js, indent=1))
    (out / "embree.tsv").write_text("".join(
        f"{sid}\t{r['tris']}\t{r['rays']}\t{r['build_s']:.9f}\t{r['query_s']:.9f}\n"
        for sid, r in res.items()))
    return tot


def main():
    mode, out = sys.argv[1], pathlib.Path(sys.argv[2])
    t0 = time.perf_counter()
    r = {"tree": capture_tree, "cliff": capture_cliff, "bench": bench}[mode](out)
    print(json.dumps(r, indent=2, default=str))
    print(f"{mode}: {time.perf_counter() - t0:.1f}s -> {out}")


if __name__ == "__main__":
    main()
