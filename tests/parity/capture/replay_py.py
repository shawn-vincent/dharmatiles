#!/usr/bin/env python3
"""Replay captured booleans in Python's manifold3d (R1 baseline).

The same inputs the Rust replay sees (``.dtm`` operands re-imported), timed
the same way (op + ``num_tri()``), so Rust and Python compare like for like.
``--no-face-id`` drops face IDs on import, as manifold-csg's C API must.

``--tolerance mesh`` passes each operand's saved tolerance into the import
(``Mesh64(tolerance=…)``, as the port's ``MeshGL`` import does; R1 found
manifold3d 3.5.1 ignores it: ``get_tolerance()`` stays at the default);
``--tolerance set`` imports at the default and then calls
``set_tolerance`` (all manifold-csg's C API allows). The default, ``none``,
imports every operand at the default tolerance for its size, as the first
R1 runs did. Python's in-situ operands were float32 imports, whose
tolerance is about 1.2e-7 x size, not the default ~1e-12 x size.

Usage:
    python tests/parity/capture/replay_py.py CAPTURE_DIR CASE [--no-face-id]
        [--tolerance none|mesh|set]
"""
from __future__ import annotations

import argparse
import json
import os
import pathlib
import struct
import time
from collections import OrderedDict

import manifold3d as m3d
import numpy as np


def read_dtm(path: pathlib.Path, face_id: bool, tolerance: str = 'none') -> m3d.Manifold:
    b = path.read_bytes()
    assert b[:8] == b'DTMESH01'
    num_prop, _, n_vert, n_tri, n_merge, n_ri, n_ro, n_rt, n_fi, n_rf, tol = struct.unpack_from(
        '<II8Qd', b, 8)
    o = 8 + 80

    def take(dtype, n):
        nonlocal o
        a = np.frombuffer(b, dtype=dtype, count=n, offset=o).copy()
        o += a.nbytes
        return a

    vp = take('<f8', n_vert * num_prop).reshape(n_vert, num_prop)
    tv = take('<u4', n_tri * 3).reshape(n_tri, 3).astype(np.uint64)
    mf = take('<u4', n_merge).astype(np.uint64)
    mt = take('<u4', n_merge).astype(np.uint64)
    ri = take('<u4', n_ri).astype(np.uint64)
    ro = take('<u4', n_ro).astype(np.uint32)
    take('<f8', n_rt)
    fi = take('<u4', n_fi).astype(np.uint64)
    kw = dict(merge_from_vert=mf, merge_to_vert=mt, run_index=ri, run_original_id=ro)
    if face_id and n_fi:
        kw['face_id'] = fi
    if tolerance == 'mesh':
        kw['tolerance'] = tol
    m = m3d.Manifold(mesh=m3d.Mesh64(vert_properties=vp, tri_verts=tv, **kw))
    if tolerance == 'set' and tol > m.get_tolerance():
        m = m.set_tolerance(tol)
    return m


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument('dir', type=pathlib.Path)
    ap.add_argument('case')
    ap.add_argument('--no-face-id', action='store_true')
    ap.add_argument('--tolerance', choices=('none', 'mesh', 'set'), default='none')
    ap.add_argument('--out', type=pathlib.Path, help='per-op JSON lines (ms, tris, vol)')
    args = ap.parse_args()
    per_op = open(args.out, 'w') if args.out else None
    cache: OrderedDict[str, m3d.Manifold] = OrderedDict()

    def get(h):
        if h not in cache:
            cache[h] = read_dtm(args.dir / 'meshes' / f'{h}.dtm', not args.no_face_id,
                                args.tolerance)
            if len(cache) > 48:
                cache.popitem(last=False)
        return cache[h]

    M, Op = m3d.Manifold, m3d.OpType
    batch = {'batch_add': Op.Add, 'batch_subtract': Op.Subtract, 'batch_intersect': Op.Intersect}
    tot = dict(ops=0, ms=0.0, tri_mismatch=0, vol_over_1e6=0)
    for line in open(args.dir / f'{args.case}.jsonl'):
        r = json.loads(line)
        ins = [get(h) for h in r['operands']]
        for m in ins:
            m.num_tri()
        p = r['params']
        t0 = time.perf_counter()
        op = r['op']
        if op == 'add':
            outs = (ins[0] + ins[1],)
        elif op == 'subtract':
            outs = (ins[0] - ins[1],)
        elif op == 'intersect':
            outs = (ins[0] ^ ins[1],)
        elif op in batch:
            outs = (M.batch_boolean(ins, batch[op]),)
        elif op == 'trim_by_plane':
            outs = (ins[0].trim_by_plane(p['normal'], p['offset']),)
        elif op == 'split':
            outs = ins[0].split(ins[1])
        elif op == 'split_by_plane':
            outs = ins[0].split_by_plane(p['normal'], p['offset'])
        else:
            raise SystemExit(op)
        tris = [o.num_tri() for o in outs]
        ms = (time.perf_counter() - t0) * 1e3
        tot['ms'] += ms
        tot['ops'] += 1
        if per_op:
            per_op.write(json.dumps(dict(id=r['id'], ms=ms, tris=tris,
                                         vol=[o.volume() for o in outs])) + '\n')
        for k, o in enumerate(outs):
            pv = r['result_vol'][k]
            rel = abs(o.volume() - pv) / abs(pv) if abs(pv) > 1e-9 else abs(o.volume())
            tot['tri_mismatch'] += tris[k] != r['result_tris'][k]
            tot['vol_over_1e6'] += rel > 1e-6
    if per_op:
        per_op.close()
    tot.update(case=args.case, face_id=not args.no_face_id, tolerance=args.tolerance,
               op_s=tot.pop('ms') / 1e3)
    print(json.dumps(tot), flush=True)
    os._exit(0)


if __name__ == '__main__':
    main()
