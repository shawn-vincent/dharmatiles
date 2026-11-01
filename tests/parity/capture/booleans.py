#!/usr/bin/env python3
"""Capture every manifold3d boolean a lab or tile performs (R1 spike).

Wraps manifold3d from the outside: the boolean entry points of
``manifold3d.Manifold`` (``+``, ``-``, ``^``, ``batch_boolean``,
``trim_by_plane``, ``split``, ``split_by_plane``) are monkeypatched, so the
labs run unmodified.  For every call the operands and the result are written
as binary meshes (``.dtm``, deduplicated by content hash) and one JSON line
records the op, its parameters and Python's time.

Manifold is lazy: ``a + b`` only builds a CSG node.  To time each op on its
own, the wrapper first forces the operands (outside the timer), then times
the op plus ``num_tri()``, which forces it.  That makes evaluation eager, so
an instrumented build can differ slightly from a plain one (nested unions are
no longer flattened into one batch); the build's own wall time is recorded
both ways.

Usage (from the repo root of a worktree):
    python tests/parity/capture/booleans.py CASE --out DIR

CASE is one of: rockpile-1x1 (seed 1, form run), massif-2x2 (rockpile form
massif, 2x2, seed 1), grass-tree-2x2 (src/tiles/ground/2x2-grass-tree.tile.py).
``--plain`` runs the build without capture, for the uninstrumented time.

File format ``.dtm`` (little-endian), read by crates/dt-geom/examples/r1_replay.rs:
    b"DTMESH01"
    u32 num_prop, u32 0
    u64 n_vert, n_tri, n_merge, n_run_index, n_run_original_id,
        n_run_transform (f64 count), n_face_id, n_run_flags
    f64 tolerance
    f64 vert_properties[n_vert * num_prop]
    u32 tri_verts[n_tri * 3]
    u32 merge_from[n_merge], merge_to[n_merge]
    u32 run_index[n_run_index], run_original_id[n_run_original_id]
    f64 run_transform[n_run_transform]
    u32 face_id[n_face_id]
    u8  run_flags[n_run_flags]
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import pathlib
import struct
import sys
import time

import numpy as np

REPO = pathlib.Path(__file__).resolve().parents[3]
# Use this worktree's frozen copies of the package and the labs, never the
# main checkout's (which is changing).
sys.path.insert(0, str(REPO / 'src'))
sys.path.insert(0, str(REPO / 'clifflab'))
sys.path.insert(0, str(REPO))

import manifold3d as m3d  # noqa: E402

MAGIC = b'DTMESH01'


def mesh_bytes(man: m3d.Manifold) -> tuple[bytes, int]:
    g = man.to_mesh64()
    vp = np.ascontiguousarray(g.vert_properties, dtype='<f8')
    num_prop = vp.shape[1] if vp.ndim == 2 and vp.size else 3
    tv = np.ascontiguousarray(g.tri_verts, dtype='<u4')
    mf = np.ascontiguousarray(g.merge_from_vert, dtype='<u4')
    mt = np.ascontiguousarray(g.merge_to_vert, dtype='<u4')
    ri = np.ascontiguousarray(g.run_index, dtype='<u4')
    ro = np.ascontiguousarray(g.run_original_id, dtype='<u4')
    rt = np.ascontiguousarray(g.run_transform, dtype='<f8').ravel()
    fi = np.ascontiguousarray(g.face_id, dtype='<u4')
    rf = np.ascontiguousarray(g.run_flags, dtype='u1')
    het = np.asarray(g.halfedge_tangent)
    if het.size and np.any(het):
        raise RuntimeError(f'halfedge tangents are not captured: {het.shape}')
    n_vert = vp.shape[0] if vp.ndim == 2 else 0
    head = MAGIC + struct.pack('<II8Qd', num_prop, 0, n_vert, len(tv), len(mf), len(ri),
                               len(ro), len(rt), len(fi), len(rf),
                               float(man.get_tolerance()))
    body = b''.join(a.tobytes() for a in (vp, tv, mf, mt, ri, ro, rt, fi, rf))
    return head + body, len(tv)


class Recorder:
    def __init__(self, out: pathlib.Path, case: str):
        self.out = out
        (out / 'meshes').mkdir(parents=True, exist_ok=True)
        self.log = open(out / f'{case}.jsonl', 'w')
        self.n = 0
        # id(manifold) -> (manifold, hash) for recent objects; holding the
        # object keeps its id unique while it is cached.
        self.seen: dict[int, tuple] = {}
        self.bytes_written = 0
        self.save_s = 0.0

    def save(self, man: m3d.Manifold) -> str:
        key = id(man)
        if key in self.seen and self.seen[key][0] is man:
            return self.seen[key][1]
        t0 = time.perf_counter()
        data, _ = mesh_bytes(man)
        h = hashlib.blake2b(data, digest_size=12).hexdigest()
        path = self.out / 'meshes' / f'{h}.dtm'
        if not path.exists():
            path.write_bytes(data)
            self.bytes_written += len(data)
        self.seen[key] = (man, h)
        if len(self.seen) > 256:
            del self.seen[next(iter(self.seen))]
        self.save_s += time.perf_counter() - t0
        return h

    def record(self, kind, operands, params, run):
        for m in operands:          # force operands outside the timer
            m.num_tri()
        t0 = time.perf_counter()
        result = run()
        outs = result if isinstance(result, tuple) else (result,)
        for o in outs:
            o.num_tri()
        dt = time.perf_counter() - t0
        rec = dict(
            id=self.n, op=kind, params=params, py_ms=dt * 1e3,
            operands=[self.save(m) for m in operands],
            operand_tris=[m.num_tri() for m in operands],
            results=[self.save(o) for o in outs],
            result_tris=[o.num_tri() for o in outs],
            result_vol=[o.volume() for o in outs],
            status=[str(o.status()) for o in outs],
        )
        self.log.write(json.dumps(rec) + '\n')
        self.n += 1
        return result


def install(rec: Recorder) -> None:
    M = m3d.Manifold
    add, sub, xor = M.__add__, M.__sub__, M.__xor__
    batch, trim, split, split_plane = M.batch_boolean, M.trim_by_plane, M.split, M.split_by_plane
    op_name = {m3d.OpType.Add: 'add', m3d.OpType.Subtract: 'subtract',
               m3d.OpType.Intersect: 'intersect'}

    M.__add__ = lambda a, b: rec.record('add', [a, b], {}, lambda: add(a, b))
    M.__sub__ = lambda a, b: rec.record('subtract', [a, b], {}, lambda: sub(a, b))
    M.__xor__ = lambda a, b: rec.record('intersect', [a, b], {}, lambda: xor(a, b))

    def batch_boolean(ms, op):
        ms = list(ms)
        return rec.record('batch_' + op_name[op], ms, {}, lambda: batch(ms, op))

    def trim_by_plane(self, normal, origin_offset):
        n = [float(v) for v in normal]
        return rec.record('trim_by_plane', [self], dict(normal=n, offset=float(origin_offset)),
                          lambda: trim(self, normal, origin_offset))

    def split_(self, cutter):
        return rec.record('split', [self, cutter], {}, lambda: split(self, cutter))

    def split_by_plane(self, normal, origin_offset):
        n = [float(v) for v in normal]
        return rec.record('split_by_plane', [self], dict(normal=n, offset=float(origin_offset)),
                          lambda: split_plane(self, normal, origin_offset))

    M.batch_boolean = staticmethod(batch_boolean)
    M.trim_by_plane = trim_by_plane
    M.split = split_
    M.split_by_plane = split_by_plane


def run_case(case: str, scratch: pathlib.Path) -> None:
    if case in ('rockpile-1x1', 'massif-2x2'):
        import rockpile
        if case == 'rockpile-1x1':
            r = rockpile.build(1, 'run', 1, 1)
        else:
            r = rockpile.build(1, 'massif', 2, 2)
        r.mesh.export(scratch / f'{case}.stl')
        print(f'{case}: {len(r.mesh.faces):,} faces, watertight={r.mesh.is_watertight}')
    elif case == 'grass-tree-2x2':
        from dharmatiles.spec import load_tile
        from dharmatiles.terrains import tile as tt
        spec = REPO / 'src/tiles/ground/2x2-grass-tree.tile.py'
        reporter = tt.make_reporter(quiet=True)
        for t in load_tile(spec):
            paths = {s.suffix: scratch / f'{case}-{s.suffix}.stl' for s in t.systems}
            tt.build_tile_from_spec(t, system_paths=paths, reporter=reporter)
        print(f'{case}: built')
    else:
        raise SystemExit(f'unknown case {case}')


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('case')
    ap.add_argument('--out', required=True, type=pathlib.Path)
    ap.add_argument('--plain', action='store_true', help='no capture: time the build only')
    args = ap.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    os.chdir(REPO)
    rec = None
    if not args.plain:
        rec = Recorder(args.out, args.case)
        install(rec)
    t0 = time.perf_counter()
    run_case(args.case, args.out)
    wall = time.perf_counter() - t0
    summary = dict(case=args.case, plain=args.plain, wall_s=wall,
                   manifold3d=getattr(m3d, '__version__', 'unknown'))
    if rec is not None:
        rec.log.close()
        summary.update(ops=rec.n, capture_s=rec.save_s, mb_written=rec.bytes_written / 1e6)
    name = f'{args.case}.{"plain" if args.plain else "capture"}.summary.json'
    (args.out / name).write_text(json.dumps(summary, indent=1))
    print(json.dumps(summary), flush=True)
    os._exit(0)   # skip nanobind's leak report for the patched class


if __name__ == '__main__':
    main()
