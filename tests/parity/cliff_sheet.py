#!/usr/bin/env python3
"""Parity harness for the cliff (P7): clifflab's rockpile at round 17
(31f7c5d) beside the Rust cliff, per form and seed.

Each row pair is Python then Rust, rendered with clifflab's own painter and
views (iterate.py: oblique, raking, front, end, a close-up of the cliff
face) and the reference-sheet crop for the form, overhang painted red by
rockpile's layer-reach audit on both meshes.  Metrics: triangles, stones,
overhang (mm², rockpile's audit), see-through holes and watertightness
(the Rust gate run on both STLs), build seconds.

Python builds are cached in OUT/py (they take a minute each);
``--rebuild`` redoes them.  The Rust half is made by ``cargo xtask parity
cliff``: OUT/rust/<form>/s<seed>/ holds its STL and build record.

Usage::

    python tests/parity/cliff_sheet.py --forms run --seeds 1 2 3 4 --out out/parity/cliff
"""
from __future__ import annotations

import argparse
import json
import pathlib
import subprocess
import sys
import time

import numpy as np
import trimesh
from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parents[2]
for p in ("src", "clifflab", "rocklab", "dirtlab", "grasslab"):
    sys.path.insert(0, str(ROOT / p))
import iterate as it   # noqa: E402  (clifflab/iterate.py: paint, views, reference crops)
import rockpile as rp  # noqa: E402
sys.path.insert(0, str(ROOT / "tests" / "parity"))
import run_lab as rl  # noqa: E402  (its renderer: pyrender, else the CPU stand-in)

REF = {"run": "B", "corner": "C", "canyon": "I", "escarpment": "D", "plateau": "A",
       "massif": "M", "mound": "F", "spire": "E", "needles": "J"}
FOOT_SHIFT = rp.FOOT_Z_MM - 3.4      # the Rust tile's edge level is 3.4 mm
H = 420


def gate(stl: pathlib.Path) -> dict:
    """The Rust gate's audit of an STL (holes, bodies, watertight)."""
    exe = ROOT / "target" / "release" / "dharmatiles"
    r = subprocess.run([str(exe), "audit", str(stl), "--json"], capture_output=True, text=True)
    try:
        return json.loads(r.stdout).get("audit", json.loads(r.stdout))
    except Exception:
        return {}


def python_side(form: str, seed: int, out: pathlib.Path, rebuild: bool) -> tuple[trimesh.Trimesh, dict]:
    d = out / "py"
    d.mkdir(parents=True, exist_ok=True)
    stl, js = d / f"{form}-s{seed}.stl", d / f"{form}-s{seed}.json"
    if rebuild or not (stl.exists() and js.exists()):
        t0 = time.perf_counter()
        r = rp.build(seed, form, 1, 1)
        r.mesh.export(stl)
        js.write_text(json.dumps({"s": time.perf_counter() - t0, "rocks": r.rocks, "tops": r.tops,
                                  "tm": r.tm}))
    m = trimesh.load(stl, process=False)
    return m, json.loads(js.read_text())


def rust_side(form: str, seed: int, out: pathlib.Path):
    d = out / "rust" / form / f"s{seed}"
    stls = sorted(d.rglob("*-db.stl"))
    if not stls:
        return None, {}
    rec = json.loads(next(iter(sorted(d.rglob("*-db.build.json")))).read_text())
    m = trimesh.load(stls[0], process=False)
    m.vertices[:, 2] += FOOT_SHIFT
    notes = {k: v for k, v in rec.get("notes", [])} if isinstance(rec.get("notes"), list) else rec.get("notes", {})
    return m, {"s": rec.get("build_s"), "rocks": notes.get("cliff_stones"), "audit": rec.get("audit", {}),
               "stl": str(stls[0])}


def views(mesh: trimesh.Trimesh, tmp: pathlib.Path) -> tuple[list, float]:
    pm = it.paint(mesh)
    # both colours (Shawn, 4 Nov 2026): orange 55-65°, red over 65°
    fc = np.asarray(pm.visual.face_colors).copy()
    soft = rp.overhang_faces(pm)
    a55 = float(mesh.area_faces[rp.overhang_faces(mesh)].sum())
    saved, rp.OVERHANG_DEG = rp.OVERHANG_DEG, 65.0
    try:
        hard = rp.overhang_faces(pm)
        ov = float(mesh.area_faces[rp.overhang_faces(mesh)].sum())
    finally:
        rp.OVERHANG_DEG = saved
    fc[soft, :3] = rl.ORANGE
    fc[hard, :3] = rl.RED
    pm.visual.face_colors = fc
    ims = []
    for name, el, az in it.VIEWS:
        rl.render([pm], tmp, el, az)
        im = Image.open(tmp).convert("RGB")
        im.thumbnail((H, H))
        ims.append((name, im))
    c = pm.triangles_center
    keep = (c[:, 1] > rp.SQUARE_MM - 26.0) & (c[:, 2] > rp.FOOT_Z_MM - 0.5)
    close = pm.submesh([np.nonzero(keep)[0]], append=True)
    rl.render([close], tmp, 18, -112)
    im = Image.open(tmp).convert("RGB")
    im.thumbnail((int(H * 1.4), H))
    ims.append(("close-up", im))
    return ims, ov, max(a55 - ov, 0.0)


def row(label: str, ims: list, text: str, ref: Image.Image | None) -> Image.Image:
    cols = list(ims) + ([("reference", ref)] if ref is not None else [])
    w = sum(i.width for _, i in cols) + 12 * (len(cols) + 1)
    sheet = Image.new("RGB", (w, H + 70), (245, 245, 242))
    d = ImageDraw.Draw(sheet)
    d.text((12, 8), label, fill=(20, 20, 20), font=it.font(24))
    d.text((12, 38), text, fill=(90, 90, 90), font=it.font(18))
    x = 12
    for _, im in cols:
        sheet.paste(im, (x, 66))
        x += im.width + 12
    return sheet


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--forms", nargs="+", default=["run"])
    ap.add_argument("--seeds", nargs="+", type=int, default=[1, 2, 3, 4])
    ap.add_argument("--out", default="out/parity/cliff")
    ap.add_argument("--rebuild", action="store_true")
    ap.add_argument("--no-python", action="store_true")
    a = ap.parse_args()
    out = (ROOT / a.out) if not pathlib.Path(a.out).is_absolute() else pathlib.Path(a.out)
    out.mkdir(parents=True, exist_ok=True)
    tmp = out / "_tmp.png"
    rows, metrics = [], {}
    for form in a.forms:
        ref = it.ref_crop(REF[form], H)
        for seed in a.seeds:
            key = f"{form}-s{seed}"
            metrics[key] = {}
            for who in ("python", "rust"):
                if who == "python":
                    if a.no_python and not (out / "py" / f"{form}-s{seed}.stl").exists():
                        continue
                    mesh, info = python_side(form, seed, out, a.rebuild)
                else:
                    mesh, info = rust_side(form, seed, out)
                    if mesh is None:
                        continue
                ims, ov, band = views(mesh, tmp)
                g = gate(pathlib.Path(info["stl"])) if who == "rust" else gate(out / "py" / f"{form}-s{seed}.stl")
                info.update(overhang_mm2=ov, band_mm2=band, holes=g.get("holes"), watertight=g.get("watertight"),
                            triangles=len(mesh.faces))
                metrics[key][who] = info
                text = (f"{info['triangles']:,} faces · {info.get('rocks')} stones · overhang(>65°) {ov:.2f} · band(55-65°) {band:.2f} mm² · "
                        f"holes {g.get('holes')} · watertight {g.get('watertight')} · "
                        f"{(info.get('s') or 0):.0f} s")
                r = row(f"{who} · {form} seed {seed}", ims, text, ref if who == "rust" else None)
                r.save(out / f"{key}-{who}.png")
                rows.append(r)
    if rows:
        wmax = max(r.width for r in rows)
        sheet = Image.new("RGB", (wmax, sum(r.height for r in rows)), (245, 245, 242))
        y = 0
        for r in rows:
            sheet.paste(r, (0, y))
            y += r.height
        sheet.save(out / "contact-sheet.png")
        print(f"contact sheet {out / 'contact-sheet.png'}")
    (out / "metrics.json").write_text(json.dumps(metrics, indent=1, default=str))


if __name__ == "__main__":
    main()
