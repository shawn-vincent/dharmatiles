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
        """Sample positions, build seeds (sorted big→small), return mesh parts.

        Stamps ``scene.terrain_support_z`` and ``scene.obstacle_mask`` in place.
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

        seeds = [self._make_seed(x, y, rng) for x, y, _gd in positions]
        seeds.sort(key=lambda s: s.sort_key())
        if not seeds:
            return []

        # Pre-compute terrain gradient for slope-aligned rock rotation.
        cw         = surface.cell_w
        terrain_gz_x = np.gradient(scene.terrain_z, axis=1) / cw
        terrain_gz_y = np.gradient(scene.terrain_z, axis=0) / cw

        from ..layers.rocks import _build_rocks_mesh_from_seeds
        from ..core.color import Material, tag as _tag
        mesh = _build_rocks_mesh_from_seeds(
            seeds, self.rocks, surface,
            scene.terrain_z, scene.terrain_support_z, scene.obstacle_mask,
            layer_idx    = layer_idx,
            terrain_gz_x = terrain_gz_x,
            terrain_gz_y = terrain_gz_y,
        )
        if len(mesh.vertices) > 0:
            _tag(mesh, Material.ROCK)
            return [mesh]
        return []

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
