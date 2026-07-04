"""
Rocks and Grass: direct tile layers that scatter elements into a region.

Each class implements the ``TileLayer`` protocol via ``apply()``, which
samples positions, sorts seeds, builds meshes, and stamps the relevant
scene support fields.  Ordering in ``Region.layers`` is the author's
contract: put ``Rocks`` before ``Grass`` so blades steer around rock
footprints already stamped into ``terrain_support_z``.
"""
from __future__ import annotations

import numpy as np
import trimesh

from ..core.config import _RocksConfig, SpeciesConfig, GrassConfig as _RuntimeGrassConfig
from ..grass.thatch import ThatchGrass as _ThatchGrass
from ..core.tile import derive_seed
from ..dist import bounds, sample
from .config import Uniform, Grouped
from .seed import RockSeed
from .distribute import scatter_positions


# ── Rocks ─────────────────────────────────────────────────────────────────────

class Rocks:
    """Scatter half-ellipsoid rocks directly into a region layer list.

    Flat kwargs set rock geometry (``r``, ``aspect``, ``flat``, ``angle``,
    ``n_cuts``, ``cut``, ``roughness``, ``az_segs``, ``el_segs``, ``sink``).
    Pass ``placement=Uniform(count_per_square=N)`` to control density;
    default is 15 rocks per square.

    Place ``Rocks`` before any ``Grass`` in the same ``Region.layers``
    so grass blades steer around already-stamped rock footprints.
    """

    height_default_mm: float = 5.0

    def __init__(self, *, placement: Uniform | None = None, **rocks_kwargs):
        self.rocks = _RocksConfig(**rocks_kwargs)
        self.placement = placement or Uniform(count_per_square=15)

    def footprint_mm(self) -> float:
        return float(bounds(self.rocks.r)[1])

    def _make_seed(self, x: float, y: float,
                   rng: np.random.Generator) -> RockSeed:
        rx     = float(sample(self.rocks.r, rng))
        ry     = rx * float(sample(self.rocks.aspect, rng))
        h_frac = float(sample(self.rocks.flat, rng))
        height = 0.5 * (rx + ry) * h_frac
        angle  = float(sample(self.rocks.angle, rng))
        return RockSeed(x=x, y=y, rx=rx, ry=ry, height=height, angle=angle)

    def scatter(
        self,
        scene,
        *,
        placement_mask: np.ndarray | None = None,
        layer_idx:      int               = 0,
    ) -> list[trimesh.Trimesh]:
        """Sample positions, build faceted stones (sorted big→small),
        return mesh parts.

        Since 2026-07-03 ``Rocks`` is an adapter over the faceted-stone
        primitive (``scatter/stones.py``, docs/design/rocks-faceted-stones.md):
        legacy ``RocksConfig`` size params map onto ``StoneSpec``s, so every
        existing tile gets bedded faceted stones with no spec edits.  The
        dome kernel in ``layers/rocks.py`` is retired from this path.
        ``n_cuts``/``cut``/``roughness``/``sink`` are accepted but ignored.

        Stamps ``scene.terrain_support_z`` and ``scene.obstacle_mask`` in
        place (via the stone build path).
        """
        surface  = scene.surface
        rng_seed = derive_seed(surface.seed, 'rocks-scatter', layer_idx) \
                   ^ self.placement.seed
        rng = np.random.default_rng(rng_seed)

        n_sq      = surface.cols * surface.rows
        positions = scatter_positions(
            self.placement, n_sq, self.footprint_mm(),
            placement_mask, scene, surface, rng,
        )
        if not positions:
            return []

        from .stones import StoneSpec, _build_and_stamp
        specs = []
        for x, y, _gd in positions:
            rx   = float(sample(self.rocks.r, rng))
            asp  = float(sample(self.rocks.aspect, rng))
            flat = float(sample(self.rocks.flat, rng))
            yaw  = float(np.degrees(sample(self.rocks.angle, rng)))
            foot = 2.0 * rx
            # Legacy height was mean-radius x flat measured from the base;
            # bed-to-widest buries part of it, so compensate (x1.45) and
            # floor at a stubby proportion so flat pebbles don't vanish.
            height = max(0.5 * (rx + rx * asp) * flat * 1.45, 0.45 * foot)
            specs.append(StoneSpec(
                x=float(x), y=float(y),
                footprint_mm=foot,
                height_mm=height,
                aspect=asp,
                # Facet count scales with size, cap raised for hero-sized
                # stones (E9: a 20 mm boulder with 14 facets shows straight
                # 10 mm silhouette lines no real rock has).
                facets=int(np.clip(6 + foot * 1.0, 7, 18)),
                yaw_deg=yaw,
                burial=float(rng.uniform(0.85, 1.05)),
                # Scatter rocks (shore pebbles, water boulders) read as
                # transported stones — well-worn, scaled to size.
                roundover_mm=float(min(rng.uniform(0.2, 0.45) * rx, 1.6)),
                seed=int(rng.integers(0, 2**31)),
            ))
        return _build_and_stamp(scene, specs)

    def apply(
        self,
        scene,
        *,
        placement_mask: np.ndarray | None = None,
    ) -> list[trimesh.Trimesh]:
        """``TileLayer`` entry point — delegates to ``scatter()``."""
        return self.scatter(scene, placement_mask=placement_mask)


# ── Grass ─────────────────────────────────────────────────────────────────────

class Grass(_ThatchGrass):
    """All grass in the system is the accepted bushy mound-thatch grass
    (Shawn, 2026-07-03): ``Grass`` is a compat shim over
    ``dharmatiles.grass.thatch.ThatchGrass``, which builds its own mound
    substrate, soil skirts against obstacles, and draped sheaf blades.

    Legacy arguments are accepted but ``species`` and ``placement`` are
    IGNORED — the accepted default species and grid placement always
    apply, so every tile gets the same approved grass.  Use
    ``ThatchGrass`` directly to experiment with a custom species.

    Keep ``Rocks``/``Tree`` before ``Grass`` in ``Region.layers`` (the
    skirt, crowding, and deflection all read the stamped obstacles).
    """

    def __init__(self, species=None, *, placement=None,
                 max_stack_height: float = 1.2):
        super().__init__(None, max_stack_height=max_stack_height)


class FloppyGrass:
    """The pre-2026-07-03 field-simulation grass (superseded by Grass /
    ThatchGrass; kept until the mound-thatch system is fully accepted —
    see docs/design/grass-mound-thatch.md keep/delete inventory).

    Unlike ``Rocks``, ``FloppyGrass`` is a *field simulation*: each blade's
    growth path depends on prior blades' shared occupancy grid.  Place
    ``Rocks`` (and any other obstacle-stamping layers) before it
    in ``Region.layers`` so blades route around existing footprints.
    """

    height_default_mm: float = 5.0

    def __init__(
        self,
        species: SpeciesConfig | None = None,
        *,
        placement: Grouped | None = None,
        max_stack_height: float = 2.0,
    ):
        self.species = species or SpeciesConfig()
        self.placement = placement or Grouped()
        self.max_stack_height = max_stack_height

    def footprint_mm(self) -> float:
        return float(bounds(self.species.blade_width)[1])

    def scatter(
        self,
        scene,
        *,
        placement_mask: np.ndarray | None = None,
        layer_idx:      int               = 0,
        verbose:        bool              = False,
    ) -> list[trimesh.Trimesh]:
        """Plant + grow blades inside *placement_mask*, return mesh parts.

        Reads ``scene.terrain_support_z`` (populated by any prior Rocks)
        and stamps ``scene.vegetation_support_z`` as blades grow.
        """
        from ..grass.grow import grow_all
        from ..grass.mesh import build_meshes

        surface = scene.surface
        seed    = (derive_seed(surface.seed, 'grass-scatter', layer_idx)
                   ^ self.placement.seed)
        grass_cfg = _RuntimeGrassConfig(
            species          = self.species,
            max_stack_height = self.max_stack_height,
            seed             = seed,
        )
        rng = np.random.default_rng(grass_cfg.seed)

        # Lift vegetation_support_z to include rock tops (and any other
        # terrain support stamped by prior layers) so the grower's
        # leading-edge sampler and the seed-depth check both see them.
        np.maximum(scene.vegetation_support_z, scene.terrain_support_z,
                   out=scene.vegetation_support_z)

        paths = grow_all(scene, surface, grass_cfg, rng, verbose=verbose,
                         placement_mask=placement_mask, placement=self.placement)
        return build_meshes(paths, grass_cfg, scene, surface)

    def apply(
        self,
        scene,
        *,
        placement_mask: np.ndarray | None = None,
    ) -> list[trimesh.Trimesh]:
        """``TileLayer`` entry point — delegates to ``scatter()``."""
        return self.scatter(scene, placement_mask=placement_mask)
