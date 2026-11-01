#!/usr/bin/env python3
"""Summarise R1 replays: Rust CSVs (r1_replay) against the Python replay of
the same inputs (replay_py.py --out) and against the in-situ capture.

Usage:
    python tests/parity/capture/summarise_r1.py OUT_DIR CASE...

Expects OUT_DIR/CASE.{ffi,port}.{seq,par}.csv and CASE.py.{face,nofid}.jsonl.
The ffi backend cannot pass face IDs in, so it is compared with the
no-face-ID Python replay; the port with the face-ID one.
"""
from __future__ import annotations

import csv
import json
import pathlib
import sys


def load_py(p: pathlib.Path):
    out = {}
    for line in open(p):
        r = json.loads(line)
        out[r['id']] = r
    return out


def main() -> None:
    out = pathlib.Path(sys.argv[1])
    rows = []
    for case in sys.argv[2:]:
        py = {v: load_py(out / f'{case}.py.{v}.jsonl') for v in ('face', 'nofid')}
        py_s = {v: sum(r['ms'] for r in py[v].values()) / 1e3 for v in py}
        for b in ('ffi', 'port'):
            ref = py['nofid' if b == 'ffi' else 'face']
            for par in ('seq', 'par'):
                f = out / f'{case}.{b}.{par}.csv'
                if not f.exists():
                    continue
                s = dict(case=case, backend=b, par=par, results=0, ops=set(), ms=0.0,
                         insitu_ms=0.0, not_wt=0, status=0, tri_eq_replay=0,
                         tri_eq_insitu=0, vol_1e6_replay=0, max_rel=0.0, ids=0, props=0,
                         identical=0, import_fail=0)
                seen = set()
                for r in csv.DictReader(open(f)):
                    if r['k'] == 'import-failed':
                        s['import_fail'] += 1
                        continue
                    i, k = int(r['id']), int(r['k'])
                    if i not in seen:
                        seen.add(i)
                        s['ms'] += float(r['ms'])
                        s['insitu_ms'] += float(r['py_ms'])
                    s['results'] += 1
                    s['not_wt'] += r['watertight'] != 'true'
                    s['status'] += r['status_ok'] != 'true'
                    s['ids'] += r['ids_ok'] != 'true'
                    s['props'] += r['props_ok'] != 'true'
                    s['identical'] += r['identical'] == 'true'
                    s['tri_eq_insitu'] += r['tris'] == r['py_tris']
                    pr = ref.get(i)
                    if pr:
                        s['tri_eq_replay'] += int(r['tris']) == pr['tris'][k]
                        pv, v = pr['vol'][k], float(r['vol'])
                        rel = abs(v - pv) / abs(pv) if abs(pv) > 1e-9 else abs(v)
                        s['vol_1e6_replay'] += rel > 1e-6
                        s['max_rel'] = max(s['max_rel'], rel)
                s['ops'] = len(seen)
                s['py_replay_s'] = py_s['nofid' if b == 'ffi' else 'face']
                s['rust_s'] = s.pop('ms') / 1e3
                s['insitu_s'] = s.pop('insitu_ms') / 1e3
                s['ratio_vs_replay'] = s['rust_s'] / s['py_replay_s']
                rows.append(s)
    for s in rows:
        print(json.dumps(s))


if __name__ == '__main__':
    main()
