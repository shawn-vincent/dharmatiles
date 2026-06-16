"""Bezier-tube mesh builder for CloudTree skeletons.

Each (parent → child) edge is a curved tube whose cross-section rings are
parallel-transported along the Bézier path (Bishop frame), giving smooth,
twist-free curves.  The base ring of every child tube *reuses* the vertex
ring already written for its parent, so branch-point vertices are shared and
the mesh is topologically connected.  At junctions with more than one child
the shared ring is referenced by two or more child quad-strips; face overlaps
at those joints are intentional and harmless for 3D printing.

Leaf branches are NOT rendered as wood tubes.  Instead each leaf edge gets a
separate watertight foliage-cone mesh: a cone-shaped solid whose cross-section
rings are true half-circle D-shapes.  Each ring's lower semicircle arc has its
bottommost point sitting exactly on the Bézier branch path (the ring centre is
offset upward by the ring radius); the flat chord connects the two diameter
endpoints horizontally, so the bulk of the foliage billows upward and outward
while the underside tracks the branch.
"""
from __future__ import annotations

import numpy as np
import trimesh

from ..core.color import Material, debug_material, tag as _tag

# Fixed polygon count for every cross-section ring.
_N_SIDES = 12


def build_cloud_tree_mesh(
    nodes:    np.ndarray,      # (N, 3) — root + branch pts + attractors
    parents:  np.ndarray,      # (N,) int; -1 for root
    radii:    np.ndarray,      # (N,) — bottom-up pipe-model radii
    in_dirs:  np.ndarray,      # (N, 3) — tangent *arriving* at each node
    out_dirs: np.ndarray,      # (N, 3) — tangent *leaving* parent toward node
    *,
    terrain_z: float,
    handle_scale: float = 0.45,
    foliage_radius_mm: float = 4.0,
    leaf_clump_length_mm: float | None = None,
    debug_attractors: np.ndarray | None = None,
    attractor_group_labels: np.ndarray | None = None,
    attractor_radius_mm: float = 0.6,
) -> tuple[trimesh.Trimesh, list[trimesh.Trimesh], list[trimesh.Trimesh]]:
    """Build wood tube mesh + foliage cone meshes from a simplified skeleton.

    Returns
    -------
    wood_mesh
        Single connected Trimesh for the trunk and all non-leaf branches.
    attractor_meshes
        Debug icospheres (empty unless ``debug_attractors`` is set).
    foliage_meshes
        One watertight cone mesh per leaf branch, un-tagged (caller applies
        ``Material.FOLIAGE``).
    """
    n = len(nodes)

    # ── leaf classification (before any loop so we can skip leaf edges) ────
    is_leaf = np.ones(n, dtype=bool)
    for i in range(1, n):
        is_leaf[int(parents[i])] = False
    is_leaf[0] = False   # root has its own bottom cap

    # Nodes whose *every* child is a leaf need a wood tip cap.
    has_non_leaf_child = np.zeros(n, dtype=bool)
    for i in range(1, n):
        if not is_leaf[i]:
            has_non_leaf_child[int(parents[i])] = True

    wood_leaves: set[int] = set()
    for i in range(1, n):
        if is_leaf[i]:
            p = int(parents[i])
            if not has_non_leaf_child[p]:
                wood_leaves.add(p)

    # ── flat vertex / face accumulators ────────────────────────────────────
    verts_acc: list[np.ndarray] = []
    faces_acc: list[list[int]]  = []
    n_verts    = 0

    def _add_verts(arr: np.ndarray) -> int:
        nonlocal n_verts
        off = n_verts
        verts_acc.append(np.asarray(arr, float))
        n_verts += len(arr)
        return off

    # ── per-node state ─────────────────────────────────────────────────────
    node_frame: list[tuple[np.ndarray, np.ndarray] | None] = [None] * n
    ring_off:   list[int]                                   = [-1]   * n

    # ── root ──────────────────────────────────────────────────────────────
    root_in       = _safe_norm(np.asarray(in_dirs[0], float))
    u0, v0        = _basis(root_in)
    node_frame[0] = (u0, v0)
    ring_off[0]   = _add_verts(_make_ring(nodes[0], float(radii[0]), u0, v0))

    # ── edges — skip leaf edges entirely ──────────────────────────────────
    for i in range(1, n):
        if is_leaf[i]:
            continue                       # leaf branch → foliage cone, not wood

        p  = int(parents[i])
        p0 = np.asarray(nodes[p], float)
        p3 = np.asarray(nodes[i], float)
        length = float(np.linalg.norm(p3 - p0))
        pu, pv = node_frame[p]

        if length < 1e-8:
            node_frame[i] = (pu, pv)
            ring_off[i]   = _add_verts(_make_ring(nodes[i], float(radii[i]), pu, pv))
            continue

        r0 = max(float(radii[p]), 0.42)
        r1 = max(float(radii[i]), 0.42)

        t0 = _safe_norm(np.asarray(in_dirs[p], float))
        t1 = _safe_norm(np.asarray(in_dirs[i], float))
        h  = handle_scale * length
        p1 = p0 + h * t0
        p2 = p3 - h * t1

        n_steps = max(4, int(np.ceil(length / 2.5)))
        ts      = np.linspace(0.0, 1.0, n_steps + 1)
        curve   = _bezier_eval(p0, p1, p2, p3, ts)
        radii_t = r0 + (r1 - r0) * ts

        u, v = pu, pv
        step_off = [ring_off[p]]
        for j in range(1, n_steps + 1):
            tan  = _safe_norm(_bezier_tangent(p0, p1, p2, p3, ts[j]))
            u, v = _transport(u, v, tan)
            step_off.append(_add_verts(_make_ring(curve[j], radii_t[j], u, v)))

        node_frame[i] = (u, v)
        ring_off[i]   = step_off[-1]

        for j in range(n_steps):
            oa, ob = step_off[j], step_off[j + 1]
            for k in range(_N_SIDES):
                k1 = (k + 1) % _N_SIDES
                faces_acc.append([oa + k, oa + k1, ob + k1])
                faces_acc.append([oa + k, ob + k1, ob + k])

    # ── end caps ──────────────────────────────────────────────────────────
    # Root bottom (normal faces downward).
    c  = _add_verts(np.asarray(nodes[0], float)[np.newaxis])
    ro = ring_off[0]
    for k in range(_N_SIDES):
        k1 = (k + 1) % _N_SIDES
        faces_acc.append([c, ro + k1, ro + k])

    # Wood leaf tips: nodes all of whose children are leaves.
    for p in wood_leaves:
        c  = _add_verts(np.asarray(nodes[p], float)[np.newaxis])
        ro = ring_off[p]
        for k in range(_N_SIDES):
            k1 = (k + 1) % _N_SIDES
            faces_acc.append([c, ro + k, ro + k1])

    # ── foliage cones + optional exposed wood stubs (one per leaf edge) ──
    # leaf_clump_length_mm = K: the cone covers only the last min(L, K) mm of
    # each leaf branch.  The remainder (L − K, if positive) is drawn as a plain
    # wood tube that starts at the parent node and ends at the split point.
    # The cone taper rate is fixed at (foliage_radius_mm − r_wood) / K; for
    # branches shorter than K the r_end scales down proportionally.
    # leaf_clump_length_mm = None: full branch is a cone (current behaviour).
    foliage_meshes:  list[trimesh.Trimesh] = []
    extra_wood_stubs: list[trimesh.Trimesh] = []

    for i in range(1, n):
        if not is_leaf[i]:
            continue
        p = int(parents[i])
        if p < 0 or node_frame[p] is None:
            continue

        p0_a  = np.asarray(nodes[p], float)
        p3_a  = np.asarray(nodes[i], float)
        t0_a  = np.asarray(in_dirs[p], float)
        t1_a  = np.asarray(in_dirs[i], float)
        r_wood = max(float(radii[p]), 0.42)
        branch_len = float(np.linalg.norm(p3_a - p0_a))

        if leaf_clump_length_mm is not None and branch_len > 1e-8:
            K         = float(leaf_clump_length_mm)
            clump_len = min(branch_len, K)
            t_split   = (branch_len - clump_len) / branch_len
            # consistent taper rate: r_end = r_wood + (r_max - r_wood) * clump_len / K
            r_cone_end = r_wood + (foliage_radius_mm - r_wood) * (clump_len / K)
        else:
            clump_len  = branch_len
            t_split    = 0.0
            r_cone_end = float(foliage_radius_mm)

        if t_split > 1e-6:
            # Compute the split point and its tangent along the Bezier.
            t0n = _safe_norm(t0_a)
            t1n = _safe_norm(t1_a)
            h   = handle_scale * branch_len
            bp1 = p0_a + h * t0n
            bp2 = p3_a - h * t1n
            p_split   = _bezier_eval(p0_a, bp1, bp2, p3_a, np.array([t_split]))[0]
            tan_split = _safe_norm(_bezier_tangent(p0_a, bp1, bp2, p3_a, t_split))

            pu, pv = node_frame[p]
            stub = _build_exposed_wood(
                p0=p0_a, p3=p_split,
                t0=t0n,  t1=tan_split,
                radius=r_wood,
                u0=pu,   v0=pv,
                handle_scale=handle_scale,
            )
            if stub is not None and len(stub.faces) > 0:
                extra_wood_stubs.append(stub)

            cone_p0 = p_split
            cone_t0 = tan_split
        else:
            cone_p0 = p0_a
            cone_t0 = t0_a

        cone = _build_foliage_cone(
            p0=cone_p0, p3=p3_a,
            t0=cone_t0, t1=t1_a,
            r_start=r_wood, r_end=r_cone_end,
            handle_scale=handle_scale,
        )
        if cone is not None and len(cone.faces) > 0:
            foliage_meshes.append(cone)

    # ── assemble wood mesh (main skeleton + exposed stubs) ────────────────
    if faces_acc:
        verts     = np.vstack(verts_acc)
        faces     = np.array(faces_acc, dtype=np.int32)
        wood_mesh = trimesh.Trimesh(vertices=verts, faces=faces, process=False)
        for method in ("remove_duplicate_faces", "remove_degenerate_faces",
                       "remove_unreferenced_vertices"):
            fn = getattr(wood_mesh, method, None)
            if fn is not None:
                fn()
    else:
        wood_mesh = trimesh.Trimesh(process=False)

    if extra_wood_stubs:
        stubs_mesh = trimesh.util.concatenate(extra_wood_stubs)
        wood_mesh  = trimesh.util.concatenate([wood_mesh, stubs_mesh])

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

    return wood_mesh, attractor_meshes, foliage_meshes


# ─────────────────────────────────────────────────────────────────────────────
# Exposed-wood stub builder
# ─────────────────────────────────────────────────────────────────────────────

def _build_exposed_wood(
    p0: np.ndarray, p3: np.ndarray,
    t0: np.ndarray, t1: np.ndarray,
    radius: float,
    u0: np.ndarray, v0: np.ndarray,
    handle_scale: float = 0.45,
) -> trimesh.Trimesh | None:
    """Constant-radius wood tube for the exposed portion of a leaf branch.

    Starts at the parent node (*p0*) using the parent's Bishop frame (*u0*,
    *v0*) so the cross-section orientation is continuous with the main wood
    mesh.  Both caps are emitted so the stub is a watertight solid on its own.
    The start cap overlaps with the parent node's existing tip cap; the
    coincident interior faces are harmless for 3D printing.
    """
    length = float(np.linalg.norm(p3 - p0))
    if length < 1e-8:
        return None

    t0n = _safe_norm(t0)
    t1n = _safe_norm(t1)
    h   = handle_scale * length
    p1  = p0 + h * t0n
    p2  = p3 - h * t1n

    n_steps = max(2, int(np.ceil(length / 2.5)))
    ts      = np.linspace(0.0, 1.0, n_steps + 1)
    curve   = _bezier_eval(p0, p1, p2, p3, ts)

    u, v = u0, v0
    verts_acc: list[np.ndarray] = []
    faces_acc: list[list[int]]  = []
    nv = 0

    ring_offs: list[int] = []
    for j in range(n_steps + 1):
        tan = t0n if j == 0 else (t1n if j == n_steps else
              _safe_norm(_bezier_tangent(p0, p1, p2, p3, float(ts[j]))))
        if j > 0:
            u, v = _transport(u, v, tan)
        ring_offs.append(nv)
        verts_acc.append(_make_ring(curve[j], radius, u, v))
        nv += _N_SIDES

    # Lateral quads
    for j in range(len(ring_offs) - 1):
        oa, ob = ring_offs[j], ring_offs[j + 1]
        for k in range(_N_SIDES):
            k1 = (k + 1) % _N_SIDES
            faces_acc.append([oa + k, oa + k1, ob + k1])
            faces_acc.append([oa + k, ob + k1, ob + k])

    # Start cap (faces backward, away from tip)
    c_start = nv
    verts_acc.append(np.asarray(curve[0])[np.newaxis])
    nv += 1
    for k in range(_N_SIDES):
        k1 = (k + 1) % _N_SIDES
        faces_acc.append([c_start, ring_offs[0] + k1, ring_offs[0] + k])

    # End cap (faces forward, toward split point)
    c_end = nv
    verts_acc.append(np.asarray(curve[-1])[np.newaxis])
    for k in range(_N_SIDES):
        k1 = (k + 1) % _N_SIDES
        faces_acc.append([c_end, ring_offs[-1] + k, ring_offs[-1] + k1])

    verts = np.vstack(verts_acc)
    faces = np.array(faces_acc, dtype=np.int32)
    return trimesh.Trimesh(vertices=verts, faces=faces, process=False)


# ─────────────────────────────────────────────────────────────────────────────
# Foliage cone builder
# ─────────────────────────────────────────────────────────────────────────────

def _build_foliage_cone(
    p0: np.ndarray,
    p3: np.ndarray,
    t0: np.ndarray,
    t1: np.ndarray,
    r_start: float,
    r_end:   float,
    handle_scale: float = 0.45,
) -> trimesh.Trimesh | None:
    """Build a watertight foliage cone for one leaf branch.

    The cone follows the Bézier path from *p0* (parent node) to *p3*
    (attractor/leaf tip).

    - **Ring 0** (at the wood junction): full circle, vertex-aligned to the
      D-ring ordering, so the quad strip from ring 0 to ring 1 morphs smoothly
      from round to flat without any twist.
    - **Rings 1 … n** (toward the tip): true half-circle D — lower semicircle
      arc whose bottommost point sits on the branch path, plus a flat chord.

    The foliage therefore transitions seamlessly from the circular wood tube
    and then billows upward and outward along the D cross-section.

    Start cap: fan from the circle's geometric centre.
    End cap: a quarter-sphere roundover.  The bottom arc rolls forward and
    upward to the flat top plane, while the chord vertices stay on that plane.
    """
    _WORLD_UP = np.array([0.0, 0.0, 1.0])
    N = _N_SIDES

    length = float(np.linalg.norm(p3 - p0))
    if length < 1e-8:
        return None

    t0n = _safe_norm(t0)
    t1n = _safe_norm(t1)
    h   = handle_scale * length
    p1  = p0 + h * t0n
    p2  = p3 - h * t1n

    n_steps = max(4, int(np.ceil(length / 2.5)))
    ts      = np.linspace(0.0, 1.0, n_steps + 1)
    curve   = _bezier_eval(p0, p1, p2, p3, ts)
    radii_t = r_start + (r_end - r_start) * ts

    def _up_frame(tan: np.ndarray):
        """Return (up_in_plane, side) — up is world +Z projected onto ⊥tan."""
        up = _WORLD_UP - float(np.dot(_WORLD_UP, tan)) * tan
        up_len = float(np.linalg.norm(up))
        if up_len < 0.05:          # near-vertical branch — fall back to +X
            ref = np.array([1.0, 0.0, 0.0])
            up  = ref - float(np.dot(ref, tan)) * tan
            up_len = float(np.linalg.norm(up))
        up /= up_len
        side = np.cross(tan, up)
        side /= float(np.linalg.norm(side)) + 1e-12
        return up, side

    # ── Build rings ───────────────────────────────────────────────────────────
    # Ring 0 (at the wood junction): full circle, theta_start=π/2 so vertex 0
    # sits at the right diameter endpoint — the same position as vertex 0 of
    # every subsequent D-ring.  This alignment means the transition quads morph
    # smoothly from round → flat without any twist.
    # Rings 1..n_steps: true D-shape (lower semicircle + flat chord).
    # The terminal flat fan is replaced by a quarter-sphere nose, sampled as
    # smaller D-rings that move forward while their chord remains in the flat
    # top plane.
    rings:       list[np.ndarray] = []
    cap_centers: list[np.ndarray] = []
    cap_nose:    np.ndarray | None = None

    for j in range(n_steps + 1):
        if j == 0:
            tan = t0n
        elif j == n_steps:
            tan = t1n
        else:
            tan = _safe_norm(_bezier_tangent(p0, p1, p2, p3, float(ts[j])))

        up, side = _up_frame(tan)
        r        = float(radii_t[j])

        if j == 0:
            # Ring 0: full circle centred exactly on the branch path — no upward
            # offset.  Vertex ordering starts at θ=π/2 (right diameter point)
            # to align with the D-ring vertex layout that follows.
            center = curve[j]
            rings.append(_make_ring(center, r, up, side, theta_start=np.pi / 2.0))
            cap_centers.append(center)          # geometric centre of circle
        else:
            # D-rings: offset upward so the arc bottom sits on the branch path.
            center = curve[j] + r * up
            rings.append(_make_d_ring(center, r, up, side))
            cap_centers.append(center - (4.0 * r / (3.0 * np.pi)) * up)

    end_tan = t1n
    end_up, end_side = _up_frame(end_tan)
    end_r      = float(radii_t[-1])
    end_center = curve[-1] + end_r * end_up
    cap_steps  = 4
    for s in range(1, cap_steps):
        phi = (0.5 * np.pi) * (s / cap_steps)
        rings.append(_make_d_roundover_ring(end_center, end_r, end_up, end_side, end_tan, phi))
    cap_nose = end_center + end_r * end_tan

    # ── Fill vertex / face buffers ────────────────────────────────────────────
    verts_parts: list[np.ndarray] = []
    ring_offs:   list[int]        = []
    nv = 0

    for ring in rings:
        ring_offs.append(nv)
        verts_parts.append(ring)
        nv += N

    start_cap_idx = nv
    verts_parts.append(cap_centers[0][np.newaxis]);  nv += 1
    cap_nose_idx = nv
    verts_parts.append(cap_nose[np.newaxis]); nv += 1

    faces_list: list[list[int]] = []

    # Start cap — outward normal faces backward (away from tip).
    ro0 = ring_offs[0]
    for k in range(N):
        k1 = (k + 1) % N
        faces_list.append([start_cap_idx, ro0 + k1, ro0 + k])

    # Lateral quad strips.
    for j in range(len(ring_offs) - 1):
        oa, ob = ring_offs[j], ring_offs[j + 1]
        for k in range(N):
            k1 = (k + 1) % N
            faces_list.append([oa + k, oa + k1, ob + k1])
            faces_list.append([oa + k, ob + k1, ob + k])

    # Rounded end cap — outward normal faces forward (toward tip).
    ro_last = ring_offs[-1]
    for k in range(N):
        k1 = (k + 1) % N
        faces_list.append([cap_nose_idx, ro_last + k, ro_last + k1])

    verts = np.vstack(verts_parts)
    faces = np.array(faces_list, dtype=np.int32)
    return trimesh.Trimesh(vertices=verts, faces=faces, process=False)


# ─────────────────────────────────────────────────────────────────────────────
# Geometry helpers
# ─────────────────────────────────────────────────────────────────────────────

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


def _transport(
    u: np.ndarray, v: np.ndarray, new_t: np.ndarray,
) -> tuple[np.ndarray, np.ndarray]:
    u_new = u - float(np.dot(u, new_t)) * new_t
    n     = float(np.linalg.norm(u_new))
    if n < 1e-10:
        return _basis(new_t)
    u_new /= n
    v_new  = np.cross(new_t, u_new)
    v_new /= float(np.linalg.norm(v_new)) + 1e-12
    return u_new, v_new


def _make_ring(
    center: np.ndarray, radius: float,
    u: np.ndarray, v: np.ndarray,
    theta_start: float = 0.0,
) -> np.ndarray:
    theta  = np.linspace(theta_start, theta_start + 2.0 * np.pi, _N_SIDES, endpoint=False)
    circle = np.cos(theta)[:, None] * u + np.sin(theta)[:, None] * v
    return center + radius * circle


def _make_d_ring(
    center: np.ndarray, radius: float,
    up: np.ndarray, side: np.ndarray,
) -> np.ndarray:
    """True half-circle D polygon with _N_SIDES vertices.

    Layout (N = _N_SIDES):
    - Vertices 0 … N//2:   lower semicircle arc, from the right diameter
      endpoint (center + radius*side) around the bottom (center − radius*up)
      to the left endpoint (center − radius*side).  N//2 + 1 vertices total,
      including both endpoints.
    - Vertices N//2+1 … N−1:  N//2 − 1 interior points evenly spaced along
      the flat chord from left back to right (endpoints already in the arc).

    Total: (N//2 + 1) + (N//2 − 1) = N vertices.  The polygon is a convex
    half-disk; the centroid lies 4r/(3π) below the diameter toward the arc.
    """
    N       = _N_SIDES
    n_arc   = N // 2 + 1          # arc vertices including both diameter ends
    n_chord = N - n_arc            # interior chord vertices (no endpoints)

    # Arc: right → bottom → left  (θ from π/2 to 3π/2)
    theta_arc = np.linspace(np.pi / 2.0, 3.0 * np.pi / 2.0, n_arc)
    arc_pts   = center + radius * (
        np.cos(theta_arc)[:, None] * up + np.sin(theta_arc)[:, None] * side
    )

    if n_chord > 0:
        # Chord interior: left → right (arc[-1] → arc[0])
        t         = np.linspace(0.0, 1.0, n_chord + 2)[1:-1]
        chord_pts = arc_pts[-1] + t[:, None] * (arc_pts[0] - arc_pts[-1])
    else:
        chord_pts = np.empty((0, 3))

    return np.vstack([arc_pts, chord_pts])


def _make_d_roundover_ring(
    center: np.ndarray,
    radius: float,
    up: np.ndarray,
    side: np.ndarray,
    forward: np.ndarray,
    phi: float,
) -> np.ndarray:
    """D-ring cross-section sampled on a quarter-sphere terminal roundover."""
    N       = _N_SIDES
    n_arc   = N // 2 + 1
    n_chord = N - n_arc

    fwd_offset = radius * np.sin(phi)
    cap_radius = radius * np.cos(phi)
    cap_center = center + fwd_offset * forward

    theta_arc = np.linspace(np.pi / 2.0, 3.0 * np.pi / 2.0, n_arc)
    arc_pts   = cap_center + cap_radius * (
        np.cos(theta_arc)[:, None] * up + np.sin(theta_arc)[:, None] * side
    )

    if n_chord > 0:
        t         = np.linspace(0.0, 1.0, n_chord + 2)[1:-1]
        chord_pts = arc_pts[-1] + t[:, None] * (arc_pts[0] - arc_pts[-1])
    else:
        chord_pts = np.empty((0, 3))

    return np.vstack([arc_pts, chord_pts])


def _basis(w: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
    ref = np.array([0.0, 0.0, 1.0])
    if abs(float(np.dot(w, ref))) > 0.9:
        ref = np.array([1.0, 0.0, 0.0])
    u = np.cross(w, ref)
    u /= float(np.linalg.norm(u)) + 1e-12
    v = np.cross(w, u)
    v /= float(np.linalg.norm(v)) + 1e-12
    return u, v
