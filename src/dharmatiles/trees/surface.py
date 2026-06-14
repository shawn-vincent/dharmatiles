"""
Unified bark surface builder for the tree skeleton.

One ring is pre-computed per node using a canonical frame whose tangent
*bisects* the incoming and outgoing edge directions (the miter-joint
approach).  For a straight run the bisector equals the edge direction; at a
bend the ring splits the angle equally so quad strips on both sides of the
junction are equally un-sheared.  Edges re-use the ring vertices already
stored at both endpoints, so junctions are geometrically seamless — the
last row of quads of one segment shares its vertex positions with the first
row of the next.

Detail is gated on the node's local radius r:

  r >= ridge_min_r_mm  →  full bark: rings + ridges + wrinkles + flare
  r >= branch_min_r_mm →  plain swept ring (no ridges)
  r <  branch_min_r_mm →  node and all its edges skipped

All active nodes use the same ``az_segs`` vertex count so ring→ring quad
strips are always well-formed.

Bark ridge phase is keyed on *arc distance from root* (not global Z) so
ridges flow continuously from trunk into branches without a phase jump at
junctions.

Public API
----------
``compute_frames(nodes_xyz, parents)``
    Parallel-transport normal vectors from root to all nodes. Returns (N,3).

``build_tree_mesh(nodes_xyz, parents, radii, arc_dists, cfg, rng, tz, crown_base_z)``
    One ring per node; side strips per edge; caps at root and leaf tips.
    Returns trimesh.Trimesh.
"""
from __future__ import annotations

import numpy as np
import trimesh

from ..dist import sample


# ── Math helpers ──────────────────────────────────────────────────────────────

def _normalize(v: np.ndarray) -> np.ndarray:
    n = float(np.linalg.norm(v))
    return v / n if n > 1e-10 else np.array([0.0, 0.0, 1.0])


def _hermite_pos(t: float, p0, p1, m0, m1) -> np.ndarray:
    h00 = 2*t**3 - 3*t**2 + 1
    h10 =   t**3 - 2*t**2 + t
    h01 = -2*t**3 + 3*t**2
    h11 =   t**3 -   t**2
    return h00*p0 + h10*m0 + h01*p1 + h11*m1


def _hermite_tan(t: float, p0, p1, m0, m1) -> np.ndarray:
    dh00 = 6*t**2 - 6*t
    dh10 = 3*t**2 - 4*t + 1
    dh01 = -6*t**2 + 6*t
    dh11 = 3*t**2 - 2*t
    return dh00*p0 + dh10*m0 + dh01*p1 + dh11*m1


def _parallel_transport(v: np.ndarray, t0: np.ndarray, t1: np.ndarray) -> np.ndarray:
    """Rotate v by the rotation that takes unit vector t0 to unit vector t1."""
    axis = np.cross(t0, t1)
    sin_a = float(np.linalg.norm(axis))
    cos_a = float(np.dot(t0, t1))
    if sin_a < 1e-8:
        return v.copy() if cos_a > 0 else -v.copy()
    k = axis / sin_a
    return v * cos_a + np.cross(k, v) * sin_a + k * float(np.dot(k, v)) * (1.0 - cos_a)


# ── Frame propagation ─────────────────────────────────────────────────────────

def compute_frames(
    nodes_xyz: np.ndarray,
    parents:   np.ndarray,
) -> np.ndarray:
    """Parallel-transport a normal frame from the root to every node.

    Returns ``normals (N, 3)`` — one unit normal per node, perpendicular to
    the tangent of the incoming edge.  The root node uses an arbitrary seed
    normal.  At bifurcations each child inherits an independent frame from
    the same parent normal, so sibling branches diverge naturally.
    """
    N       = len(nodes_xyz)
    normals = np.zeros((N, 3))

    children: list[list[int]] = [[] for _ in range(N)]
    for i in range(1, N):
        p = int(parents[i])
        if p >= 0:
            children[p].append(i)

    # Root seed normal — perpendicular to direction toward first child
    if children[0]:
        t = nodes_xyz[children[0][0]] - nodes_xyz[0]
    else:
        t = np.array([0.0, 0.0, 1.0])
    tn = t / max(float(np.linalg.norm(t)), 1e-10)

    ref = np.array([1.0, 0.0, 0.0])
    if abs(float(np.dot(tn, ref))) > 0.9:
        ref = np.array([0.0, 1.0, 0.0])
    n0 = np.cross(tn, ref)
    normals[0] = n0 / np.linalg.norm(n0)

    # BFS: transport from each node to its children
    queue = list(children[0])
    while queue:
        node   = queue.pop(0)
        p      = int(parents[node])
        t_edge = nodes_xyz[node] - nodes_xyz[p]
        t_len  = float(np.linalg.norm(t_edge))
        if t_len < 1e-10:
            normals[node] = normals[p]
        else:
            t_hat = t_edge / t_len
            n     = normals[p] - float(np.dot(normals[p], t_hat)) * t_hat
            nn    = float(np.linalg.norm(n))
            normals[node] = n / nn if nn > 1e-8 else normals[p]
        queue.extend(children[node])

    return normals


def _compute_node_frames(
    nodes_xyz: np.ndarray,
    parents:   np.ndarray,
    normals:   np.ndarray,
) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    """Compute per-node (corrected_normal, binormal, bisector) using bisector tangents.

    Each node's canonical tangent bisects the incoming and outgoing edge
    directions, so the ring at a junction splits the bend angle equally
    between the two adjacent quad strips:

      root node  → outgoing direction only (no parent)
      leaf node  → incoming direction only (no children)
      internal   → ``normalize(t_in + t_out_mean)``

    For bifurcations ``t_out_mean`` is the mean unit vector toward all
    children, so a symmetric Y-fork produces a ring perpendicular to the
    continuing trunk axis.

    The parallel-transported *normal* (perpendicular to the incoming tangent)
    is then re-projected onto the plane perpendicular to the bisector tangent
    so the ring lies cleanly in that plane.

    Returns
    -------
    normals_out : (N, 3) — corrected normals, ⊥ to the bisector tangent
    binormals   : (N, 3) — ``cross(bisector, corrected_normal)``
    bisectors   : (N, 3) — unit bisector tangent at each node
    """
    N = len(nodes_xyz)

    children: list[list[int]] = [[] for _ in range(N)]
    for i in range(1, N):
        p = int(parents[i])
        if p >= 0:
            children[p].append(i)

    # ── Incoming unit tangent (parent → node) ────────────────────────────────
    t_in = np.zeros((N, 3))
    for i in range(1, N):
        p  = int(parents[i])
        d  = nodes_xyz[i] - nodes_xyz[p]
        dn = np.linalg.norm(d)
        t_in[i] = d / dn if dn > 1e-8 else np.array([0., 0., 1.])

    # ── Mean outgoing unit tangent (node → mean of children) ─────────────────
    t_out = np.zeros((N, 3))
    for i in range(N):
        if children[i]:
            avg = np.zeros(3)
            for c in children[i]:
                d  = nodes_xyz[c] - nodes_xyz[i]
                dn = np.linalg.norm(d)
                avg += d / dn if dn > 1e-8 else np.array([0., 0., 1.])
            an     = np.linalg.norm(avg)
            t_out[i] = avg / an if an > 1e-8 else np.array([0., 0., 1.])

    # ── Bisector tangent ──────────────────────────────────────────────────────
    bisectors = np.zeros((N, 3))
    for i in range(N):
        has_in  = (i > 0)
        has_out = bool(children[i])

        if has_in and has_out:
            b  = t_in[i] + t_out[i]
            bn = np.linalg.norm(b)
            # Degenerate (near-antiparallel 180° bend): fall back to incoming
            bisectors[i] = b / bn if bn > 1e-8 else t_in[i]
        elif has_in:
            bisectors[i] = t_in[i]   # leaf: incoming only
        else:
            bisectors[i] = t_out[i]  # root: outgoing only

    # ── Re-project normals onto plane ⊥ bisector, then derive binormals ───────
    normals_out = np.zeros((N, 3))
    binormals   = np.zeros((N, 3))

    for i in range(N):
        t = bisectors[i]
        n = normals[i]

        # Remove the component of n along the bisector tangent
        n_orth = n - float(np.dot(n, t)) * t
        nn = np.linalg.norm(n_orth)
        if nn < 1e-8:
            # n was nearly parallel to bisector — pick any perpendicular
            ref    = (np.array([0., 1., 0.]) if abs(t[0]) > 0.9
                      else np.array([1., 0., 0.]))
            n_orth = np.cross(t, ref)
            nn     = np.linalg.norm(n_orth)
        normals_out[i] = n_orth / nn if nn > 1e-8 else np.array([1., 0., 0.])

        b  = np.cross(t, normals_out[i])
        bn = np.linalg.norm(b)
        binormals[i] = b / bn if bn > 1e-8 else np.array([0., 1., 0.])

    return normals_out, binormals, bisectors


# ── Per-ring helpers ──────────────────────────────────────────────────────────

def _flare_mult(
    z:            float,
    tz:           float,
    crown_base_z: float,
    cfg,
) -> float:
    """Radius multiplier for root flare, based on height above terrain."""
    dz           = z - tz
    flare_height = cfg.flare_fraction * crown_base_z
    if dz >= flare_height or flare_height < 1e-6:
        return 1.0
    t_low = 1.0 - dz / flare_height     # 1 at ground, 0 at flare top
    return 1.0 + cfg.flare_amp * (t_low ** cfg.flare_power)


def _make_ring(
    center:         np.ndarray,   # (3,) ring centre
    r:              float,        # base radius at this ring
    n_vec:          np.ndarray,   # (3,) normal (unit)
    b_vec:          np.ndarray,   # (3,) binormal (unit)
    az:             int,          # azimuth vertex count
    arc_dist:       float,        # arc distance from root (for phase / twist)
    with_ridges:    bool,
    aspect:         float,
    twist_rate:     float,
    ridge_params:   list,         # [(k, amp, base_phase, drift_rate), ...]
    wrinkle_amp:    float,
    wrinkle_period: float,
    wrinkle_phase:  float,
    flare_mult:     float,
) -> np.ndarray:
    """Return (az, 3) ring vertex positions."""
    twist = arc_dist * twist_rate
    theta = 2.0 * np.pi * np.arange(az) / az

    r_eff = np.full(az, r * flare_mult)

    if with_ridges:
        ridge = np.ones(az)
        for k, amp, base_ph, drift_rt in ridge_params:
            phase  = base_ph + drift_rt * arc_dist
            ridge += amp * np.cos(k * (theta + twist) + phase)
        r_eff *= ridge

        dz = wrinkle_amp * np.sin(
            2.0 * np.pi * arc_dist / max(wrinkle_period, 0.1) + wrinkle_phase
        )
    else:
        dz = 0.0

    cos_t = np.cos(theta + twist)
    sin_t = np.sin(theta + twist)

    pts       = np.empty((az, 3))
    pts[:, 0] = center[0] + r_eff * (cos_t * n_vec[0] + sin_t * b_vec[0] * aspect)
    pts[:, 1] = center[1] + r_eff * (cos_t * n_vec[1] + sin_t * b_vec[1] * aspect)
    pts[:, 2] = (center[2]
                 + r_eff * (cos_t * n_vec[2] + sin_t * b_vec[2] * aspect)
                 + dz)
    return pts


# ── Mesh primitives ───────────────────────────────────────────────────────────

def _side_strip(ring0: np.ndarray, ring1: np.ndarray) -> trimesh.Trimesh:
    """Quad strip connecting two same-size rings.  No caps.

    Winding gives outward-facing normals for a tube travelling from ring0
    toward ring1.
    """
    az    = len(ring0)
    verts = np.vstack([ring0, ring1])   # (2*az, 3)

    faces: list[list[int]] = []
    for ai in range(az):
        a0, a1 = ai,      (ai + 1) % az
        b0, b1 = az + ai, az + (ai + 1) % az
        faces += [[a0, a1, b0], [a1, b1, b0]]

    mesh = trimesh.Trimesh(
        vertices=verts,
        faces=np.array(faces, dtype=np.int32),
        process=False,
    )
    mesh.fix_normals()
    return mesh


def _fan_cap(
    ring:   np.ndarray,
    center: np.ndarray,
    flip:   bool = False,
) -> trimesh.Trimesh:
    """Fan of triangles from ``center`` to each edge of ``ring``.

    ``flip=False``  →  normals point away from center (outward top cap).
    ``flip=True``   →  normals point toward center (outward bottom cap,
                        where center is *below* the ring).
    """
    az    = len(ring)
    verts = np.vstack([center.reshape(1, 3), ring])   # (az+1, 3)

    faces: list[list[int]] = []
    for ai in range(az):
        if flip:
            faces.append([0, 1 + (ai + 1) % az, 1 + ai])
        else:
            faces.append([0, 1 + ai, 1 + (ai + 1) % az])

    mesh = trimesh.Trimesh(
        vertices=verts,
        faces=np.array(faces, dtype=np.int32),
        process=False,
    )
    mesh.fix_normals()
    return mesh


def _build_children(parents: np.ndarray) -> list[list[int]]:
    children: list[list[int]] = [[] for _ in range(len(parents))]
    for i in range(1, len(parents)):
        p = int(parents[i])
        if p >= 0:
            children[p].append(i)
    return children


def _extract_runs(children: list[list[int]]) -> list[list[int]]:
    """Return maximal root/branch/leaf runs through unary guide nodes."""
    runs: list[list[int]] = []
    for start, child_idxs in enumerate(children):
        if start != 0 and len(child_idxs) == 1:
            continue
        for child in child_idxs:
            run = [start, child]
            cur = child
            while len(children[cur]) == 1:
                cur = children[cur][0]
                run.append(cur)
            runs.append(run)
    return runs


def _run_endpoint_tangents(
    points: np.ndarray,
    start_tangent: np.ndarray | None = None,
) -> tuple[np.ndarray, np.ndarray]:
    """Tangents for one visible branch run.

    Unary guide nodes reserve growth space, but visible geometry should not
    kink at them.  The branch is therefore one smooth span from first to last
    run node; guide nodes only influence the departure/arrival tangents.

    If ``start_tangent`` is supplied, the run leaves its base along that fixed
    direction.  Child runs use this to continue straight out of their parent
    run before bending toward their own endpoint.
    """
    if len(points) <= 1:
        t = (
            _normalize(start_tangent)
            if start_tangent is not None
            else np.array([0.0, 0.0, 1.0])
        )
        return t, t
    if len(points) == 2:
        d = _normalize(points[1] - points[0])
        t0 = _normalize(start_tangent) if start_tangent is not None else d
        return t0, d

    start_tan = _normalize(points[1] - points[0])
    end_tan = _normalize(points[-1] - points[-2])
    direct = _normalize(points[-1] - points[0])
    t0 = (
        _normalize(start_tangent)
        if start_tangent is not None
        else _normalize(start_tan + direct)
    )

    # Keep the run globally aimed at its endpoint while letting the nearest
    # guide nodes bias how it leaves and arrives.
    return t0, _normalize(end_tan + direct)


def _run_start_tangent_overrides(
    nodes_xyz: np.ndarray,
    parents: np.ndarray,
    runs: list[list[int]],
) -> dict[int, np.ndarray]:
    """Return fixed start tangents for runs that begin at branchpoints.

    A child branch should initially continue along the direction its base
    branch had at the branchpoint, then curve toward its own target.  This
    avoids the visual hard turn created when each child starts tangent to its
    own chord.
    """
    end_tangents: dict[int, np.ndarray] = {}
    for run in runs:
        if len(run) < 2:
            continue
        _, t1 = _run_endpoint_tangents(nodes_xyz[run])
        end_tangents[run[-1]] = t1

    start_tangents: dict[int, np.ndarray] = {}
    for run in runs:
        start = int(run[0])
        if start == 0:
            continue
        if start in end_tangents:
            start_tangents[start] = end_tangents[start]
            continue

        parent = int(parents[start])
        if parent >= 0:
            start_tangents[start] = _normalize(nodes_xyz[start] - nodes_xyz[parent])

    return start_tangents


# ── Public: full tree mesh ────────────────────────────────────────────────────

def build_tree_mesh(
    nodes_xyz:    np.ndarray,
    parents:      np.ndarray,
    radii:        np.ndarray,
    arc_dists:    np.ndarray,
    cfg,
    rng:          np.random.Generator,
    tz:           float,
    crown_base_z: float,
) -> trimesh.Trimesh:
    """Convert a tree skeleton into a trimesh using shared per-node rings.

    Algorithm
    ---------
    1. Compute one canonical ring per node (using the node's fixed normal +
       binormal from the incoming edge direction).
    2. For each skeleton edge, emit only the side quad strip — no caps.
    3. Add a bottom cap at the root (sunk ``cfg.sink`` mm below terrain).
    4. Add a top cap at every *effective leaf*: a node whose ring is active
       but none of its children have active rings.

    Parameters
    ----------
    nodes_xyz    : (N, 3) skeleton node positions
    parents      : (N,) int  — parent index; -1 for root
    radii        : (N,) float
    arc_dists    : (N,) float — cumulative arc distance from root to each node
    cfg          : BarkConfig
    rng          : seeded random generator (per-tree bark variation)
    tz           : terrain Z at tree base
    crown_base_z : height of crown base above tz (for flare calculation)
    """
    # ── Per-tree bark parameters ──────────────────────────────────────────────
    aspect         = float(sample(cfg.aspect,         rng))
    wrinkle_amp    = float(sample(cfg.wrinkle_amp,    rng))
    wrinkle_period = float(sample(cfg.wrinkle_period, rng))
    wrinkle_phase  = float(rng.uniform(0.0, 2.0 * np.pi))

    ridge_params: list[tuple[int, float, float, float]] = []
    for k in range(2, cfg.ridge_harmonics + 2):
        ridge_params.append((
            k,
            cfg.ridge_amp / k,
            float(rng.uniform(0.0, 2.0 * np.pi)),
            2.0 * np.pi / max(cfg.ridge_drift_mm, 0.1),
        ))

    # ── Frame propagation ──────────────────────────────────────────────────────
    # compute_frames gives parallel-transported normals (⊥ incoming tangent).
    # _compute_node_frames refines them onto bisector-tangent planes and
    # returns the corrected normals + binormals used for ring construction.
    pt_normals                      = compute_frames(nodes_xyz, parents)
    normals, binormals, bisectors   = _compute_node_frames(nodes_xyz, parents, pt_normals)

    N = len(nodes_xyz)
    children = _build_children(parents)

    # ── One ring per node ─────────────────────────────────────────────────────
    # All active nodes use cfg.az_segs so ring→ring quad strips are always
    # well-formed.  Detail (ridges/wrinkles) is determined by the node's
    # own radius, not the edge's max radius.
    rings: list[np.ndarray | None] = [None] * N
    for i in range(N):
        r = float(radii[i])
        if r < cfg.branch_min_r_mm:
            continue
        with_ridges = r >= cfg.ridge_min_r_mm
        fm = _flare_mult(float(nodes_xyz[i][2]), tz, crown_base_z, cfg)
        rings[i] = _make_ring(
            nodes_xyz[i], r, normals[i], binormals[i],
            cfg.az_segs, float(arc_dists[i]),
            with_ridges, aspect, cfg.twist_rate,
            ridge_params, wrinkle_amp, wrinkle_period, wrinkle_phase, fm,
        )

    parts: list[trimesh.Trimesh] = []

    # ── One capped curved tube per visible branch run ─────────────────────────
    #
    # The skeleton may contain unary guide nodes used only to reserve space and
    # shape a branch.  Mesh geometry is emitted only once a run reaches a leaf
    # or real branch point.  Each run is drawn as one smooth Hermite span, not
    # as piecewise geometry through every guide node.
    #
    # No vertex sharing occurs between pieces, so at branching junctions the
    # tubes of sibling branches overlap geometrically but remain topologically
    # independent.  Slicers treat the overlapping closed volumes as a union.
    #
    # The root edge uses a base cap sunk cfg.sink mm below terrain so the trunk
    # extends into the ground and leaves no gap at the soil surface.
    n_curve = max(1, cfg.curve_segs)

    runs = _extract_runs(children)
    start_tangents = _run_start_tangent_overrides(nodes_xyz, parents, runs)

    for run in runs:
        if any(rings[i] is None for i in run):
            continue

        start = run[0]
        end = run[-1]
        edge_rings: list[np.ndarray] = [rings[start]]  # type: ignore[list-item]
        run_points = nodes_xyz[run]
        p0 = nodes_xyz[start]
        p1 = nodes_xyz[end]
        chord_len = float(np.linalg.norm(p1 - p0))
        t0, t1 = _run_endpoint_tangents(run_points, start_tangents.get(start))
        m0 = t0 * cfg.curve_tension * chord_len
        m1 = t1 * cfg.curve_tension * chord_len

        n_cur = normals[start].copy()
        tan_cur = t0.copy()
        steps = max(n_curve, n_curve * (len(run) - 1))

        for k in range(1, steps + 1):
            t = k / steps
            if k == steps:
                edge_rings.append(rings[end])  # type: ignore[arg-type]
                continue

            pos = _hermite_pos(t, p0, p1, m0, m1)
            tan = _normalize(_hermite_tan(t, p0, p1, m0, m1))

            n_cur = _parallel_transport(n_cur, tan_cur, tan)
            n_cur = _normalize(n_cur - float(np.dot(n_cur, tan)) * tan)
            b_cur = np.cross(tan, n_cur)
            tan_cur = tan

            r = float(radii[start]) + t * (float(radii[end]) - float(radii[start]))
            arc_d = float(arc_dists[start]) + t * (float(arc_dists[end]) - float(arc_dists[start]))
            fm = _flare_mult(float(pos[2]), tz, crown_base_z, cfg)
            with_ridges = r >= cfg.ridge_min_r_mm

            edge_rings.append(_make_ring(
                pos, r, n_cur, b_cur, cfg.az_segs, arc_d,
                with_ridges, aspect, cfg.twist_rate,
                ridge_params, wrinkle_amp, wrinkle_period, wrinkle_phase, fm,
            ))

        if start == 0:
            base_center = nodes_xyz[0].copy()
            base_center[2] -= cfg.sink
        else:
            base_center = nodes_xyz[start].copy()

        strips = [_side_strip(edge_rings[k], edge_rings[k + 1])
                  for k in range(len(edge_rings) - 1)]
        piece = trimesh.util.concatenate([
            *strips,
            _fan_cap(edge_rings[0], base_center, flip=True),
            _fan_cap(edge_rings[-1], nodes_xyz[end], flip=False),
        ])
        piece.merge_vertices()
        parts.append(piece)

    if not parts:
        return trimesh.Trimesh(process=False)

    result = trimesh.util.concatenate(parts)
    result.fix_normals()
    return result
