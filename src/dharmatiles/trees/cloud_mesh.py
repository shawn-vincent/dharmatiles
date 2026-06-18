"""Closed Bezier-tube mesh builder for CloudTree skeletons.

Each (parent → child) edge is a curved tube swept along a cubic Bézier path.
Cross-section rings are parallel-transported (Bishop frame) for smooth,
twist-free curves.

Architecture
------------
Every skeleton edge is meshed as its own closed solid. Non-root edges start
slightly behind their parent node, inside the incoming parent branch, so forks
have real volumetric overlap. The per-edge solids are unioned with manifold3d
before returning a single watertight wood mesh.

Foliage (``foliage_radius_mm > 0``) is handled by building a separate
icosphere-based clump mesh for the last ``leaf_clump_length_mm`` of every leaf
branch.  The clump is offset perpendicular-upward by its own ring radius
throughout, so the branch runs along the bottom surface of the clump and
protrudes below it — the branch is visible beneath the foliage across the full
clump length.
"""
from __future__ import annotations

import warnings

import numpy as np
import trimesh

from ..core.color import Material, debug_material, tag as _tag
from .bark import BarkConfig
from .leaf import build_leaf_mesh

# Fixed polygon count for every cross-section ring.
_N_SIDES = 12
# Higher ring resolution used when bark grooves are carved into a branch.
_N_BARK_SIDES = 48
# Polygon count for foliage cone/dome rings.
_N_FOLIAGE_SIDES = 48
# Latitude bands for the hemispherical dome at each leaf tip.
_N_DOME_LATS = 4
# More latitude bands on foliage domes for a rounder cap.
_N_FOLIAGE_DOME_LATS = 8


class _BarkLine:
    __slots__ = ("line_id", "phase", "theta")

    def __init__(self, line_id: int, phase: float, theta: float) -> None:
        self.line_id = int(line_id)
        self.phase = float(phase)
        self.theta = float(theta)


def build_cloud_tree_mesh(
    nodes:    np.ndarray,      # (N, 3) — root + branch pts + attractors
    parents:  np.ndarray,      # (N,) int; -1 for root
    radii:    np.ndarray,      # (N,) — bottom-up pipe-model radii
    in_dirs:  np.ndarray,      # (N, 3) — tangent *arriving* at each node
    out_dirs: np.ndarray,      # (N, 3) — tangent *leaving* parent toward node
    *,
    terrain_z: float,
    handle_scale: float = 0.45,
    strict_fdm_angle_deg: float | None = None,
    foliage_radius_mm: float = 4.0,
    leaf_clump_length_mm: float | None = None,
    bark: BarkConfig | None = None,
    bark_seed: int = 0,
    debug_attractors: np.ndarray | None = None,
    attractor_group_labels: np.ndarray | None = None,
    attractor_radius_mm: float = 0.6,
    # ── Leaf geometry ─────────────────────────────────────────────────────────
    leaf_enable: bool = True,
    leaf_base_count: int = 5,
    leaf_length_mm: float = 8.0,
    leaf_width_mm: float = 5.0,
    leaf_thickness_mm: float = 1.2,
    leaf_fold_angle_deg: float = 5.0,
    leaf_keel_depth_mm: float = 1.0,
    leaf_keel_tip_angle_deg: float = 45.0,
    leaf_spacing_factor: float = 1.5,
    leaf_cap_count: int = 12,
    leaf_angle_jitter_deg: float = 24.0,
    leaf_pos_jitter: float = 0.8,
) -> tuple[trimesh.Trimesh, list[trimesh.Trimesh]]:
    """Build a single tree mesh from a simplified skeleton.

    Returns
    -------
    tree_mesh
        Single Trimesh containing trunk, all branches, and foliage sweeps.
        Tagged ``Material.WOOD``.
    attractor_meshes
        Debug icospheres (empty unless ``debug_attractors`` is set).
    """
    _ = terrain_z, out_dirs
    n = len(nodes)
    if strict_fdm_angle_deg is not None:
        _warn_if_branch_below_strict_fdm_angle(nodes, parents, strict_fdm_angle_deg)

    render_foliage = foliage_radius_mm > 0.0
    bark_config = BarkConfig(enabled=False) if bark is None else bark
    render_bark = bool(bark_config.enabled)
    tree_height_mm = max(float(np.max(nodes[:, 2]) - terrain_z), 1e-6)

    # ── children list + leaf classification ───────────────────────────────
    children: list[list[int]] = [[] for _ in range(n)]
    for i in range(1, n):
        children[int(parents[i])].append(i)
    is_leaf = [len(children[i]) == 0 for i in range(n)]

    # ── per-node state ────────────────────────────────────────────────────
    node_frame: list[tuple[np.ndarray, np.ndarray] | None] = [None] * n
    node_bark: list[list[_BarkLine]] = [[] for _ in range(n)]

    # ── root frame ────────────────────────────────────────────────────────
    root_in       = _safe_norm(np.asarray(in_dirs[0], float))
    u0, v0        = _basis(root_in)
    node_frame[0] = (u0, v0)
    if render_bark:
        node_bark[0] = _root_bark_lines(float(radii[0]), bark_config, bark_seed)

    # ── BFS edge loop ─────────────────────────────────────────────────────
    queue   = list(children[0])
    visited = [False] * n
    visited[0] = True
    edge_solids: list[trimesh.Trimesh] = []
    # Leaf shells are kept out of the boolean union: each leaf is an
    # independent watertight shell that need not be CSG-merged with the tree or
    # with other leaves.  Unioning ~thousands of them dominated runtime; we
    # concatenate them onto the unioned trunk/branches/clumps instead (same
    # visible surfaces, vastly cheaper).
    leaf_solids: list[trimesh.Trimesh] = []

    while queue:
        i = queue.pop(0)
        if visited[i]:
            continue
        visited[i] = True

        p  = int(parents[i])
        p0 = np.asarray(nodes[p], float)
        p3 = np.asarray(nodes[i], float)
        parent_frame = node_frame[p]
        if parent_frame is None:
            parent_frame = _basis(_safe_norm(np.asarray(in_dirs[p], float)))
        pu, pv = parent_frame

        length = float(np.linalg.norm(p3 - p0))

        # ── degenerate edge ───────────────────────────────────────────
        if length < 1e-8:
            node_frame[i] = (pu, pv)
            node_bark[i] = [line for line in node_bark[p]]
            queue.extend(children[i])
            continue

        # ── radius profile ────────────────────────────────────────────
        r_start    = max(float(radii[p]), 0.42)
        r_end_wood = max(float(radii[i]), 0.42)

        is_foliage_leaf = render_foliage and is_leaf[i]

        # Wood tube always uses constant radius — the foliage clump is built
        # as a separate subdivided mesh and appended to edge_solids below.
        t_split    = 0.0
        r_cone_end = r_end_wood
        clump_len  = 0.0
        if is_foliage_leaf:
            clump_len = (
                min(length, float(leaf_clump_length_mm))
                if leaf_clump_length_mm is not None
                else length
            )

        bark_end_t = 1.0
        foliage_bark_start_t: float | None = None

        edge_bark = _select_bark_lines(
            node_bark[p],
            r_start,
            bark_config,
        ) if render_bark else []
        edge_end_bark = _advance_bark_lines(edge_bark, length, r_end_wood, bark_config)
        continuing_line_ids: set[int] = set()
        if render_bark and edge_end_bark and bark_end_t >= 1.0 - 1e-9:
            for child in children[i]:
                child_len = float(np.linalg.norm(np.asarray(nodes[child], float) - p3))
                if child_len < 1e-8:
                    continue
                continuing_line_ids.update(
                    line.line_id
                    for line in _select_bark_lines(edge_end_bark, r_end_wood, bark_config)
                )
        end_taper_line_ids = {
            line.line_id for line in edge_bark
            if line.line_id not in continuing_line_ids
        }

        start = p0
        if p != 0:
            parent_tangent = _safe_norm(np.asarray(in_dirs[p], float))
            overlap = max(0.15, 0.25 * r_start)
            overlap = min(overlap, 0.35 * length)
            start = p0 - parent_tangent * overlap

        edge_mesh, end_frame = _build_closed_edge_solid(
            start=start,
            end=p3,
            start_frame=(pu, pv),
            start_tangent=_safe_norm(np.asarray(in_dirs[p], float)),
            end_tangent=_safe_norm(np.asarray(in_dirs[i], float)),
            r_start=r_start,
            r_end_wood=r_end_wood,
            r_cone_end=r_cone_end,
            t_split=t_split,
            handle_scale=handle_scale,
            is_foliage_leaf=False,  # foliage is a separate subdivided mesh
            dome_tip=is_leaf[i],
            bark=bark_config if render_bark else None,
            bark_lines=edge_bark,
            bark_seed=bark_seed,
            edge_id=i,
            bark_end_t=bark_end_t,
            tree_base_z=terrain_z,
            tree_height_mm=tree_height_mm,
            end_taper_line_ids=end_taper_line_ids,
            foliage_bark_start_t=foliage_bark_start_t,
        )
        if len(edge_mesh.vertices) > 0:
            edge_solids.append(edge_mesh)

        # Build the foliage clump as a separate subdivided+displaced solid.
        if is_foliage_leaf and clump_len > 1e-6:
            # Locate where the clump starts on this skeleton edge (Bezier).
            _bt_start = _safe_norm(np.asarray(in_dirs[p], float))
            _bt_end   = _safe_norm(np.asarray(in_dirs[i], float))
            _bh       = handle_scale * length
            _bbp1     = p0 + _bh * _bt_start
            _bbp2     = p3 - _bh * _bt_end
            clump_start_pos, clump_start_tan = _bezier_clump_start(
                p0, _bbp1, _bbp2, p3, clump_len,
            )
            clump, clump_leaves = _build_foliage_clump_mesh(
                tip_pos=p3,
                tip_tangent=_bt_end,
                start_pos=clump_start_pos,
                start_tangent=clump_start_tan,
                r_wood=r_end_wood,
                r_foliage=foliage_radius_mm,
                clump_length_mm=clump_len,
                edge_id=i,
                bark_seed=bark_seed,
                leaf_enable=leaf_enable,
                leaf_base_count=leaf_base_count,
                leaf_length_mm=leaf_length_mm,
                leaf_width_mm=leaf_width_mm,
                leaf_thickness_mm=leaf_thickness_mm,
                leaf_fold_angle_deg=leaf_fold_angle_deg,
                leaf_keel_depth_mm=leaf_keel_depth_mm,
                leaf_keel_tip_angle_deg=leaf_keel_tip_angle_deg,
                leaf_spacing_factor=leaf_spacing_factor,
                leaf_cap_count=leaf_cap_count,
                leaf_angle_jitter_deg=leaf_angle_jitter_deg,
                leaf_pos_jitter=leaf_pos_jitter,
            )
            if len(clump.vertices) > 0:
                edge_solids.append(clump)
            leaf_solids.extend(clump_leaves)

        node_frame[i] = end_frame
        node_bark[i] = edge_end_bark

        queue.extend(children[i])

    # ── assemble ──────────────────────────────────────────────────────────
    # The boolean union cost is driven by the *number* of operands, not their
    # total faces.  Feeding ~thousands of individual leaf shells in dominated
    # runtime.  Pre-concatenate every leaf shell into a single operand first:
    # the union then sees one leaf aggregate instead of thousands, while still
    # removing the buried leaf/clump overlap faces (keeping the mesh compact).
    leaf_solids = [m for m in leaf_solids if len(m.vertices) > 0]
    if leaf_solids:
        edge_solids.append(trimesh.util.concatenate(leaf_solids))
    tree_mesh = _union_edge_solids(edge_solids)
    _tag(tree_mesh, Material.WOOD)

    # ── debug attractor spheres ───────────────────────────────────────────
    attractor_meshes: list[trimesh.Trimesh] = []
    if debug_attractors is not None and len(debug_attractors) > 0:
        ico_base = trimesh.creation.icosphere(subdivisions=0, radius=attractor_radius_mm)
        use_group_colors = (
            attractor_group_labels is not None
            and len(np.unique(attractor_group_labels)) > 1
        )
        for idx, pt in enumerate(debug_attractors):
            s = ico_base.copy()
            s.vertices = s.vertices + pt
            mat = (
                debug_material(int(attractor_group_labels[idx]))
                if use_group_colors else Material.FLOWER
            )
            _tag(s, mat)
            attractor_meshes.append(s)

    return tree_mesh, attractor_meshes


def _build_closed_edge_solid(
    *,
    start: np.ndarray,
    end: np.ndarray,
    start_frame: tuple[np.ndarray, np.ndarray],
    start_tangent: np.ndarray,
    end_tangent: np.ndarray,
    r_start: float,
    r_end_wood: float,
    r_cone_end: float,
    t_split: float,
    handle_scale: float,
    is_foliage_leaf: bool,
    dome_tip: bool,
    bark: BarkConfig | None,
    bark_lines: list[_BarkLine],
    bark_seed: int,
    edge_id: int,
    bark_end_t: float,
    tree_base_z: float,
    tree_height_mm: float,
    end_taper_line_ids: set[int],
    foliage_bark_start_t: float | None,
) -> tuple[trimesh.Trimesh, tuple[np.ndarray, np.ndarray]]:
    """Build one capped branch edge as a closed swept solid."""
    p0 = np.asarray(start, float)
    p3 = np.asarray(end, float)
    length = float(np.linalg.norm(p3 - p0))
    if length < 1e-8:
        return trimesh.Trimesh(process=False), start_frame

    h = handle_scale * length
    bp1 = p0 + h * start_tangent
    bp2 = p3 - h * end_tangent

    step_mm = 2.5
    if bark is not None and bark_lines and bark.roughness_amplitude_mm > 1e-9:
        step_mm = min(step_mm, max(0.35, bark.roughness_cell_mm))
    n_steps = max(4, int(np.ceil(length / step_mm)))
    base_ts = np.linspace(0.0, 1.0, n_steps + 1)
    foliage_bark_end_t_by_id = _foliage_bark_endpoint_t_by_id(
        bark_lines,
        base_ts,
        foliage_bark_start_t,
        edge_id=edge_id,
        bark_seed=bark_seed,
    )
    extra_ts: list[float] = []
    if foliage_bark_start_t is not None:
        extra_ts.append(float(np.clip(foliage_bark_start_t, 0.0, 1.0)))
    if foliage_bark_end_t_by_id is not None:
        extra_ts.extend(foliage_bark_end_t_by_id.values())
    ts = np.array(sorted(set(float(t) for t in np.concatenate((base_ts, extra_ts)))))
    curve = _bezier_eval(p0, bp1, bp2, p3, ts)
    seg_lens = np.linalg.norm(np.diff(curve, axis=0), axis=1)
    arc_s = np.concatenate(([0.0], np.cumsum(seg_lens)))
    foliage_bark_end_s_by_id, foliage_bark_taper_start_s_by_id = (
        _foliage_bark_endpoint_maps(
            foliage_bark_end_t_by_id,
            ts,
            arc_s,
            foliage_bark_start_t,
        )
    )

    # Wood phase tapers to the skeleton child radius; foliage leaves may then
    # expand over the terminal clump length.
    if is_foliage_leaf and t_split > 1e-6:
        radii_t = np.where(
            ts <= t_split,
            r_start + (r_end_wood - r_start) * (ts / t_split),
            r_end_wood
            + (r_cone_end - r_end_wood) * ((ts - t_split) / (1.0 - t_split)),
        )
    else:
        r_final = r_cone_end if is_foliage_leaf else r_end_wood
        radii_t = r_start + (r_final - r_start) * ts

    verts_acc: list[np.ndarray] = []
    faces_acc: list[list[int]] = []
    n_verts = 0

    def _add_verts(arr: np.ndarray) -> int:
        nonlocal n_verts
        off = n_verts
        verts_acc.append(np.asarray(arr, float))
        n_verts += len(arr)
        return off

    step_off: list[int] = []
    n_sides = _N_BARK_SIDES if (bark is not None and bark_lines) else (
        _N_FOLIAGE_SIDES if is_foliage_leaf else _N_SIDES
    )
    u, v = start_frame
    for j in range(len(ts)):
        tan = _safe_norm(_bezier_tangent(p0, bp1, bp2, p3, float(ts[j])))
        if j > 0:
            u, v = _transport(u, v, tan)
        centers = _bark_centers_for_ring(
            bark_lines,
            bark,
            radius=float(radii_t[j]),
            s=float(arc_s[j]),
            t=float(ts[j]),
            edge_id=edge_id,
            bark_seed=bark_seed,
            bark_end_t=bark_end_t,
            edge_length=float(arc_s[-1]),
            z=float(curve[j][2]),
            tree_base_z=tree_base_z,
            tree_height_mm=tree_height_mm,
            end_taper_line_ids=end_taper_line_ids,
            line_end_s_by_id=foliage_bark_end_s_by_id,
            line_taper_start_s_by_id=foliage_bark_taper_start_s_by_id,
        )
        ring = _make_ring(
            curve[j],
            float(radii_t[j]),
            u,
            v,
            n_sides=n_sides,
            bark=bark,
            groove_centers=centers,
            s=float(arc_s[j]),
            edge_id=edge_id,
            bark_seed=bark_seed,
        )
        step_off.append(_add_verts(ring))

    # Start cap faces backward along the edge.
    c_start = _add_verts(curve[0][np.newaxis])
    ro = step_off[0]
    for k in range(n_sides):
        k1 = (k + 1) % n_sides
        faces_acc.append([c_start, ro + k1, ro + k])

    for j in range(len(step_off) - 1):
        oa, ob = step_off[j], step_off[j + 1]
        for k in range(n_sides):
            k1 = (k + 1) % n_sides
            faces_acc.append([oa + k, oa + k1, ob + k1])
            faces_acc.append([oa + k, ob + k1, ob + k])

    n_dome_lats = _N_FOLIAGE_DOME_LATS if is_foliage_leaf else _N_DOME_LATS
    if dome_tip:
        tip_tan = _safe_norm(_bezier_tangent(p0, bp1, bp2, p3, 1.0))
        u_tip, v_tip = u, v
        r_tip = float(radii_t[-1])
        prev_off = step_off[-1]
        # s offset for dome rings: arc length along the sphere cap surface.
        for lat_i in range(1, n_dome_lats + 1):
            phi = (np.pi / 2.0) * lat_i / n_dome_lats
            ring_ctr = curve[-1] + r_tip * float(np.sin(phi)) * tip_tan

            if lat_i < n_dome_lats:
                ring_r = r_tip * float(np.cos(phi))
                dome_ring = _make_ring(ring_ctr, ring_r, u_tip, v_tip, n_sides=n_sides)
                curr_off = _add_verts(dome_ring)
                for k in range(n_sides):
                    k1 = (k + 1) % n_sides
                    faces_acc.append([prev_off + k, prev_off + k1, curr_off + k1])
                    faces_acc.append([prev_off + k, curr_off + k1, curr_off + k])
                prev_off = curr_off
            else:
                c_tip = _add_verts(ring_ctr[np.newaxis])
                for k in range(n_sides):
                    k1 = (k + 1) % n_sides
                    faces_acc.append([c_tip, prev_off + k, prev_off + k1])
    else:
        c_end = _add_verts(curve[-1][np.newaxis])
        eo = step_off[-1]
        for k in range(n_sides):
            k1 = (k + 1) % n_sides
            faces_acc.append([c_end, eo + k, eo + k1])

    mesh = trimesh.Trimesh(
        vertices=np.vstack(verts_acc),
        faces=np.array(faces_acc, dtype=np.int32),
        process=False,
    )
    mesh.fix_normals()
    return mesh, (u, v)


def _union_edge_solids(edge_solids: list[trimesh.Trimesh]) -> trimesh.Trimesh:
    if not edge_solids:
        return trimesh.Trimesh(process=False)

    if len(edge_solids) == 1:
        return edge_solids[0]

    # Concatenate instead of boolean union.  Each branch tube and foliage
    # clump is already a closed, correctly-oriented solid; slicers compute
    # the geometric union during slicing, so overlapping shells produce an
    # identical 3D-print result.  The manifold boolean on 80+ operands with
    # 8+ M total faces was taking >16 s per tree — the dominant cost.
    return trimesh.util.concatenate(edge_solids)


# ── Foliage clump: icosphere deformed to cone+dome profile, Gaussian noise ─────

# Icosphere subdivision level for the whole clump.  3 → 1280 faces; 4 → 5120.
_FOLIAGE_ICO_SUBDIVISIONS          = 3
# Fine Gaussian noise: per-vertex surface grain.
_FOLIAGE_NOISE_AMPLITUDE_MM        = 0.10  # 1-sigma displacement (mm)
# Coarse smooth noise: large-scale silhouette distortion.
_FOLIAGE_COARSE_NOISE_AMPLITUDE_MM = 1.0   # peak ± displacement (mm)
_FOLIAGE_COARSE_NOISE_CELL_MM      = 4.0   # spatial wavelength (mm)
# Maximum possible inward noise erosion of the foliage surface: coarse peak
# amplitude plus 2σ of the fine Gaussian.  Used to pre-sink the foliage cone
# so branches stay buried under the skin even in the worst-noise case.
_FOLIAGE_MAX_NOISE_MM = _FOLIAGE_COARSE_NOISE_AMPLITUDE_MM + 2.0 * _FOLIAGE_NOISE_AMPLITUDE_MM
# Extra inward sink for leaf bases past the noised skin, so each base stays in
# contact (slightly embedded) rather than skimming the surface.
_LEAF_BASE_EMBED_MM = 0.0
# Downward droop of leaf bases: weight of the (perp-to-radial) downward
# component relative to the outward radial when forming the leaf tangent.
# 0 = straight out along the surface normal; larger = more droop toward the
# ground.
_LEAF_DROOP_WEIGHT = 1.4


def _foliage_gaussian_noise(
    verts: np.ndarray,
    edge_id: int,
    bark_seed: int,
) -> np.ndarray:
    """Per-vertex Gaussian noise keyed on 3-D world position.

    Noise is a function of spatial position, not vertex-array index, so
    vertices at the same geometric location (e.g. the cone/dome seam ring)
    receive the same displacement — no seam artefacts and no apex-fan star
    pattern from the polar singularity.

    A tiny quantisation cell (0.05 mm) means vertices further apart than
    ~0.05 mm are essentially independent, giving the appearance of per-vertex
    Gaussian noise while remaining continuous at topology boundaries.
    """
    cell_mm = 0.05
    coords = np.floor(verts / cell_mm).astype(np.int64)   # (N, 3)

    base = np.uint64(_hash01_int(bark_seed, "fol-gauss", edge_id))
    m64  = np.uint64(1099511628211)

    # Hash each quantised position → uniform u1, u2 in (0, 1].
    h = np.full(len(verts), base, dtype=np.uint64)
    for dim in range(3):
        h ^= coords[:, dim].astype(np.uint64)
        h  = h * m64
    u1 = np.clip(h.astype(np.float64) / float(2**64), 1e-10, 1.0)

    h2 = h * m64
    h2 ^= np.uint64(0xDEADBEEFCAFEBABE)
    h2  = h2 * m64
    u2  = h2.astype(np.float64) / float(2**64)

    # Box-Muller: (u1, u2) → standard normal.
    normal = np.sqrt(-2.0 * np.log(u1)) * np.cos(2.0 * np.pi * u2)
    return normal * _FOLIAGE_NOISE_AMPLITUDE_MM


def _hash01_int(*parts: object) -> int:
    """Same hash as _hash01 but returns the raw 64-bit integer (no /2^64)."""
    h = 1469598103934665603
    for part in parts:
        for byte in str(part).encode("utf-8"):
            h ^= byte
            h  = (h * 1099511628211) & 0xFFFFFFFFFFFFFFFF
        h ^= 0xFF
        h  = (h * 1099511628211) & 0xFFFFFFFFFFFFFFFF
    return h


def _foliage_coarse_noise(
    verts: np.ndarray,
    edge_id: int,
    bark_seed: int,
) -> np.ndarray:
    """Single-octave 3-D smooth (trilinearly interpolated) value noise.

    Uses a large spatial cell (_FOLIAGE_COARSE_NOISE_CELL_MM) so the
    displacement varies slowly across the surface, distorting the overall
    silhouette of the clump rather than adding fine grain.
    """
    cell   = _FOLIAGE_COARSE_NOISE_CELL_MM
    m64    = np.uint64(1099511628211)
    base   = np.uint64(_hash01_int(bark_seed, "fol-coarse", edge_id))
    N      = len(verts)

    p    = verts / cell
    fl   = np.floor(p).astype(np.int64)
    frac = p - fl
    s    = frac * frac * (3.0 - 2.0 * frac)   # smoothstep per axis

    val = np.zeros(N, dtype=np.float64)
    for dz in (0, 1):
        wz = s[:, 2] if dz else (1.0 - s[:, 2])
        for dy in (0, 1):
            wy = s[:, 1] if dy else (1.0 - s[:, 1])
            for dx in (0, 1):
                wx = s[:, 0] if dx else (1.0 - s[:, 0])
                h  = np.full(N, base, dtype=np.uint64)
                h ^= (fl[:, 0] + dx).astype(np.uint64); h = h * m64
                h ^= (fl[:, 1] + dy).astype(np.uint64); h = h * m64
                h ^= (fl[:, 2] + dz).astype(np.uint64); h = h * m64
                corner = h.astype(np.float64) / np.float64(2**64) * 2.0 - 1.0
                val += corner * (wx * wy * wz)

    return _FOLIAGE_COARSE_NOISE_AMPLITUDE_MM * val


def _bezier_clump_start(
    p0: np.ndarray,
    bp1: np.ndarray,
    bp2: np.ndarray,
    p3: np.ndarray,
    clump_len: float,
    n_samples: int = 64,
) -> tuple[np.ndarray, np.ndarray]:
    """Position and tangent on a cubic Bezier at arc-length clump_len from the tip.

    Walks arc length backward from p3 to find where the foliage clump starts,
    returning (start_pos, start_tangent) for the bent-cone spine.
    """
    ts           = np.linspace(0.0, 1.0, n_samples)
    pts          = _bezier_eval(p0, bp1, bp2, p3, ts)                # (N, 3)
    segs         = np.linalg.norm(np.diff(pts[::-1], axis=0), axis=1)
    arc_from_tip = np.concatenate(([0.0], np.cumsum(segs)))
    t_start      = float(np.clip(
        np.interp(clump_len, arc_from_tip, ts[::-1]), 0.0, 1.0,
    ))
    start_pos    = _bezier_eval(p0, bp1, bp2, p3, np.array([t_start]))[0]
    start_tan    = _safe_norm(_bezier_tangent(p0, bp1, bp2, p3, t_start))
    return start_pos, start_tan


def _build_foliage_clump_mesh(
    *,
    tip_pos: np.ndarray,
    tip_tangent: np.ndarray,
    start_pos: np.ndarray,
    start_tangent: np.ndarray,
    r_wood: float,
    r_foliage: float,
    clump_length_mm: float,
    edge_id: int,
    bark_seed: int,
    leaf_enable: bool = False,
    leaf_base_count: int = 0,
    leaf_length_mm: float = 0.0,
    leaf_width_mm: float = 0.0,
    leaf_thickness_mm: float = 0.24,
    leaf_fold_angle_deg: float = 5.0,
    leaf_keel_depth_mm: float = 1.0,
    leaf_keel_tip_angle_deg: float = 45.0,
    leaf_spacing_factor: float = 1.5,
    leaf_cap_count: int = 12,
    leaf_angle_jitter_deg: float = 24.0,
    leaf_pos_jitter: float = 0.8,
) -> tuple[trimesh.Trimesh, list[trimesh.Trimesh]]:
    """Foliage clump: icosphere bent along the branch Bezier spine.

    Profile arc segments (back → front):
      1. Back hemisphere  (radius r_wood,    arc = π·r_wood/2)
         — extends backward from start_pos along start_tangent; ring centres
           shifted 0.5 × ring_radius perpendicular-upward (tc = 0 factor).
      2. Cone body        (r_wood → r_foliage)
         — cross-sections swept along the Bezier spine from start_pos to tip_pos.
           Each ring centre is shifted perpendicular-upward by a fraction of
           its own radius that rises smoothly from 0.5 at the base (tc=0) to
           (1 − r_wood/r_foliage) at the tip (tc=1):
               factor(tc) = 0.5 + (0.5 − r_wood/r_foliage) × smoothstep(tc)
           At tc=0 the branch sits halfway into the lower half of the clump; at
           tc=1 the ring bottom aligns exactly with the branch bottom (fully
           embedded — no protrusion).
      3. Forward dome     (radius r_foliage, arc = π·r_foliage/2)
         — extends forward from tip_pos along tip_tangent; centre shifted
           (r_foliage − r_wood) perpendicular-upward (tc=1 factor), so the dome
           bottom aligns with the branch bottom and the branch is fully inside.

    Noise applied along surface normals:
      • Fine Gaussian  (0.05 mm cell): per-vertex surface grain
      • Coarse smooth  (4 mm cell):    large-scale silhouette distortion
    """
    r_base  = float(r_wood)
    r_tip   = float(r_foliage)
    tip_t   = _safe_norm(np.asarray(tip_tangent,   float))
    start_t = _safe_norm(np.asarray(start_tangent, float))
    tip_p   = np.asarray(tip_pos,   float)
    start_p = np.asarray(start_pos, float)

    # ── Bezier spine: start_pos → tip_pos ────────────────────────────────────
    spine_d  = float(np.linalg.norm(tip_p - start_p))
    sh       = 0.45 * max(spine_d, 1e-6)
    s_bp1    = start_p + sh * start_t
    s_bp2    = tip_p   - sh * tip_t

    N_SPINE    = 64
    spine_ts   = np.linspace(0.0, 1.0, N_SPINE)
    spine_pts  = _bezier_eval(start_p, s_bp1, s_bp2, tip_p, spine_ts)   # (N, 3)
    spine_traw = np.vstack([
        _bezier_tangent(start_p, s_bp1, s_bp2, tip_p, float(t))
        for t in spine_ts
    ])
    tn         = np.linalg.norm(spine_traw, axis=1, keepdims=True)
    spine_tans = spine_traw / np.where(tn > 1e-10, tn, 1.0)             # (N, 3)
    seg_lens   = np.linalg.norm(np.diff(spine_pts, axis=0), axis=1)
    spine_arc  = np.concatenate(([0.0], np.cumsum(seg_lens)))            # (N,)
    tot_spine  = float(spine_arc[-1])

    # ── Profile arc lengths ───────────────────────────────────────────────────
    south_arc    = (np.pi / 2.0) * r_base
    cone_arc_p   = float(np.sqrt(tot_spine ** 2 + (r_tip - r_base) ** 2))
    north_arc    = (np.pi / 2.0) * r_tip
    total_arc    = south_arc + cone_arc_p + north_arc

    # ── Icosphere ────────────────────────────────────────────────────────────
    ico    = trimesh.creation.icosphere(subdivisions=_FOLIAGE_ICO_SUBDIVISIONS, radius=1.0)
    uverts = ico.vertices.copy()                      # (M, 3) unit sphere

    dot_t  = uverts @ tip_t                           # latitude ∈ [-1, 1]
    u_vals = (dot_t + 1.0) * 0.5 * total_arc         # arc param ∈ [0, total_arc]

    # Unit radial direction for each vertex (perp to tip_t on the unit sphere).
    rvec   = uverts - dot_t[:, None] * tip_t
    r_lat  = np.linalg.norm(rvec, axis=1, keepdims=True)
    rad_u  = np.where(r_lat > 1e-10, rvec / r_lat, 0.0)               # (M, 3)

    verts  = np.zeros_like(uverts)

    # Perpendicular-upward offset: shift each ring center by that ring's own
    # radius in the world-up direction projected perp to the local tangent.
    # This places the bottom edge of every ring exactly at the branch centerline,
    # exposing the full branch on the underside of the foliage.
    _WUP = np.array([0.0, 0.0, 1.0])

    def _pu_unit_scalar(tangent: np.ndarray) -> np.ndarray:
        """World-up perp to tangent, normalised (zero if tangent is vertical)."""
        p = _WUP - float(np.dot(_WUP, tangent)) * tangent
        n = float(np.linalg.norm(p))
        return p / n if n > 1e-6 else np.zeros(3)

    def _pu_unit_batch(tangents: np.ndarray) -> np.ndarray:
        """Per-row world-up perp to tangent, normalised, shape (M, 3)."""
        p = _WUP - (tangents * _WUP).sum(axis=1, keepdims=True) * tangents
        n = np.linalg.norm(p, axis=1, keepdims=True)
        return np.where(n > 1e-6, p / n, 0.0)

    # ── Back hemisphere: backward from start_p along start_t ─────────────────
    ms = u_vals <= south_arc
    if ms.any():
        phi    = (u_vals[ms] / south_arc) * (np.pi / 2.0)
        ax_s   = r_base * (np.sin(phi) - 1.0)           # ≤ 0 (behind start_p)
        rr_s   = r_base * np.cos(phi)
        ru     = rad_u[ms]
        dp     = (ru * start_t).sum(axis=1, keepdims=True)
        rp     = ru - dp * start_t
        rn     = np.linalg.norm(rp, axis=1, keepdims=True)
        rpu    = np.where(rn > 1e-10, rp / rn, 0.0)
        pu_s   = _pu_unit_scalar(start_t)               # (3,) fixed direction
        # Back hemisphere uses the base factor (0.5) for continuity with the cone.
        verts[ms] = start_p + pu_s * (0.5 * rr_s[:, None]) + ax_s[:, None] * start_t + rpu * rr_s[:, None]

    # ── Cone body: cross-sections swept along Bezier spine ────────────────────
    mc = (~ms) & (u_vals <= south_arc + cone_arc_p)
    if mc.any():
        tc   = (u_vals[mc] - south_arc) / cone_arc_p    # [0, 1] along spine
        rr_c = r_base + tc * (r_tip - r_base)
        sv   = tc * tot_spine                            # arc position on spine

        sp_c = np.column_stack([
            np.interp(sv, spine_arc, spine_pts[:, d]) for d in range(3)
        ])
        st_raw = np.column_stack([
            np.interp(sv, spine_arc, spine_tans[:, d]) for d in range(3)
        ])
        stn  = np.linalg.norm(st_raw, axis=1, keepdims=True)
        st_c = st_raw / np.where(stn > 1e-10, stn, 1.0)   # local spine tangent

        ru   = rad_u[mc]
        dp   = (ru * st_c).sum(axis=1, keepdims=True)
        rp   = ru - dp * st_c
        rn   = np.linalg.norm(rp, axis=1, keepdims=True)
        rpu  = np.where(rn > 1e-10, rp / rn, 0.0)
        pu_c = _pu_unit_batch(st_c)                     # (M, 3) per-vertex direction
        # Offset rises smoothly from 0.5 at the base (tc=0) to factor_tip at
        # the tip (tc=1) via cubic smoothstep.
        # factor_tip gives offset = r_tip - r_base - _FOLIAGE_MAX_NOISE_MM, so
        # the ring bottom sits _FOLIAGE_MAX_NOISE_MM below the branch bottom.
        # After the noise erodes the surface inward by up to that amount, the
        # branch bottom ends up flush with (but not exposed through) the skin.
        factor_tip = max(0.0, 1.0 - (r_base + _FOLIAGE_MAX_NOISE_MM) / max(r_tip, 1e-6))
        smooth_tc  = 3.0 * tc ** 2 - 2.0 * tc ** 3     # smoothstep ∈ [0, 1]
        tc_factor  = 0.5 + (factor_tip - 0.5) * smooth_tc
        verts[mc] = sp_c + pu_c * (rr_c * tc_factor)[:, None] + rpu * rr_c[:, None]

    # ── Forward dome: forward from tip_p along tip_t ──────────────────────────
    mn = u_vals > south_arc + cone_arc_p
    if mn.any():
        phi    = (u_vals[mn] - south_arc - cone_arc_p) / north_arc * (np.pi / 2.0)
        ax_n   = r_tip * np.sin(phi)
        rr_n   = r_tip * np.cos(phi)
        pu_tip     = _pu_unit_scalar(tip_t)
        dome_shift = max(0.0, r_tip - r_base - _FOLIAGE_MAX_NOISE_MM)
        # Blend the perp-upward offset from full at the equator (phi=0, matching
        # tc=1 cone) to zero at the north pole (phi=pi/2), so the dome tip lies
        # on the branch axis rather than being shifted sideways.  Eliminates the
        # offset dome appearance under tight branch curves.
        shift_blend = np.cos(phi)
        ring_ctr = tip_p + ax_n[:, None] * tip_t + (dome_shift * shift_blend)[:, None] * pu_tip
        verts[mn] = ring_ctr + rad_u[mn] * rr_n[:, None]

    # ── Normals + two noise layers ────────────────────────────────────────────
    shaped  = trimesh.Trimesh(vertices=verts, faces=ico.faces.copy(), process=False)
    shaped.fix_normals()
    normals = shaped.vertex_normals.copy()

    disp = (
        _foliage_gaussian_noise(verts, edge_id, bark_seed)
        + _foliage_coarse_noise(verts, edge_id, bark_seed)
    )
    # Shift the full noise wave inward: subtract its peak so the maximum
    # displacement is exactly 0 (the smooth envelope) and the full 2A
    # trough range erodes inward — no amplitude loss, no outward expansion.
    noise_peak = float(disp.max())
    disp = disp - noise_peak
    verts = verts + normals * disp[:, np.newaxis]

    # ── Leaves: rows up the cone, over the dome, capped at the apex ──────────
    leaf_parts: list[trimesh.Trimesh] = []
    if leaf_enable and leaf_base_count > 0 and leaf_length_mm > 1e-6 and leaf_width_mm > 1e-6:
        _WD = np.array([0.0, 0.0, -1.0])
        factor_tip = max(0.0, 1.0 - (r_base + _FOLIAGE_MAX_NOISE_MM) / max(r_tip, 1e-6))
        jit = np.radians(float(leaf_angle_jitter_deg))
        pj  = float(leaf_pos_jitter)
        # Leaves wrap the full circumference of every cone and dome (theta is
        # periodic, so no clipping); theta = 0 points world-up.
        pu_tip, vp_tip = _two_perp(tip_t)

        def _cone_point(tc: float, theta: float):
            """Smooth-cone surface point + outward radial + ring radius at (tc, θ)."""
            sv  = tc * tot_spine
            sp  = np.array([np.interp(sv, spine_arc, spine_pts[:, d]) for d in range(3)])
            stj = _safe_norm(np.array([np.interp(sv, spine_arc, spine_tans[:, d]) for d in range(3)]))
            pu, vp = _two_perp(stj)
            rr  = r_base + tc * (r_tip - r_base)
            tcf = 0.5 + (factor_tip - 0.5) * (3.0 * tc ** 2 - 2.0 * tc ** 3)
            ctr = sp + pu * rr * tcf
            radial = float(np.cos(theta)) * pu + float(np.sin(theta)) * vp
            return ctr + rr * radial, radial, rr

        def _dome_point(phi: float, theta: float):
            """Smooth forward-dome surface point + outward normal + ring radius."""
            ax  = r_tip * np.sin(phi)
            rr  = r_tip * np.cos(phi)
            dome_shift = max(0.0, r_tip - r_base - _FOLIAGE_MAX_NOISE_MM)
            ctr  = tip_p + ax * tip_t + dome_shift * np.cos(phi) * pu_tip
            perp = float(np.cos(theta)) * pu_tip + float(np.sin(theta)) * vp_tip
            pt   = ctr + rr * perp
            normal = _safe_norm(np.sin(phi) * tip_t + np.cos(phi) * perp)
            return pt, normal, rr

        def _emit_leaf(
            base_smooth: np.ndarray,
            radial: np.ndarray,
            key,
            *,
            ltan_override: np.ndarray | None = None,
        ) -> None:
            radial = _safe_norm(radial)
            # Sink the base from the smooth surface onto the *noised* skin (the
            # skin was eroded inward by noise_peak − disp along its ≈ normal);
            # evaluating the same noise fields here reproduces that, then
            # _LEAF_BASE_EMBED_MM tucks the base slightly inside for contact.
            disp_base = float(
                _foliage_gaussian_noise(base_smooth[None, :], edge_id, bark_seed)[0]
                + _foliage_coarse_noise(base_smooth[None, :], edge_id, bark_seed)[0]
            )
            lbase = base_smooth + radial * ((disp_base - noise_peak) - _LEAF_BASE_EMBED_MM)
            # Growth tangent: outward + droop toward the ground.
            # ltan_override bypasses the droop calculation — callers near the pole
            # pass a horizontal spread direction so the leaf faces the viewer from
            # above rather than spiking vertically (droop → dp≈0 → ltan≈up at
            # high phi, producing invisible vertical spikes).
            if ltan_override is not None:
                ltan = _safe_norm(np.asarray(ltan_override, float))
            else:
                dp   = _WD - float(np.dot(_WD, radial)) * radial
                dp_n = float(np.linalg.norm(dp))
                ltan = _safe_norm(radial + _LEAF_DROOP_WEIGHT * dp / dp_n) if dp_n > 1e-9 else radial
            # Angle jitter: pitch (extra droop) about the lateral axis, plus a
            # rotation (yaw) about the surface normal — a few degrees each.
            if jit > 1e-9:
                a_pitch = (2.0 * _hash01(bark_seed, "leaf-pitch", edge_id, key) - 1.0) * jit
                a_yaw   = (2.0 * _hash01(bark_seed, "leaf-yaw",   edge_id, key) - 1.0) * jit
                ltan = _rotate_vec(ltan, np.cross(ltan, radial), a_pitch)
                ltan = _safe_norm(_rotate_vec(ltan, radial, a_yaw))
            lseed = _hash01_int(bark_seed, "base-leaf", edge_id, key)
            for lp in build_leaf_mesh(
                base_pos=lbase,
                tangent=ltan,
                length_mm=float(leaf_length_mm),
                width_mm=float(leaf_width_mm),
                thickness_mm=float(leaf_thickness_mm),
                fold_angle_deg=float(leaf_fold_angle_deg),
                keel_depth_mm=float(leaf_keel_depth_mm),
                keel_tip_angle_deg=float(leaf_keel_tip_angle_deg),
                up_hint=radial,   # top/crease faces outward, away from the cone
                seed=lseed,
            ):
                if len(lp.vertices) > 0:
                    leaf_parts.append(lp)

        # Tile leaves side-by-side over the whole cap: a ~leaf_width grid in
        # surface arc-length (rows) and circumference (columns).  The arc spans
        # from near the cone base, up the cone, and over the dome — stopping just
        # short of the apex, which the cap fills.  Row/column counts are derived
        # from the spacing so coverage stays dense regardless of clump size.
        spacing = max(float(leaf_width_mm) * float(leaf_spacing_factor), 1e-3)
        arc_lo  = south_arc + 0.05 * cone_arc_p
        arc_hi  = total_arc - 0.10 * north_arc
        arc_span = max(arc_hi - arc_lo, 1e-6)
        cone_end_arc = south_arc + cone_arc_p
        n_rows  = max(1, int(np.ceil(arc_span / spacing)))

        def _radius_at_arc(arc: float) -> float:
            if arc <= cone_end_arc:
                tc = (arc - south_arc) / max(cone_arc_p, 1e-9)
                return r_base + tc * (r_tip - r_base)
            phi = (arc - cone_end_arc) / max(north_arc, 1e-9) * (np.pi / 2.0)
            return float(r_tip * np.cos(phi))

        for ri in range(n_rows):
            arc_center = arc_lo + ((ri + 0.5) / n_rows) * arc_span
            rr_row     = _radius_at_arc(arc_center)   # for column count only
            # Columns fill the full circumference (2π·rr) at the row spacing.
            # Alternate rows are offset half a cell (brick stagger).
            n_col   = max(1, int(np.ceil((2.0 * np.pi * rr_row) / spacing)))
            stagger = 0.5 * (ri % 2)
            for ci in range(n_col):
                # Independent per-leaf jitter in BOTH directions: up/down along
                # the arc and left/right around the circumference.
                a_jit = (2.0 * _hash01(bark_seed, "leaf-arc", edge_id, ri, ci) - 1.0) * pj * spacing
                arc   = float(np.clip(arc_center + a_jit, arc_lo, arc_hi))
                on_cone = arc <= cone_end_arc
                if on_cone:
                    tc  = (arc - south_arc) / max(cone_arc_p, 1e-9)
                else:
                    phi = (arc - cone_end_arc) / max(north_arc, 1e-9) * (np.pi / 2.0)
                rr_leaf = _radius_at_arc(arc)
                th_jit  = pj * spacing / max(rr_leaf, 1e-6)   # spacing → angle
                col_f   = (ci + 0.5 + stagger) / n_col
                t_jit   = (2.0 * _hash01(bark_seed, "leaf-col", edge_id, ri, ci) - 1.0) * th_jit
                theta   = -np.pi + col_f * 2.0 * np.pi + t_jit
                if on_cone:
                    base_smooth, radial, _ = _cone_point(tc, theta)
                else:
                    base_smooth, radial, _ = _dome_point(phi, theta)
                _emit_leaf(base_smooth, radial, (ri, ci))

        # Near-apex ring: one structured circumferential ring inside the actual
        # gap zone — between the last main-grid row (~phi 73.5° at default
        # 1.5mm leaf spacing) and the pole.  At phi=81° the ring sits squarely
        # in the uncovered cap region and its columns use a horizontal SPREAD
        # tangent (outward from the tip axis, not the droop direction) so each
        # leaf presents its face toward a top-down viewer.  The droop formula
        # produces dp≈0 near the pole, making leaves vertical spikes that are
        # invisible from above — the spread tangent avoids this.
        near_apex_phi = (np.pi / 2.0) * 0.90          # ≈ 81° dome latitude
        near_apex_rr  = r_tip * float(np.cos(near_apex_phi))
        n_col_na      = max(1, int(np.ceil((2.0 * np.pi * near_apex_rr) / spacing)))
        phi_jit_scale = pj * spacing / max(r_tip, 1e-6)
        for ci in range(n_col_na):
            phi_jit_na = (
                2.0 * _hash01(bark_seed, "na-arc", edge_id, ci) - 1.0
            ) * phi_jit_scale
            phi_na   = float(np.clip(near_apex_phi + phi_jit_na, 0.02, np.pi / 2.0 - 0.02))
            th_na    = pj * spacing / max(near_apex_rr, 1e-6)
            t_jit_na = (
                2.0 * _hash01(bark_seed, "na-col", edge_id, ci) - 1.0
            ) * th_na
            theta_na = -np.pi + (ci + 0.5) / n_col_na * 2.0 * np.pi + t_jit_na
            base_smooth, radial, _ = _dome_point(phi_na, theta_na)
            spread_na = (
                float(np.cos(theta_na)) * pu_tip + float(np.sin(theta_na)) * vp_tip
            )
            _emit_leaf(base_smooth, radial, ("near-apex", ci), ltan_override=spread_na)

        # Cap: random leaves spread over the polar zone.  Spread tangent (horizontal
        # outward from the tip axis) makes each leaf face the top-down viewer
        # rather than standing as a vertical spike.
        for ci in range(max(0, int(leaf_cap_count))):
            u_theta = _hash01(bark_seed, "cap-theta", edge_id, ci)
            u_phi   = _hash01(bark_seed, "cap-phi",   edge_id, ci)
            theta   = -np.pi + 2.0 * np.pi * u_theta            # full circle
            phi     = (np.pi / 2.0) - (0.30 * np.pi / 2.0) * u_phi  # spread to ~63°
            base_smooth, radial, _ = _dome_point(phi, theta)
            spread_cap = (
                float(np.cos(theta)) * pu_tip + float(np.sin(theta)) * vp_tip
            )
            _emit_leaf(base_smooth, radial, ("cap", ci), ltan_override=spread_cap)

    result = trimesh.Trimesh(vertices=verts, faces=ico.faces.copy(), process=False)
    result.fix_normals()
    return result, leaf_parts


# ── Bark helpers ──────────────────────────────────────────────────────────────

def _root_bark_lines(
    root_radius: float,
    bark: BarkConfig,
    bark_seed: int,
) -> list[_BarkLine]:
    n_root = max(5, int(np.floor((2.0 * np.pi * root_radius) / bark.spacing_mm)))
    return [
        _BarkLine(
            line_id=i,
            phase=(
                2.0 * np.pi * bark.phase_jitter
                * _hash01(bark_seed, "bark-phase", i)
            ),
            theta=2.0 * np.pi * i / n_root,
        )
        for i in range(n_root)
    ]


def _select_bark_lines(
    parent_lines: list[_BarkLine],
    radius: float,
    bark: BarkConfig,
) -> list[_BarkLine]:
    if not parent_lines or radius < bark.min_branch_radius_mm:
        return []
    n_desired = max(5, int(np.floor((2.0 * np.pi * radius) / bark.spacing_mm)))
    if n_desired >= len(parent_lines):
        return [_BarkLine(line.line_id, line.phase, line.theta) for line in parent_lines]

    ordered = sorted(parent_lines, key=lambda line: line.theta % (2.0 * np.pi))
    selected_idx = np.linspace(0, len(ordered), n_desired, endpoint=False)
    selected = [ordered[int(round(idx)) % len(ordered)] for idx in selected_idx]

    by_id: dict[int, _BarkLine] = {}
    for line in selected:
        by_id[line.line_id] = line
    if len(by_id) < n_desired:
        for line in ordered:
            by_id.setdefault(line.line_id, line)
            if len(by_id) == n_desired:
                break

    return [_BarkLine(line.line_id, line.phase, line.theta) for line in by_id.values()]


def _advance_bark_lines(
    edge_lines: list[_BarkLine],
    edge_length: float,
    radius: float,
    bark: BarkConfig,
) -> list[_BarkLine]:
    if not edge_lines:
        return []
    return [
        _BarkLine(
            line.line_id,
            line.phase,
            _wrap_angle(_bark_theta(line, bark, edge_length, radius)),
        )
        for line in edge_lines
    ]


def _foliage_bark_endpoint_t_by_id(
    bark_lines: list[_BarkLine],
    ts: np.ndarray,
    foliage_bark_start_t: float | None,
    *,
    edge_id: int,
    bark_seed: int,
) -> dict[int, float] | None:
    if foliage_bark_start_t is None or not bark_lines:
        return None

    start_t = float(np.clip(foliage_bark_start_t, 0.0, 1.0))
    next_ring_idx = int(np.searchsorted(ts, start_t, side="right"))
    if next_ring_idx >= len(ts):
        return None

    segment_end_t = float(ts[next_ring_idx])
    if segment_end_t <= start_t + 1e-9:
        return None

    end_t_by_id: dict[int, float] = {}
    span = segment_end_t - start_t
    for line in bark_lines:
        u = _hash01(bark_seed, "foliage-bark-end", edge_id, line.line_id)
        end_t_by_id[line.line_id] = start_t + span * (0.2 + 0.75 * u)
    return end_t_by_id


def _foliage_bark_endpoint_maps(
    end_t_by_id: dict[int, float] | None,
    ts: np.ndarray,
    arc_s: np.ndarray,
    foliage_bark_start_t: float | None,
) -> tuple[dict[int, float] | None, dict[int, float] | None]:
    if foliage_bark_start_t is None or not end_t_by_id:
        return None, None

    start_t = float(np.clip(foliage_bark_start_t, 0.0, 1.0))
    start_s = float(np.interp(start_t, ts, arc_s))
    end_s_by_id = {
        line_id: float(np.interp(end_t, ts, arc_s))
        for line_id, end_t in end_t_by_id.items()
    }
    taper_start_s_by_id = {line_id: start_s for line_id in end_t_by_id}
    return end_s_by_id, taper_start_s_by_id


def _bark_centers_for_ring(
    bark_lines: list[_BarkLine],
    bark: BarkConfig | None,
    *,
    radius: float,
    s: float,
    t: float,
    edge_id: int,
    bark_seed: int,
    bark_end_t: float,
    edge_length: float | None = None,
    z: float | None = None,
    tree_base_z: float | None = None,
    tree_height_mm: float | None = None,
    end_taper_line_ids: set[int] | None = None,
    line_end_s_by_id: dict[int, float] | None = None,
    line_taper_start_s_by_id: dict[int, float] | None = None,
) -> list[tuple[int, float, float]]:
    if bark is None or not bark_lines or radius < bark.min_branch_radius_mm:
        return []
    if line_end_s_by_id is None and t > bark_end_t:
        return []
    centers = []
    for line in bark_lines:
        line_end_s = None
        line_taper_start_s = None
        if line_end_s_by_id is not None:
            line_end_s = line_end_s_by_id.get(line.line_id)
            if line_taper_start_s_by_id is not None:
                line_taper_start_s = line_taper_start_s_by_id.get(line.line_id)
        strength = _bark_taper_strength(
            line.line_id,
            bark,
            s=s,
            t=t,
            bark_end_t=bark_end_t,
            edge_length=edge_length,
            end_taper_line_ids=end_taper_line_ids,
            line_end_s=line_end_s,
            line_taper_start_s=line_taper_start_s,
        )
        if strength > 1e-6:
            centers.append((
                line.line_id,
                _wrap_angle(
                    _bark_theta(line, bark, s, radius)
                    + _bark_twist_angle(bark, z, tree_base_z, tree_height_mm)
                ),
                strength,
            ))
    return _filter_non_overlapping_centers(
        centers,
        radius=radius,
        min_gap_mm=bark.width_mm * 1.25,
        edge_id=edge_id,
        bark_seed=bark_seed,
    )


def _bark_taper_strength(
    line_id: int,
    bark: BarkConfig,
    *,
    s: float,
    t: float,
    bark_end_t: float,
    edge_length: float | None,
    end_taper_line_ids: set[int] | None,
    line_end_s: float | None = None,
    line_taper_start_s: float | None = None,
) -> float:
    if line_end_s is not None:
        start_s = 0.0 if line_taper_start_s is None else float(line_taper_start_s)
        taper_len = max(float(line_end_s) - start_s, 1e-6)
        x = np.clip((float(line_end_s) - s) / taper_len, 0.0, 1.0)
        return float(x * x * (3.0 - 2.0 * x))

    taper_all_at_bark_end = bark_end_t < 1.0 - 1e-9
    taper_this_line = (
        taper_all_at_bark_end
        or (end_taper_line_ids is not None and line_id in end_taper_line_ids)
    )
    if not taper_this_line:
        return 1.0

    taper_len = max(float(bark.width_mm) * 3.0, 1.0)
    if edge_length is not None and edge_length > 1e-9:
        end_s = max(0.0, min(edge_length, bark_end_t * edge_length))
        taper_len = min(taper_len, max(edge_length * 0.45, 1e-6))
        x = np.clip((end_s - s) / taper_len, 0.0, 1.0)
    else:
        taper_t = min(0.18, max(bark_end_t, 1e-6))
        x = np.clip((bark_end_t - t) / taper_t, 0.0, 1.0)

    # Smoothstep keeps full-width grooves flat until the taper starts, then
    # lands with zero slope at the end point.
    return float(x * x * (3.0 - 2.0 * x))


def _bark_theta(line: _BarkLine, bark: BarkConfig, s: float, radius: float) -> float:
    wave_amp = min(float(bark.wave_amplitude_mm), 0.25 * float(bark.spacing_mm))
    if bark.wave_length_mm <= 1e-9:
        return line.theta
    return line.theta + (wave_amp / max(radius, 1e-6)) * np.sin(
        2.0 * np.pi * s / bark.wave_length_mm + line.phase
    )


def _bark_twist_angle(
    bark: BarkConfig,
    z: float | None,
    tree_base_z: float | None,
    tree_height_mm: float | None,
) -> float:
    if abs(bark.twist_rotations) <= 1e-9:
        return 0.0
    if z is None or tree_base_z is None or tree_height_mm is None:
        return 0.0
    height = max(float(tree_height_mm), 1e-6)
    t = np.clip((float(z) - float(tree_base_z)) / height, 0.0, 1.0)
    return float(2.0 * np.pi * bark.twist_rotations * t)


def _filter_non_overlapping_centers(
    centers: list[tuple[int, float, float]],
    *,
    radius: float,
    min_gap_mm: float,
    edge_id: int,
    bark_seed: int,
) -> list[tuple[int, float, float]]:
    if len(centers) < 2:
        return centers
    remaining = [(line_id, _wrap_angle(theta), strength) for line_id, theta, strength in centers]
    while len(remaining) > 1:
        ordered = sorted(remaining, key=lambda item: item[1])
        gaps = []
        for idx, (_line_id, theta, _strength) in enumerate(ordered):
            next_theta = ordered[(idx + 1) % len(ordered)][1]
            gap = (next_theta - theta) % (2.0 * np.pi)
            gaps.append(gap * radius)
        min_idx = int(np.argmin(gaps))
        if gaps[min_idx] >= min_gap_mm:
            return ordered

        a = ordered[min_idx]
        b = ordered[(min_idx + 1) % len(ordered)]
        pa = _hash01(bark_seed, "bark-priority", edge_id, a[0])
        pb = _hash01(bark_seed, "bark-priority", edge_id, b[0])
        drop = a if pa < pb else b
        remaining = [item for item in ordered if item[0] != drop[0]]
    return remaining


def _bark_cut(
    theta_v: float,
    radius: float,
    bark: BarkConfig | None,
    groove_centers: list[tuple[int, float, float]],
) -> float:
    if bark is None or not groove_centers:
        return 0.0
    cut = 0.0
    for _line_id, theta_g, strength in groove_centers:
        half_width = 0.5 * bark.width_mm * strength
        if half_width <= 1e-9:
            continue
        d_mm = radius * abs(_wrap_angle_signed(theta_v - theta_g))
        if d_mm < half_width:
            cut = max(cut, bark.depth_mm * strength * (1.0 - d_mm / half_width))
    return cut


def _bark_surface_noise(
    theta_v: float,
    radius: float,
    bark: BarkConfig | None,
    groove_centers: list[tuple[int, float, float]],
    *,
    s: float,
    edge_id: int,
    bark_seed: int,
) -> float:
    if bark is None or bark.roughness_amplitude_mm <= 1e-9:
        return 0.0
    if not groove_centers:
        return 0.0
    if bark.roughness_cell_mm <= 1e-9:
        return 0.0
    if _bark_cut(theta_v, radius, bark, groove_centers) > 1e-9:
        return 0.0

    theta_cell = int(np.floor(radius * _wrap_angle(theta_v) / bark.roughness_cell_mm))
    s_cell = int(np.floor(s / bark.roughness_cell_mm))
    u1 = max(_hash01(bark_seed, "bark-rough-u1", edge_id, s_cell, theta_cell), 1e-12)
    u2 = _hash01(bark_seed, "bark-rough-u2", edge_id, s_cell, theta_cell)
    normal = np.sqrt(-2.0 * np.log(u1)) * np.cos(2.0 * np.pi * u2)
    max_amp = min(float(bark.roughness_amplitude_mm), 0.45 * float(bark.depth_mm))
    return float(np.clip(normal, -2.0, 2.0) * max_amp)


def _hash01(*parts: object) -> float:
    h = 1469598103934665603
    for part in parts:
        for byte in str(part).encode("utf-8"):
            h ^= byte
            h = (h * 1099511628211) & 0xFFFFFFFFFFFFFFFF
        h ^= 0xFF
        h = (h * 1099511628211) & 0xFFFFFFFFFFFFFFFF
    # Final avalanche (murmur3 fmix64).  Without this, FNV barely diffuses the
    # trailing bytes, so varying only the last argument (e.g. a column index)
    # leaves the high bits almost unchanged and biased — collapsing per-leaf
    # jitter into a regular grid.  The finalizer spreads every input bit across
    # all 64 output bits, giving a centred, full-range uniform.
    h ^= h >> 33
    h  = (h * 0xFF51AFD7ED558CCD) & 0xFFFFFFFFFFFFFFFF
    h ^= h >> 33
    h  = (h * 0xC4CEB9FE1A85EC53) & 0xFFFFFFFFFFFFFFFF
    h ^= h >> 33
    return h / float(2 ** 64)


def _wrap_angle(theta: float) -> float:
    return float(theta % (2.0 * np.pi))


def _wrap_angle_signed(theta: float) -> float:
    return float((theta + np.pi) % (2.0 * np.pi) - np.pi)


# ── FDM angle warning ─────────────────────────────────────────────────────────

def _warn_if_branch_below_strict_fdm_angle(
    nodes: np.ndarray,
    parents: np.ndarray,
    strict_fdm_angle_deg: float,
) -> None:
    threshold = float(strict_fdm_angle_deg)
    offenders: list[tuple[float, int, int]] = []
    for i in range(1, len(nodes)):
        p = int(parents[i])
        if p < 0:
            continue
        edge     = np.asarray(nodes[i], float) - np.asarray(nodes[p], float)
        edge_len = float(np.linalg.norm(edge))
        if edge_len < 1e-9:
            continue
        sin_e         = float(np.clip(edge[2] / edge_len, -1.0, 1.0))
        elevation_deg = float(np.degrees(np.arcsin(sin_e)))
        if elevation_deg < threshold - 1e-6:
            offenders.append((elevation_deg, p, i))

    if not offenders:
        return
    worst_elev, worst_parent, worst_child = min(offenders, key=lambda x: x[0])
    warnings.warn(
        "CloudTree FDM print failure: branch below strict FDM angle during "
        f"mesh creation: strict={threshold:.2f} deg above horizon, "
        f"worst={worst_elev:.2f} deg on edge {worst_parent}->{worst_child}; "
        f"{len(offenders)} branch(es) violated the strict limit.",
        RuntimeWarning,
        stacklevel=2,
    )


# ── Geometry helpers ──────────────────────────────────────────────────────────

def _bezier_eval(
    p0: np.ndarray, p1: np.ndarray, p2: np.ndarray, p3: np.ndarray,
    ts: np.ndarray,
) -> np.ndarray:
    t = ts[:, None]
    return (
        (1 - t) ** 3 * p0
        + 3 * (1 - t) ** 2 * t * p1
        + 3 * (1 - t) * t ** 2 * p2
        + t ** 3 * p3
    )


def _bezier_tangent(
    p0: np.ndarray, p1: np.ndarray, p2: np.ndarray, p3: np.ndarray,
    t: float,
) -> np.ndarray:
    return (
        3.0 * (1.0 - t) ** 2 * (p1 - p0)
        + 6.0 * (1.0 - t) * t * (p2 - p1)
        + 3.0 * t ** 2 * (p3 - p2)
    )


def _safe_norm(v: np.ndarray) -> np.ndarray:
    n = float(np.linalg.norm(v))
    return v / n if n > 1e-12 else v


def _rotate_vec(v: np.ndarray, axis: np.ndarray, angle: float) -> np.ndarray:
    """Rotate vector *v* about *axis* by *angle* radians (Rodrigues' formula)."""
    n = float(np.linalg.norm(axis))
    if n < 1e-12 or abs(angle) < 1e-12:
        return v
    k = axis / n
    c, s = float(np.cos(angle)), float(np.sin(angle))
    return v * c + np.cross(k, v) * s + k * float(np.dot(k, v)) * (1.0 - c)


def _two_perp(axis: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
    """Two orthonormal vectors perpendicular to *axis*.

    The first is biased toward world-up (so it points "up" across the surface,
    giving leaves a consistent droop reference); the second completes a
    right-handed frame.  Falls back to an arbitrary perpendicular when *axis*
    is itself vertical.
    """
    a  = _safe_norm(axis)
    e1 = _WUP_VEC - float(np.dot(_WUP_VEC, a)) * a
    n1 = float(np.linalg.norm(e1))
    if n1 < 1e-6:
        ref = np.array([1.0, 0.0, 0.0])
        e1  = ref - float(np.dot(ref, a)) * a
        n1  = float(np.linalg.norm(e1))
    e1 = e1 / max(n1, 1e-12)
    e2 = np.cross(a, e1)
    e2 = e2 / max(float(np.linalg.norm(e2)), 1e-12)
    return e1, e2


_WUP_VEC = np.array([0.0, 0.0, 1.0])


def _transport(
    u: np.ndarray, v: np.ndarray, new_t: np.ndarray,
) -> tuple[np.ndarray, np.ndarray]:
    u_new = u - float(np.dot(u, new_t)) * new_t
    nn    = float(np.linalg.norm(u_new))
    if nn < 1e-10:
        return _basis(new_t)
    u_new /= nn
    v_new  = np.cross(new_t, u_new)
    v_new /= float(np.linalg.norm(v_new)) + 1e-12
    return u_new, v_new


def _make_ring(
    center: np.ndarray, radius: float,
    u: np.ndarray, v: np.ndarray,
    theta_start: float = 0.0,
    *,
    n_sides: int = _N_SIDES,
    bark: BarkConfig | None = None,
    groove_centers: list[tuple[int, float, float]] | None = None,
    s: float = 0.0,
    edge_id: int = 0,
    bark_seed: int = 0,
) -> np.ndarray:
    theta  = np.linspace(theta_start, theta_start + 2.0 * np.pi, n_sides, endpoint=False)
    grooves = groove_centers or []
    cuts = np.array(
        [
            _bark_cut(float(t), radius, bark, grooves)
            for t in theta
        ],
        dtype=float,
    )
    noise = np.array(
        [
            _bark_surface_noise(
                float(t),
                radius,
                bark,
                grooves,
                s=s,
                edge_id=edge_id,
                bark_seed=bark_seed,
            )
            for t in theta
        ],
        dtype=float,
    )
    if bark is not None:
        min_safe_radius = max(0.42, radius - 0.35 * radius)
        max_safe_radius = radius + min(float(bark.roughness_amplitude_mm), 0.45 * float(bark.depth_mm))
        radius_v = np.clip(radius - cuts + noise, min_safe_radius, max_safe_radius)
    else:
        radius_v = np.full_like(theta, radius, dtype=float)
    circle = np.cos(theta)[:, None] * u + np.sin(theta)[:, None] * v
    return center + radius_v[:, None] * circle


def _basis(w: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
    ref = np.array([0.0, 0.0, 1.0])
    if abs(float(np.dot(w, ref))) > 0.9:
        ref = np.array([1.0, 0.0, 0.0])
    u = np.cross(w, ref)
    u /= float(np.linalg.norm(u)) + 1e-12
    v = np.cross(w, u)
    v /= float(np.linalg.norm(v)) + 1e-12
    return u, v
