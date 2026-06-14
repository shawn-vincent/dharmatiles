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
) -> tuple[np.ndarray, np.ndarray]:
    """Compute per-node (corrected_normal, binormal) using bisector tangents.

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

    return normals_out, binormals


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
    pt_normals             = compute_frames(nodes_xyz, parents)
    normals, binormals     = _compute_node_frames(nodes_xyz, parents, pt_normals)

    N = len(nodes_xyz)
    children: list[list[int]] = [[] for _ in range(N)]
    for i in range(1, N):
        p = int(parents[i])
        if p >= 0:
            children[p].append(i)

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

    # ── One capped frustum per skeleton edge ───────────────────────────────────
    #
    # Each edge (parent → child) is built as a fully independent watertight
    # tube: side strip + base disc cap + tip disc cap.  merge_vertices() is
    # called on each piece so that the strip's ring boundaries weld to their
    # respective caps, making the piece genuinely watertight.
    #
    # No vertex sharing occurs between pieces, so at branching junctions the
    # frustums of sibling branches overlap geometrically but remain topologically
    # independent.  Trimesh's is_watertight check is edge-valence only, so the
    # concatenated mesh reports watertight without needing a boolean union.
    # Slicers treat the overlapping closed volumes as a union naturally.
    #
    # The root edge uses a base cap sunk cfg.sink mm below terrain so the trunk
    # extends into the ground and leaves no gap at the soil surface.
    for i in range(1, N):
        p = int(parents[i])
        if rings[p] is None or rings[i] is None:
            continue

        if p == 0:
            base_center = nodes_xyz[0].copy()
            base_center[2] -= cfg.sink
        else:
            base_center = nodes_xyz[p].copy()

        piece = trimesh.util.concatenate([
            _side_strip(rings[p], rings[i]),
            _fan_cap(rings[p], base_center, flip=True),
            _fan_cap(rings[i], nodes_xyz[i], flip=False),
        ])
        piece.merge_vertices()
        parts.append(piece)

    if not parts:
        return trimesh.Trimesh(process=False)

    result = trimesh.util.concatenate(parts)
    result.fix_normals()
    return result
