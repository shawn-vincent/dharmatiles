"""Branchlet growth algorithm.

A branchlet connects a branch-surface attachment point to a leaf.  The leaf
lies FLAT at the branchlet tip, perpendicular to the growth direction — the
branchlet tip cross-section IS the leaf outline.

Geometry
--------
The branchlet is a linear loft:

  root ring  →  leaf perimeter
  (circle)      (flat teardrop in the tip cross-section plane)

Both rings lie in planes perpendicular to the growth direction (exit_dir),
separated by ``branchlet_length_mm`` along that axis.  The loft is a short
frustum that fans from a chunky circle at the branch surface to the full leaf
outline at the tip.  The leaf blade caps the open tip end as a separate
overlapping closed shell; the slicer fuses them.

Leaf orientation
----------------
The leaf lies FLAT at the branchlet tip:

  N = exit_dir        (leaf top faces the growth direction ≈ upward)
  L = world_up projected onto the exit plane (leaf long axis)
  T = cross(exit_dir, L) (leaf lateral axis)

L is derived by projecting world_up onto the plane perpendicular to exit_dir.
This guarantees L.z = cos(θ) ≥ 0 for any exit elevation θ ≥ 0°, so the leaf
tip is always above the attachment point and every loft wall face stays within
the FDM printability cone.  (Using the surface normal for L gives L.z < 0
for downward-facing surfaces, producing catastrophic overhang failures.)

The ``yaw_deg`` parameter rotates L around exit_dir for per-leaf variation.
Safe yaw range: |yaw| ≤ ~20°.  Beyond that, T_yaw.z = −sin(yaw)·cos(θ)
grows large enough that ring edges on the left side of the leaf perimeter
descend in world-z, violating the FDM floor-angle rule.

Because L ⊥ exit_dir and T ⊥ exit_dir, every leaf-perimeter vertex lies in
the plane through p1 perpendicular to exit_dir (modulo the tiny fold-height
term, < 0.1 mm).  The loft cross-sections are then nearly parallel discs and
the structure is a compact cone shape rather than a sail.

Exit-direction logic (one conditional — see
docs/design/tree-branchlet-growth-algorithm.md): if the surface normal
elevation is already above the floor angle plus a construction margin, exit
along the normal; otherwise tilt the horizontal component to that steeper
angle.  The loft is still validated against the requested floor angle.
"""
from __future__ import annotations

import warnings

import numpy as np
import trimesh

from .leaf import (
    build_leaf_mesh,
    _LEAF_N_LONG,
    _LEAF_CREASE_SHARPNESS,
    _LEAF_LONG_T_PEAK,
    _leaf_width_profile,
)
from ._utils import _safe_norm, _hash01

# ── Constants ──────────────────────────────────────────────────────────────────

# Leaf-perimeter vertex count: v_base + (N_S-1) right-edge + v_tip + (N_S-1) left-edge.
_N_PERIM      = 2 + 2 * (_LEAF_N_LONG - 1)   # = 24
_N_LOFT_RINGS = 8                              # intermediate cross-sections in the loft
_EXIT_MARGIN_DEG = 15.0                        # construction margin above the FDM floor


# ── Helpers ────────────────────────────────────────────────────────────────────

def _basis(w: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
    """Two orthonormal vectors perpendicular to w (Bishop-frame seed)."""
    ref = np.array([0.0, 0.0, 1.0])
    if abs(float(np.dot(w, ref))) > 0.9:
        ref = np.array([1.0, 0.0, 0.0])
    u = np.cross(w, ref)
    u /= float(np.linalg.norm(u)) + 1e-12
    v = np.cross(w, u)
    v /= float(np.linalg.norm(v)) + 1e-12
    return u, v


def _exit_direction(
    surface_normal: np.ndarray,
    floor_angle_rad: float,
    fallback_hint: np.ndarray,
) -> np.ndarray:
    """Safe exit direction (single conditional).

    Uses surface_normal directly when its elevation is already at or above the
    FDM floor angle; otherwise tilts the horizontal component up to exactly
    the floor angle.
    """
    n0               = _safe_norm(surface_normal)
    normal_elevation = float(np.arcsin(np.clip(n0[2], -1.0, 1.0)))

    if normal_elevation >= floor_angle_rad - 1e-6:
        return n0

    n0_h  = np.array([n0[0], n0[1], 0.0])
    h_len = float(np.linalg.norm(n0_h))
    if h_len < 1e-4:
        hint_h     = np.array([fallback_hint[0], fallback_hint[1], 0.0])
        hint_h_len = float(np.linalg.norm(hint_h))
        n0_h = hint_h / hint_h_len if hint_h_len > 1e-4 else np.array([1.0, 0.0, 0.0])
    else:
        n0_h = n0_h / h_len

    world_up = np.array([0.0, 0.0, 1.0])
    return (n0_h * float(np.cos(floor_angle_rad))
            + world_up * float(np.sin(floor_angle_rad)))


def _leaf_perimeter_verts(
    base_pos: np.ndarray,
    L: np.ndarray,
    T: np.ndarray,
    N: np.ndarray,
    length_mm: float,
    width_mm: float,
    fold_angle_deg: float,
) -> np.ndarray:
    """Outer edge loop of the leaf blade in world space.

    At the leaf's lateral edges (t = ±1) the lobe height is zero
    (sin(π·|t|) = 0), so top and bottom surfaces converge to a single curve —
    a clean perimeter with no thickness gap.

    When L ⊥ N (i.e. the leaf lies perpendicular to N), all edge points live
    in the plane through base_pos perpendicular to N, plus the tiny fold-height
    offset (< 0.1 mm for the default 3° fold angle).

    Returns
    -------
    (N_PERIM, 3) in order:
      j = 0               v_base
      j = 1 .. n_rings    right edge, s increasing
      j = n_rings+1       v_tip
      j = n_rings+2 .. N_PERIM-1  left edge, s decreasing
    """
    n_rings     = _LEAF_N_LONG - 1                                   # 11
    s_int       = np.linspace(0.0, 1.0, _LEAF_N_LONG + 1)[1:-1]    # (11,)
    w_s         = width_mm * _leaf_width_profile(s_int)              # (11,)
    long_t      = (s_int ** 0.5 * (1.0 - s_int) ** 1.5) / _LEAF_LONG_T_PEAK  # (11,)
    fold_tan    = float(np.tan(np.radians(fold_angle_deg)))
    fold_h_edge = float(np.tanh(_LEAF_CREASE_SHARPNESS)) * w_s * fold_tan * long_t  # (11,)

    midribs    = base_pos[None] + (s_int[:, None] * length_mm) * L[None]  # (11, 3)
    right_edge = midribs + w_s[:, None] * T[None] + fold_h_edge[:, None] * N[None]
    left_edge  = midribs - w_s[:, None] * T[None] + fold_h_edge[:, None] * N[None]

    return np.concatenate([
        base_pos[None],                      # j = 0
        right_edge,                           # j = 1 .. 11
        (base_pos + length_mm * L)[None],    # j = 12
        left_edge[::-1],                     # j = 13 .. 23  (s decreasing)
    ], axis=0)                               # (24, 3)


def _root_ring_verts(
    center: np.ndarray,
    exit_dir: np.ndarray,
    L_leaf: np.ndarray,
    T_leaf: np.ndarray,
    radius_mm: float,
    n: int,
) -> np.ndarray:
    """Circle of ``n`` vertices in the plane perpendicular to exit_dir.

    Vertex ordering matches ``_leaf_perimeter_verts``:
      j = 0     → −L direction (leaf-base side)
      j = n//4  → +T direction (right-edge side)
      j = n//2  → +L direction (leaf-tip side)
      j = 3n//4 → −T direction (left-edge side)
    """
    # Since L_leaf and T_leaf are both already ⊥ exit_dir in the new design,
    # the projection step is a no-op; we keep it for robustness.
    def _proj(v: np.ndarray) -> np.ndarray | None:
        vp  = v - float(np.dot(v, exit_dir)) * exit_dir
        nv  = float(np.linalg.norm(vp))
        return vp / nv if nv > 1e-4 else None

    L_p = _proj(L_leaf)
    T_p = _proj(T_leaf)

    if L_p is None and T_p is not None:
        L_p = np.cross(exit_dir, T_p)
        L_p /= max(float(np.linalg.norm(L_p)), 1e-10)
    elif L_p is None:
        ref = np.array([1.0, 0.0, 0.0])
        if abs(float(np.dot(exit_dir, ref))) > 0.9:
            ref = np.array([0.0, 1.0, 0.0])
        L_p = np.cross(exit_dir, ref)
        L_p /= max(float(np.linalg.norm(L_p)), 1e-10)
    if T_p is None:
        T_p = np.cross(exit_dir, L_p)
        T_p /= max(float(np.linalg.norm(T_p)), 1e-10)

    # j=0 → −π/2 → −L_p (base side)
    # j=n//2 → +π/2 → +L_p (tip side)
    j_arr  = np.arange(n, dtype=float)
    angles = -np.pi / 2.0 + 2.0 * np.pi * j_arr / n

    return center[None] + radius_mm * (
        np.cos(angles[:, None]) * T_p[None]
        + np.sin(angles[:, None]) * L_p[None]
    )  # (n, 3)


# ── Validation ─────────────────────────────────────────────────────────────────

def _validate_branchlet_embedding(
    root_ring: np.ndarray,
    parent_mesh: trimesh.Trimesh,
    embed_depth_mm: float,
    root_radius_mm: float,
    label: str = "",
) -> None:
    """Raise RuntimeError if any root ring vertex lies outside the parent mesh.

    All root ring vertices must be inside the foliage cluster (parent) mesh for
    the branchlet to be self-supporting.  If any vertex is outside, that part of
    the base hangs in air and the FDM printability assumptions encoded in
    ``_validate_branchlet_fdm`` are violated — the root-cap faces are excluded
    from the overhang check precisely *because* they are assumed to be buried
    inside the cluster.  A floating base breaks that assumption and creates an
    unsupported overhang.

    The check uses ``parent_mesh.contains()`` (ray-cast based) which requires
    the parent mesh to be watertight; the foliage cluster icosphere satisfies
    this after ``fix_normals()``.
    """
    pfx = f"Branchlet ({label}) " if label else "Branchlet "
    inside = parent_mesh.contains(root_ring)   # (N_PERIM,) bool
    n_outside = int((~inside).sum())
    if n_outside == 0:
        return

    # Report the worst offender by proximity to the nearest surface point.
    outside_pts = root_ring[~inside]
    closest, dist, _ = trimesh.proximity.closest_point(parent_mesh, outside_pts)
    worst_dist = float(dist.max())
    raise RuntimeError(
        f"{pfx}root ring embedding failure: {n_outside}/{len(root_ring)} "
        "root-ring vertex/vertices lie outside the parent (foliage cluster) mesh. "
        "The branchlet base is not fully embedded — the root-cap faces are NOT "
        "buried inside the cluster, so the FDM overhang assumptions are violated. "
        f"Worst vertex is {worst_dist:.3f} mm outside the cluster surface. "
        f"(embed_depth_mm={embed_depth_mm:.3f}, root_radius_mm={root_radius_mm:.3f}) "
        "Fix: increase branchlet_embed_depth_mm or decrease branchlet_root_radius_mm."
    )


def _validate_branchlet_fdm(
    loft: trimesh.Trimesh,
    floor_angle_rad: float,
    n_wall_faces: int,
    n_root_cap_faces: int,
    label: str = "",
    embed_depth_mm: float = 0.0,
    branchlet_length_mm: float = 1.0,
    parent_mesh: "trimesh.Trimesh | None" = None,
) -> None:
    """Raise RuntimeError if the branchlet loft violates FDM printability.

    Two checks:

    1. **Watertight** — the loft mesh must be a closed manifold.  An open mesh
       cannot be sliced correctly.

    2. **Face-normal FDM rule** — every *exterior* face normal must satisfy
       ``n.z ≥ −cos(floor_angle_rad)``.  Faces that are embedded inside the
       parent (foliage cluster) mesh are excluded from this check because they
       are supported by the cluster and never appear as printed overhangs.

       When ``parent_mesh`` is supplied, embedded wall faces are determined
       precisely: each wall face centroid is tested with ``parent_mesh.contains()``
       and faces whose centroid lies inside the cluster are excluded.  This is
       exact and handles the general case where the embedding direction
       (−surface_normal) diverges from the loft exit axis (exit_dir).

       When ``parent_mesh`` is None, a fraction-based fallback is used: the loft
       interpolates from the root (deeply buried) to the tip (outside the cluster)
       and rings with parameter ``t ≤ embed_depth / (embed_depth + branchlet_length)``
       are treated as embedded.

       Root cap faces are always excluded (they close the buried end).
       Tip-cap faces and un-embedded wall faces are always checked.

    Face layout produced by ``build_branchlet_and_leaf``:

    +-----------------------+-------------------+------------------+
    | wall faces            | root cap faces    | tip cap faces    |
    | [0 : n_wall]          | [n_wall :         | [n_wall +        |
    |                       |  n_wall+n_root]   |  n_root : end]   |
    +-----------------------+-------------------+------------------+
    """
    pfx = f"Branchlet ({label}) " if label else "Branchlet "

    # ── Check 1: watertight ───────────────────────────────────────────────────
    if not loft.is_watertight:
        raise RuntimeError(
            f"{pfx}loft is not watertight "
            f"({len(loft.vertices)} vertices, {len(loft.faces)} faces). "
            "Check for degenerate faces or duplicate vertices."
        )

    # ── Check 2: FDM face-normal rule ─────────────────────────────────────────
    n_faces      = len(loft.faces)
    n_tip_cap    = n_faces - n_wall_faces - n_root_cap_faces

    # ── Determine which wall faces are embedded inside the cluster ────────────
    ext_mask = np.zeros(n_faces, dtype=bool)

    if parent_mesh is not None:
        # Precise path: test each wall face centroid against the parent mesh.
        # A face whose centroid is inside the cluster is supported by cluster
        # material and excluded from the FDM check.
        wall_face_indices = np.arange(n_wall_faces)
        wall_centroids = loft.triangles_center[wall_face_indices]   # (N_wall, 3)
        inside_cluster = parent_mesh.contains(wall_centroids)        # (N_wall,) bool
        # Mark wall faces NOT inside the cluster as exterior
        ext_mask[:n_wall_faces] = ~inside_cluster
    else:
        # Fallback: fraction-based estimation when no parent mesh is available.
        # The loft has (NR − 1) transitions; each spans n_root_cap_faces×2 triangles.
        faces_per_trans  = n_root_cap_faces * 2
        n_transitions    = n_wall_faces // max(faces_per_trans, 1)
        total_loft       = float(embed_depth_mm) + max(float(branchlet_length_mm), 1e-9)
        embed_frac       = float(embed_depth_mm) / total_loft
        n_skip_trans     = max(0, min(n_transitions,
                                      int(np.floor(embed_frac * n_transitions))))
        n_skip_wall      = n_skip_trans * faces_per_trans
        ext_mask[n_skip_wall:n_wall_faces] = True

    # Tip cap is always exterior (root cap always excluded)
    ext_mask[n_wall_faces + n_root_cap_faces:
             n_wall_faces + n_root_cap_faces + n_tip_cap] = True      # tip cap

    normals   = loft.face_normals                                     # (F, 3)
    threshold = -float(np.cos(floor_angle_rad))
    ext_z     = normals[ext_mask, 2]
    bad       = ext_z < threshold - 1e-6

    if bad.any():
        worst_z       = float(ext_z[bad].min())
        worst_elev    = float(np.degrees(np.arcsin(np.clip(worst_z, -1.0, 1.0))))
        floor_deg     = float(np.degrees(floor_angle_rad))
        n_bad         = int(bad.sum())
        n_ext         = int(ext_mask.sum())
        raise RuntimeError(
            f"{pfx}FDM failure: {n_bad}/{n_ext} exterior face(s) violate the "
            f"floor-angle rule (floor={floor_deg:.0f}°, threshold n.z ≥ {threshold:.3f}). "
            f"Worst face: n.z={worst_z:.3f} (elevation {worst_elev:.1f}°). "
            "The branchlet loft has faces pointing too far downward to print without "
            "supports. Reduce leaf_angle_jitter_deg (safe range: |yaw| ≤ ~20°), "
            "reduce root_radius_mm, increase branchlet_length_mm, or "
            "increase floor_angle_deg to fix."
        )


# ── Public API ─────────────────────────────────────────────────────────────────

def build_branchlet_and_leaf(
    *,
    attachment_point: np.ndarray,
    surface_normal: np.ndarray,
    branchlet_length_mm: float = 3.0,
    floor_angle_deg: float = 45.0,
    root_radius_mm: float = 2.0,
    embed_depth_mm: float = 2.5,
    yaw_deg: float = 0.0,
    seed: int = 0,
    leaf_length_mm: float = 8.0,
    leaf_width_mm: float = 5.0,
    leaf_thickness_mm: float = 0.24,
    leaf_fold_angle_deg: float = 3.0,
    leaf_keel_depth_mm: float = 0.0,
    parent_mesh: "trimesh.Trimesh | None" = None,
) -> list[trimesh.Trimesh]:
    """Grow a branchlet from a surface point and place a flat leaf at the tip.

    The leaf lies FLAT at the branchlet tip — its top surface faces the growth
    direction (exit_dir ≈ world-up), and its long axis (L) lies in the plane
    perpendicular to exit_dir.  The branchlet is a compact frustum that fans
    from a chunky circle at the branch surface to the full leaf outline.

    Parameters
    ----------
    attachment_point
        World-space point on the branch / foliage surface.
    surface_normal
        Outward normal at ``attachment_point``; drives both the exit direction
        and the natural leaf long-axis direction.
    branchlet_length_mm
        Distance from attachment point to the leaf base (branchlet tip).
    floor_angle_deg
        FDM minimum printable elevation above horizontal.  Default 45°.
    root_radius_mm
        Radius of the root circle.  Chunky by design — much larger than the
        old tube radius — so the branchlet provides real structural support.
    embed_depth_mm
        How far the root end is pushed into the branch for slicer fusion.
        Embedding is along the inward surface normal (``−surface_normal``), NOT
        along ``−exit_dir``.  This guarantees the root center moves radially
        into the cluster regardless of how much exit_dir is tilted by the FDM
        floor constraint.  For a spherical cluster of radius R and root ring
        radius r, the minimum embed depth that keeps the entire ring inside is
        ``R − sqrt(R² − r²)`` (≈ r²/2R for small r).  The default (2.0 mm) is
        conservative enough for clusters ≥ 5 mm radius and root rings ≤ 2 mm.
    yaw_deg
        Spin of the leaf around the exit_dir (growth) axis.
    seed
        Integer seed passed to ``build_leaf_mesh`` for roll variation.
    leaf_length_mm, leaf_width_mm, leaf_thickness_mm, leaf_fold_angle_deg
        Leaf blade geometry — forwarded to ``build_leaf_mesh``.
    leaf_keel_depth_mm
        Keel depth.  Default 0 — the branchlet body provides support.

    Returns
    -------
    list[trimesh.Trimesh]
        Branchlet loft mesh followed by leaf parts.
    """
    p0        = np.asarray(attachment_point, float)
    n0        = _safe_norm(np.asarray(surface_normal, float))
    world_up  = np.array([0.0, 0.0, 1.0])
    floor_rad = float(np.radians(floor_angle_deg))

    # ── Step 1: Exit direction (single conditional) ───────────────────────────
    # Build above the exact printability boundary so leaf width, root embedding,
    # and triangulation cannot consume all available overhang margin.
    exit_floor_rad = min(
        float(np.pi / 2.0),
        floor_rad + float(np.radians(_EXIT_MARGIN_DEG)),
    )
    exit_dir = _safe_norm(_exit_direction(n0, exit_floor_rad, world_up))

    # ── Step 2: Branchlet tip — straight along exit_dir ──────────────────────
    p1 = p0 + float(branchlet_length_mm) * exit_dir

    # ── Step 3: Leaf frame — flat at the tip, perpendicular to exit_dir ──────
    #
    # N = exit_dir   (leaf top faces the growth direction ≈ upward)
    # L = world_up projected onto the exit plane (leaf long axis)
    # T = cross(exit_dir, L)
    #
    # Using world_up projected onto the exit plane (rather than the surface
    # normal) guarantees that the leaf tip is always ABOVE the attachment point
    # in world-z, keeping every loft wall face within the FDM printability cone.
    #
    # Analytically: L = (-sin(θ)·h_hat, cos(θ)) for exit_dir elevation θ.
    # At θ=45° the L direction has z=0.707, so the leaf tip is well above the
    # root ring at any reasonable leaf length.  The surface normal is used only
    # to choose the *horizontal* half-plane (h_hat), not to tilt L downward.
    #
    # Falls back to a Bishop-frame direction when exit_dir is nearly vertical
    # (top-of-cluster leaves), which is the same as the old code.
    N = exit_dir

    wup_in_plane     = world_up - float(np.dot(world_up, exit_dir)) * exit_dir
    wup_in_plane_len = float(np.linalg.norm(wup_in_plane))
    if wup_in_plane_len < 0.1:
        # exit_dir is nearly vertical (top of cluster) → arbitrary consistent axis.
        leaf_base_dir, leaf_lat_dir = _basis(exit_dir)
    else:
        leaf_base_dir = wup_in_plane / wup_in_plane_len
        raw           = np.cross(exit_dir, leaf_base_dir)
        leaf_lat_dir  = raw / (float(np.linalg.norm(raw)) + 1e-10)

    yaw_rad = float(np.radians(yaw_deg))
    L = np.cos(yaw_rad) * leaf_base_dir + np.sin(yaw_rad) * leaf_lat_dir
    T = np.cross(exit_dir, L)
    T /= max(float(np.linalg.norm(T)), 1e-10)

    # ── Step 4: Leaf perimeter — flat teardrop in the tip cross-section ───────
    # Because L ⊥ exit_dir and T ⊥ exit_dir, all perimeter vertices lie in the
    # plane through p1 perpendicular to exit_dir (modulo tiny fold offsets).
    leaf_perim = _leaf_perimeter_verts(
        p1, L, T, N,
        length_mm=float(leaf_length_mm),
        width_mm=float(leaf_width_mm),
        fold_angle_deg=float(leaf_fold_angle_deg),
    )  # (_N_PERIM, 3) = (24, 3)

    # ── Step 5: Root ring — chunky circle at the embedded root ───────────────
    # Embed along the INWARD surface normal (not -exit_dir).  exit_dir is
    # tilted upward by the FDM floor constraint, so using -exit_dir can place
    # root_center laterally outside the cluster on side/bottom attachment points.
    # -n0 is the true inward radial direction: it always moves root_center toward
    # the cluster interior regardless of exit_dir orientation.
    root_center = p0 - float(embed_depth_mm) * n0
    root_ring   = _root_ring_verts(
        root_center, exit_dir, L, T,
        radius_mm=float(root_radius_mm),
        n=_N_PERIM,
    )  # (24, 3)

    # ── Step 5b: Embedding check ──────────────────────────────────────────────
    # All root ring vertices must be inside the parent (foliage cluster) mesh.
    # If any are outside, the root-cap faces are NOT buried and the FDM
    # overhang assumptions used by _validate_branchlet_fdm break down.
    if parent_mesh is not None:
        _validate_branchlet_embedding(
            root_ring,
            parent_mesh,
            embed_depth_mm=float(embed_depth_mm),
            root_radius_mm=float(root_radius_mm),
        )

    # ── Step 6: Linear loft ───────────────────────────────────────────────────
    # Short frustum: circle → teardrop, over (embed_depth + branchlet_length) mm.
    ts    = np.linspace(0.0, 1.0, _N_LOFT_RINGS)
    rings = (
        (1.0 - ts[:, None, None]) * root_ring[None]
        +        ts[:, None, None] * leaf_perim[None]
    )  # (_N_LOFT_RINGS, 24, 3)

    # ── Step 7: Loft mesh ─────────────────────────────────────────────────────
    NP = _N_PERIM
    NR = _N_LOFT_RINGS

    root_cap_vi = NR * NP
    tip_cap_vi  = NR * NP + 1
    verts = np.vstack([
        rings.reshape(-1, 3),
        root_center[None],
        np.mean(leaf_perim, axis=0)[None],
    ])

    faces: list[list[int]] = []

    def _vi(ring_i: int, vert_j: int) -> int:
        return ring_i * NP + (vert_j % NP)

    # Loft walls
    for i in range(NR - 1):
        for j in range(NP):
            j1 = (j + 1) % NP
            a, b = _vi(i, j),    _vi(i, j1)
            c, d = _vi(i+1, j1), _vi(i+1, j)
            faces += [[a, d, c], [a, c, b]]

    # Root cap — fan inward; closes the embedded end.
    for j in range(NP):
        j1 = (j + 1) % NP
        faces.append([root_cap_vi, _vi(0, j1), _vi(0, j)])

    # Tip cap — fan outward; closes at the leaf perimeter.
    last = NR - 1
    for j in range(NP):
        j1 = (j + 1) % NP
        faces.append([tip_cap_vi, _vi(last, j), _vi(last, j1)])

    n_wall_faces    = (NR - 1) * NP * 2   # 7 rings × 24 edges × 2 tris = 336
    n_root_cap_faces = NP                  # 24 root-cap fan tris

    loft = trimesh.Trimesh(
        vertices=verts,
        faces=np.array(faces, dtype=np.int32),
        process=False,
    )
    loft.fix_normals()
    _validate_branchlet_fdm(
        loft,
        floor_rad,
        n_wall_faces=n_wall_faces,
        n_root_cap_faces=n_root_cap_faces,
        embed_depth_mm=float(embed_depth_mm),
        branchlet_length_mm=float(branchlet_length_mm),
        parent_mesh=parent_mesh,
    )

    # ── Step 8: Leaf blade ────────────────────────────────────────────────────
    # tangent = L  (horizontal leaf long axis, in the exit plane)
    # up_hint = exit_dir  (leaf top faces the growth direction ≈ upward)
    # Together these produce the same (L, T, N) frame used for the perimeter,
    # so the leaf blade's edge loop exactly matches the loft's tip ring.
    leaf_parts = build_leaf_mesh(
        base_pos=p1,
        tangent=L,
        length_mm=float(leaf_length_mm),
        width_mm=float(leaf_width_mm),
        thickness_mm=float(leaf_thickness_mm),
        fold_angle_deg=float(leaf_fold_angle_deg),
        keel_depth_mm=float(leaf_keel_depth_mm),
        up_hint=exit_dir,
        seed=seed,
    )

    return [loft] + [p for p in leaf_parts if len(p.vertices) > 0]
