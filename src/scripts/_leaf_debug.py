"""FDM wall-printability colouring for debug leaf scripts.

Kept separate from ``dharmatiles.trees.leaf`` so production imports of
``leaf.py`` do not pull in trimesh ray-casting analysis paths.

Import in debug scripts as::

    from _leaf_debug import color_leaf_walls_by_fdm
    from _leaf_debug import color_leaf_walls_diagnostic, print_leaf_diagnostic
"""
from __future__ import annotations

from collections import deque

import numpy as np
import trimesh

from dharmatiles.core.color import RGBA_FLAG_FAIL, RGBA_FLAG_PASS
from dharmatiles.trees.leaf import (
    _LEAF_FDM_FLOOR_DEG,
    _LEAF_FDM_SUPPORT_TOLERANCE_MM,
)

# Maximum XY distance (mm) from a printable face to an angle_ok isolated face
# for the Z-support pass to mark the isolated face as printable.
# Set to 2 mm — generous enough to cover arch-side faces that share no BFS
# path to the sphere yet have printable material directly below them in Z.
_LEAF_Z_SUPPORT_XY_MM: float = 2.0


def color_leaf_walls_by_fdm(
    mesh:         trimesh.Trimesh,
    wall_faces:   range,
    support_mesh: trimesh.Trimesh,
    *,
    floor_angle_deg:      float = _LEAF_FDM_FLOOR_DEG,
    support_tolerance_mm: float = _LEAF_FDM_SUPPORT_TOLERANCE_MM,
    color_ok:    np.ndarray = np.array(RGBA_FLAG_PASS, dtype=np.uint8),
    color_fail:  np.ndarray = np.array(RGBA_FLAG_FAIL, dtype=np.uint8),
) -> None:
    """Color wall faces green/red by FDM printability, in-place.

    Green (``RGBA_FLAG_PASS``) marks printable faces; red (``RGBA_FLAG_FAIL``)
    marks overhangs.  Both colours come from :mod:`dharmatiles.core.color` so
    any debug view shares the same pass/fail convention system-wide.

    A wall face is printable iff **both** conditions hold:

    1. **Angle-OK** — the face normal's Z component is above
       ``-sin(floor_angle_deg)`` (face is not tilted more than
       ``floor_angle_deg`` below horizontal).

    2. **Reachable** — the face is connected to the support mesh via a chain
       of angle-OK wall faces.  Grounding is established at the face centroid:
       a face is directly grounded if its centroid is inside ``support_mesh``,
       within ``support_tolerance_mm`` of the surface, or has ``support_mesh``
       geometry directly below it (downward ray hit).

    Additionally, a face that is directly grounded is marked printable
    regardless of its angle (the support mesh is right there — no different
    from the build plate).

    Printability determination uses two passes:

    **Pass 1 — Z-floored BFS** through the face adjacency graph.  Propagates
    from grounded faces through shared wall edges to any angle-OK face whose
    centroid Z ≥ the minimum Z of any grounded face.  The Z-floor prevents BFS
    from reaching arch faces that hang below the attachment point (no material
    below them in FDM) while still letting BFS traverse the arch freely above
    that level.

    **Pass 2 — Z-support propagation** catches faces that are topologically
    isolated from the BFS network (surrounded by angle-OK=False arch-shoulder
    faces in the face graph) yet have printable material directly below them in
    Z.  A face is marked printable in this pass if any already-printable face
    lies within ``_LEAF_Z_SUPPORT_XY_MM`` in XY at a strictly lower Z.  The
    pass repeats until stable, propagating printability upward through chains
    of such faces.

    **Why face centroid, not lowest vertex?**

    The solidify_leaf root ring is embedded into the parent mesh by a
    *angled* raycast — the root vertex for a cantilevered tip is physically
    WEST of the tip perimeter vertex, embedded into the sphere wall, even
    though the tip face hangs freely in the air to the EAST.  Using the
    root vertex's inside-mesh status trivially marks every face as grounded
    because the root vertex is always inside the closed mesh.  The face
    centroid correctly identifies whether the face itself is adjacent to the
    mesh.

    Surface and cap faces are not touched.

    Parameters
    ----------
    mesh                 : Solid returned by :func:`~dharmatiles.trees.leaf.solidify_leaf`.
    wall_faces           : Range of wall face indices from ``solidify_leaf``.
    support_mesh         : Mesh the leaf rests on (sphere + trunk, etc.).
    floor_angle_deg      : Printability floor angle (degrees from horizontal).
    support_tolerance_mm : Distance tolerance for on-surface detection at the
                           face centroid.
    color_ok             : RGBA colour for printable faces (default: ``RGBA_FLAG_PASS``).
    color_fail           : RGBA colour for overhang faces (default: ``RGBA_FLAG_FAIL``).
    """
    threshold = -np.sin(np.radians(floor_angle_deg))
    wall_idx  = np.array(list(wall_faces), dtype=np.intp)
    if len(wall_idx) == 0:
        return

    wall_nz  = mesh.face_normals[wall_idx, 2]
    angle_ok = wall_nz >= threshold

    face_verts    = mesh.vertices[mesh.faces[wall_idx]]   # (N, 3, 3)
    face_centroids = face_verts.mean(axis=1)              # (N, 3)

    # ── Direct-grounding check at the face centroid ──────────────────────────
    # The root ring is always embedded INSIDE the closed support mesh, so
    # querying the lowest vertex (the root) trivially returns "inside" for
    # every face — even cantilevered ones.  The face centroid correctly
    # distinguishes faces that are genuinely adjacent to the mesh surface from
    # those that only have their root vertex embedded deep inside it.
    inside       = support_mesh.contains(face_centroids)
    _, surf_dist, _ = support_mesh.nearest.on_surface(face_centroids)
    inside_or_on = inside | (surf_dist <= support_tolerance_mm)

    has_support  = inside_or_on.copy()
    outside_idx  = np.where(~inside_or_on)[0]
    if len(outside_idx):
        # Cast a downward ray from each non-grounded face centroid.
        # Only do this for centroids that are OUTSIDE the mesh; centroids
        # already inside the mesh are handled by the `inside` check above
        # (a downward ray from inside a closed mesh trivially exits and would
        # always return True, which is wrong for deeply-embedded centroids).
        ray_origins = face_centroids[outside_idx] - np.array([0.0, 0.0, 1e-3])
        ray_dirs    = np.tile([0.0, 0.0, -1.0], (len(outside_idx), 1))
        has_support[outside_idx] = support_mesh.ray.intersects_any(
            ray_origins, ray_dirs,
        )

    # ── Build wall-face neighbor graph ───────────────────────────────────────
    edge_to_faces: dict[tuple[int, int], list[int]] = {}
    for local_i, face in enumerate(mesh.faces[wall_idx]):
        for a, b in ((face[0], face[1]), (face[1], face[2]), (face[2], face[0])):
            edge_to_faces.setdefault(tuple(sorted((int(a), int(b)))), []).append(local_i)

    neighbors: list[set[int]] = [set() for _ in wall_idx]
    for incident in edge_to_faces.values():
        for fi in incident:
            neighbors[fi].update(j for j in incident if j != fi)

    # ── BFS: propagate printability from grounded seeds ──────────────────────
    # Directly grounded faces (has_support=True) seed the BFS regardless of
    # their own angle — if the mesh is right there, the face is supported.
    #
    # BFS is unconstrained in direction but is Z-floored: a neighbour is only
    # visited if its centroid Z ≥ the minimum Z of any directly-grounded face.
    # This models FDM layer-by-layer printing correctly for leaf arches:
    #
    # • The wall face strip winds around the leaf perimeter (not monotone in Z),
    #   so upward-only BFS breaks completely — faces adjacent in the strip can
    #   be at very different Z levels.
    #
    # • Pure unconstrained BFS incorrectly propagates from a grounded face
    #   downward into arch faces that hang below the contact point (no material
    #   below them in FDM) — the lower-quarter leaf bug.
    #
    # • Z-flooring prevents propagation below the lowest attachment point while
    #   still letting BFS traverse the arch freely in both directions above it.
    min_z_seed = (
        float(face_centroids[has_support, 2].min()) if has_support.any() else -np.inf
    )
    printable = has_support.copy()
    queue: deque[int] = deque(int(i) for i in np.where(printable)[0])
    while queue:
        fi = queue.popleft()
        for nb in neighbors[fi]:
            if (
                not printable[nb]
                and angle_ok[nb]
                and face_centroids[nb, 2] >= min_z_seed - 1e-4
            ):
                printable[nb] = True
                queue.append(nb)

    # ── Z-support pass: catch faces isolated in the face graph ───────────────
    # A face angle check (angle_ok) applies to unsupported spans — when the
    # nozzle must bridge from one support to another.  A face that has printable
    # material directly below it in Z (within _LEAF_Z_SUPPORT_XY_MM in XY) is
    # directly stacked: it always prints regardless of its normal angle.
    #
    # This pass marks any non-printable face as printable if a printable face
    # lies strictly below it in Z and within the XY threshold, REGARDLESS of
    # angle_ok.  The Z floor (min_z_seed) still prevents propagation below the
    # lowest attachment point.  Repeats until stable so a chain resolves fully.
    eligible = (~printable) & (face_centroids[:, 2] >= min_z_seed - 1e-4)
    changed = eligible.any()
    while changed:
        changed = False
        for i in np.where(eligible)[0]:
            below_printable = printable & (face_centroids[:, 2] < face_centroids[i, 2])
            if below_printable.any():
                xy_dist = np.linalg.norm(
                    face_centroids[below_printable, :2] - face_centroids[i, :2], axis=1
                )
                if xy_dist.min() <= _LEAF_Z_SUPPORT_XY_MM:
                    printable[i] = True
                    eligible[i]  = False
                    changed = True

    mesh.visual.face_colors[wall_idx[ printable]] = color_ok
    mesh.visual.face_colors[wall_idx[~printable]] = color_fail


# ══════════════════════════════════════════════════════════════════════════════
# 5-color diagnostic mode
# ══════════════════════════════════════════════════════════════════════════════

#: Category codes returned by :func:`color_leaf_walls_diagnostic`.
DIAG_SEED_OK   = 0  # centroid grounded + angle_ok                  → lime
DIAG_SEED_OVER = 1  # centroid grounded + NOT angle_ok              → purple
DIAG_BFS       = 2  # not grounded, reachable via BFS, angle_ok     → green
DIAG_ISOLATED  = 3  # angle_ok but unreachable from support          → orange
DIAG_FAIL      = 4  # NOT angle_ok AND not grounded                 → red

#: Human-readable names for each :data:`DIAG_*` constant.
DIAG_NAMES: tuple[str, ...] = (
    "SEED_OK",    # 0
    "SEED_OVER",  # 1
    "BFS",        # 2
    "ISOLATED",   # 3
    "FAIL",       # 4
)

#: RGBA colors for the 5 diagnostic categories (indexed by DIAG_* constants).
_DIAG_RGBA: np.ndarray = np.array([
    (130, 220,  60, 255),   # lime     — SEED_OK   (grounded + printable)
    (200,  70, 230, 255),   # purple   — SEED_OVER (grounded + bad angle)
    ( 50, 210,  50, 255),   # green    — BFS       (propagated from grounded)
    (255, 170,  30, 255),   # orange   — ISOLATED  (angle_ok but cut off)
    (220,  50,  50, 255),   # red      — FAIL      (definite overhang)
], dtype=np.uint8)


def color_leaf_walls_diagnostic(
    mesh:         trimesh.Trimesh,
    wall_faces:   range,
    support_mesh: trimesh.Trimesh,
    *,
    floor_angle_deg:      float = _LEAF_FDM_FLOOR_DEG,
    support_tolerance_mm: float = _LEAF_FDM_SUPPORT_TOLERANCE_MM,
) -> dict:
    """5-color diagnostic colouring — apply to *mesh* and return face-level data.

    Unlike :func:`color_leaf_walls_by_fdm` (which uses only green/red), this
    function paints wall faces with five distinct colors so you can see exactly
    why each face was classified:

    =========  =======  =========================================================
    Category   Color    Meaning
    =========  =======  =========================================================
    SEED_OK    lime     Centroid inside/on/below support mesh; angle ≤ floor.
                        Directly printable: the sphere is right there.
    SEED_OVER  purple   Centroid inside/on/below support mesh; angle > floor.
                        On the mesh but facing wrong way — investigate geometry.
    BFS        green    Not grounded, but reachable via BFS chain from grounded.
                        Printable by connectivity from the sphere.
    ISOLATED   orange   Angle_ok=True but cannot be reached from support through
                        an angle-OK face chain.  Blocked by overhang barrier.
                        Currently treated as unprintable — may be wrong.
    FAIL       red      Angle > floor AND not grounded.  Definite overhang.
    =========  =======  =========================================================

    Parameters
    ----------
    mesh                 : Solid returned by ``solidify_leaf``.
    wall_faces           : Range of wall face indices from ``solidify_leaf``.
    support_mesh         : Mesh the leaf rests on.
    floor_angle_deg      : FDM floor angle (default: ``_LEAF_FDM_FLOOR_DEG``).
    support_tolerance_mm : On-surface tolerance at centroid.

    Returns
    -------
    dict
        ``'categories'`` : ``np.ndarray`` shape (N,) int8 — one of :data:`DIAG_*`
        ``'angle_ok'``   : ``np.ndarray`` shape (N,) bool
        ``'grounded'``   : ``np.ndarray`` shape (N,) bool  (direct contact before BFS)
        ``'centroids'``  : ``np.ndarray`` shape (N,3)
        ``'nz'``         : ``np.ndarray`` shape (N,)  — face normal Z components
        ``'wall_idx'``   : ``np.ndarray`` shape (N,) int — global face indices
    """
    threshold = -np.sin(np.radians(floor_angle_deg))
    wall_idx  = np.array(list(wall_faces), dtype=np.intp)
    N = len(wall_idx)
    if N == 0:
        return {
            'categories': np.empty(0, dtype=np.int8),
            'angle_ok':   np.empty(0, dtype=bool),
            'grounded':   np.empty(0, dtype=bool),
            'centroids':  np.empty((0, 3)),
            'nz':         np.empty(0),
            'wall_idx':   wall_idx,
        }

    nz       = mesh.face_normals[wall_idx, 2]
    angle_ok = nz >= threshold

    face_verts = mesh.vertices[mesh.faces[wall_idx]]   # (N, 3, 3)
    centroids  = face_verts.mean(axis=1)               # (N, 3)

    # ── Direct grounding: centroid inside / on surface / mesh directly below ──
    inside       = support_mesh.contains(centroids)
    _, surf_dist, _ = support_mesh.nearest.on_surface(centroids)
    inside_or_on = inside | (surf_dist <= support_tolerance_mm)

    grounded    = inside_or_on.copy()
    outside_idx = np.where(~inside_or_on)[0]
    if len(outside_idx):
        ray_origins = centroids[outside_idx] - np.array([0.0, 0.0, 1e-3])
        ray_dirs    = np.tile([0.0, 0.0, -1.0], (len(outside_idx), 1))
        grounded[outside_idx] = support_mesh.ray.intersects_any(ray_origins, ray_dirs)

    # ── Neighbor graph ────────────────────────────────────────────────────────
    edge_to_faces: dict[tuple[int, int], list[int]] = {}
    for local_i, face in enumerate(mesh.faces[wall_idx]):
        for a, b in ((face[0], face[1]), (face[1], face[2]), (face[2], face[0])):
            edge_to_faces.setdefault(tuple(sorted((int(a), int(b)))), []).append(local_i)
    neighbors: list[set[int]] = [set() for _ in range(N)]
    for incident in edge_to_faces.values():
        for fi in incident:
            neighbors[fi].update(j for j in incident if j != fi)

    # ── BFS from grounded seeds (Z-floored, direction-unconstrained) ──────────
    # See color_leaf_walls_by_fdm for a full explanation of the Z-floor approach.
    # BFS visits any angle_ok neighbour whose centroid Z ≥ the lowest grounded
    # face's centroid Z.  This prevents downward propagation into arch faces
    # below the attachment point while still allowing free traversal of the arch.
    min_z_seed = float(centroids[grounded, 2].min()) if grounded.any() else -np.inf
    bfs_reached = grounded.copy()
    queue: deque[int] = deque(int(i) for i in np.where(grounded)[0])
    while queue:
        fi = queue.popleft()
        for nb in neighbors[fi]:
            if (
                not bfs_reached[nb]
                and angle_ok[nb]
                and centroids[nb, 2] >= min_z_seed - 1e-4
            ):
                bfs_reached[nb] = True
                queue.append(nb)

    # ── Z-support pass: recover faces isolated in the face graph ─────────────
    # Mirror of the same pass in color_leaf_walls_by_fdm (see detailed comment
    # there).  angle_ok is NOT required — direct Z-support overrides the angle
    # constraint.  Here bfs_reached is the working "printable" set so newly
    # recovered faces are classified as BFS in the category assignment below.
    z_eligible = (~bfs_reached) & (centroids[:, 2] >= min_z_seed - 1e-4)
    changed = z_eligible.any()
    while changed:
        changed = False
        for i in np.where(z_eligible)[0]:
            below_ok = bfs_reached & (centroids[:, 2] < centroids[i, 2])
            if below_ok.any():
                xy_dist = np.linalg.norm(
                    centroids[below_ok, :2] - centroids[i, :2], axis=1
                )
                if xy_dist.min() <= _LEAF_Z_SUPPORT_XY_MM:
                    bfs_reached[i] = True
                    z_eligible[i]  = False
                    changed = True

    # ── Assign category codes ─────────────────────────────────────────────────
    # Note: faces recovered by the Z-support pass are classified as BFS since
    # they are printable by Z-support (a generalisation of BFS connectivity).
    cats = np.full(N, DIAG_FAIL, dtype=np.int8)
    for i in range(N):
        if grounded[i]:
            cats[i] = DIAG_SEED_OK if angle_ok[i] else DIAG_SEED_OVER
        elif bfs_reached[i]:
            cats[i] = DIAG_BFS
        elif angle_ok[i]:
            cats[i] = DIAG_ISOLATED
        # else: DIAG_FAIL (already set)

    # ── Paint faces in-place ──────────────────────────────────────────────────
    for cat_id in range(len(DIAG_NAMES)):
        mask = cats == cat_id
        if mask.any():
            mesh.visual.face_colors[wall_idx[mask]] = _DIAG_RGBA[cat_id]

    return {
        'categories': cats,
        'angle_ok':   angle_ok,
        'grounded':   grounded,
        'centroids':  centroids,
        'nz':         nz,
        'wall_idx':   wall_idx,
    }


def print_leaf_diagnostic(name: str, info: dict) -> None:
    """Print a per-face diagnostic table for one leaf, sorted by centroid Z.

    Parameters
    ----------
    name : Label for this leaf (e.g. "equator", "lower-quarter").
    info : Dict returned by :func:`color_leaf_walls_diagnostic`.
    """
    cats      = info['categories']
    angle_ok  = info['angle_ok']
    grounded  = info['grounded']
    centroids = info['centroids']
    nz        = info['nz']
    N = len(cats)

    order = np.argsort(centroids[:, 2])

    # Category color symbols for quick scanning
    _sym = ("🟢", "🟣", "💚", "🟠", "🔴")

    print(f"\n── {name} ({N} wall faces, floor={_LEAF_FDM_FLOOR_DEG}°) "
          + "─" * max(0, 50 - len(name)))
    print(f"  {'fi':>4}  {'cx':>7}  {'cy':>7}  {'cz':>7}  {'nz':>7}  "
          f"{'aOK':>4}  {'gnd':>4}  category")
    print(f"  {'─'*4}  {'─'*7}  {'─'*7}  {'─'*7}  {'─'*7}  "
          f"{'─'*4}  {'─'*4}  ──────────────")
    for fi in order:
        cx, cy, cz = centroids[fi]
        sym = _sym[cats[fi]]
        print(
            f"  {fi:>4}  {cx:>7.2f}  {cy:>7.2f}  {cz:>7.2f}  {nz[fi]:>7.3f}  "
            f"{'T' if angle_ok[fi] else 'F':>4}  {'T' if grounded[fi] else 'F':>4}  "
            f"{sym} {DIAG_NAMES[cats[fi]]}"
        )

    # Summary counts
    print()
    for cat_id, cat_name in enumerate(DIAG_NAMES):
        count = int((cats == cat_id).sum())
        if count:
            print(f"  {_sym[cat_id]} {cat_name}: {count}")
    print()
