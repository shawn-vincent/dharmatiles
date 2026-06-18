"""
Tile spec: executable layer-based tile specifications.

A ``.tile.py`` file is executed as Python and must bind a module-level
``tile`` variable to a :class:`Tile` instance, **or** a ``tiles``
variable to a list of :class:`Tile` instances.

The spec is the implementation language: ``Region.layers`` holds real
layer instances (``SoilCarpet``, ``GrassCarpet``, ``Scatter``, ``Water``),
and the orchestrator runs their ``apply()`` methods in the order they appear.
No string types, no ``params=dict(...)``, no phase enum.

Example::

    from dharmatiles.spec import Tile, Region, Boundary, Edge, FloodFill
    from dharmatiles.spec import SurfaceConfig, SpeciesConfig, D, repeat_sizes
    from dharmatiles.systems import DungeonBlocks, OpenLOCK
    from dharmatiles.layers import SoilCarpet, GrassCarpet, Scatter
    from dharmatiles.scatter import Rocks, Grass, Grouped

    species = SpeciesConfig()
    _base = Tile(
        surface=SurfaceConfig(seed=42),
        areas=[
            Region(id='meadow', selector=FloodFill(0.25, 0.5), layers=[
                GrassCarpet(species=species,
                            placement=Grouped(groups_per_square=240)),
                Scatter(
                    Rocks(r=D[0.8:2.2].power(1.5)),
                    Grass(species=species,
                          placement=Grouped(groups_per_square=24)),
                ),
            ]),
            Boundary(id='margin',
                     from_anchor=Edge.TOP(0.48),
                     to_anchor=Edge.BOTTOM(0.52)),
            Region(id='dirt', selector=FloodFill(0.75, 0.5), layers=[
                SoilCarpet(),
            ]),
        ],
    )

    # Emit 1×1 and 3×3 from one spec file:
    tiles = repeat_sizes(_base, [(1, 1), (3, 3)])
"""
from __future__ import annotations

import dataclasses
import importlib.util
import sys
import types
from dataclasses import dataclass, field
from pathlib import Path
from typing import ClassVar, Protocol

import numpy as np
import trimesh

from .core.config import SurfaceConfig, SpeciesConfig
from .dist import D


__all__ = [
    'Tile', 'Region', 'Boundary', 'TileLayer',
    'Anchor', 'Edge', 'FloodFill',
    'FlatHeight',
    'SurfaceConfig', 'SpeciesConfig',
    'D',
    'repeat_sizes',
    'load_tile',
    'load_spec',  # backward-compat alias
]


class TileLayer(Protocol):
    """A content layer applied to a region or boundary.

    Layers are run in spec order by the orchestrator.  Each layer may
    mutate the scene (``terrain_z``, ``terrain_support_z``, ``obstacle_mask``)
    and/or return Trimesh parts for the final union.
    """
    height_default_mm: ClassVar[float]

    def apply(
        self,
        scene,                                  # TileScene
        *,
        placement_mask: np.ndarray | None,
    ) -> list[trimesh.Trimesh]:
        ...


# ── Boundary anchors ──────────────────────────────────────────────────────────

@dataclass(frozen=True)
class Anchor:
    """A point on the tile perimeter, given as an edge name and a 0–1 parameter.

    Use the :class:`Edge` factory class rather than constructing these directly.
    """
    edge: str   # 'top' | 'bottom' | 'left' | 'right'
    t:    float # normalised position along that edge, 0..1


class Edge:
    """Factory for :class:`Anchor` perimeter points.

    Each method returns an ``Anchor`` at the given normalised position *t*
    (0 = one corner, 1 = the other).

    Usage::

        Boundary(from_anchor=Edge.LEFT(0.5), to_anchor=Edge.RIGHT(0.5))
    """

    @staticmethod
    def TOP(t: float) -> Anchor:    return Anchor('top',    t)
    @staticmethod
    def BOTTOM(t: float) -> Anchor: return Anchor('bottom', t)
    @staticmethod
    def LEFT(t: float) -> Anchor:   return Anchor('left',   t)
    @staticmethod
    def RIGHT(t: float) -> Anchor:  return Anchor('right',  t)


# ── Region selectors ──────────────────────────────────────────────────────────

class FloodFill:
    """Select a region by BFS flood fill from one or more seed points.

    Seed points are normalised (x, y) coordinates — (0, 0) is bottom-left,
    (1, 1) is top-right of the tile.

    Single seed::

        FloodFill(0.25, 0.5)

    Multiple seeds (both flood-fill into the same region index)::

        FloodFill((0.1, 0.1), (0.9, 0.9))
    """

    def __init__(self, *args):
        if len(args) == 2 and all(isinstance(a, (int, float)) for a in args):
            # FloodFill(x, y) — two bare floats → single seed
            self.seeds: tuple[tuple[float, float], ...] = (
                (float(args[0]), float(args[1])),
            )
        else:
            # FloodFill((x1,y1), (x2,y2), ...) — explicit seed tuples
            self.seeds = tuple((float(x), float(y)) for x, y in args)
        if not self.seeds:
            raise ValueError("FloodFill requires at least one seed point")

    def __repr__(self) -> str:
        if len(self.seeds) == 1:
            x, y = self.seeds[0]
            return f"FloodFill({x}, {y})"
        return f"FloodFill({', '.join(str(s) for s in self.seeds)})"


# ── Terrain types (I) ─────────────────────────────────────────────────────────

@dataclass(frozen=True)
class FlatHeight:
    """Constant-height terrain specification for a region.

    The orchestrator reads ``height_mm`` to derive the IDW blend target for
    this region.  Future terrain types (e.g. ``GaussianMound``,
    ``HeightmapPNG``) will expose the same interface so the orchestrator can
    treat them uniformly.

    Usage::

        Region(id='pool', terrain=FlatHeight(3.0), layers=[Water()])
    """
    height_mm: float


# ── Spec dataclasses ──────────────────────────────────────────────────────────

@dataclass
class Region:
    """A named contiguous area of the tile.

    ``selector`` is a :class:`FloodFill` that seeds the BFS region detection.
    ``layers`` run in the order they appear here.

    Terrain height is set via ``terrain=FlatHeight(h)``; ``height_mm=h`` is a
    backward-compatible shortcut that is converted to ``FlatHeight(h)`` in
    ``__post_init__``.  When neither is given, ``terrain`` is derived from the
    first layer's ``height_default_mm`` (default 5.0 mm).
    """
    id:        str
    selector:  FloodFill
    layers:    list = field(default_factory=list)   # list[TileLayer]
    height_mm: float | None = None                  # backward-compat shortcut
    terrain:   FlatHeight | None = None             # explicit terrain spec (I)

    def __post_init__(self) -> None:
        if self.terrain is None:
            if self.height_mm is not None:
                self.terrain = FlatHeight(self.height_mm)
            else:
                h = (getattr(self.layers[0], 'height_default_mm', 5.0)
                     if self.layers else 5.0)
                self.terrain = FlatHeight(h)

    @property
    def effective_height_mm(self) -> float:
        """Height used for IDW blending; delegates to ``terrain.height_mm``."""
        return self.terrain.height_mm if self.terrain is not None else 5.0


@dataclass
class Boundary:
    """A curve that divides the tile into two regions.

    ``from_anchor`` and ``to_anchor`` are :class:`Anchor` instances (use
    :class:`Edge` factory methods).  Legacy ``('edge', t)`` tuples are also
    accepted.

    ``waypoints`` are optional normalised ``(x, y)`` interior control points.
    Boundaries may start and end on the same tile edge; the path plus the
    perimeter segment between anchors can carve out a flood-fillable region.

    ``width_mm = 0`` (default) → zero-width dividing line; no layers allowed.
    ``width_mm > 0`` → physical strip; layers fill the strip.
    """
    id:          str
    from_anchor: Anchor | tuple[str, float]
    to_anchor:   Anchor | tuple[str, float]
    path:        str   = 'organic'
    waypoints:   list[tuple[float, float]] = field(default_factory=list)
    amplitude_mm:    float = 3.6
    wavelength_mm:   float = 10.0
    detail_fraction: float = 0.25
    seed_offset:     int   = 0
    width_mm:        float = 0.0
    layers:          list  = field(default_factory=list)   # list[TileLayer]

    def __post_init__(self) -> None:
        if self.layers and self.width_mm <= 0.0:
            raise ValueError(
                f"Boundary '{self.id}': layers require width_mm > 0. "
                f"Set width_mm on the boundary."
            )


@dataclass
class Tile:
    """Complete specification for one tile.

    ``areas`` is an ordered list of :class:`Region` and :class:`Boundary`
    items.  The orchestrator runs each area's layers in the order they appear
    here, so you control which region's rocks are stamped before another
    region's grass grows.

    ``systems`` lists the base-attachment targets to emit (default:
    ``[DungeonBlocks(), OpenLOCK()]``).  Each system builds the tile at its
    own scale and attaches its base.
    """
    surface:  SurfaceConfig
    areas:    list = field(default_factory=list)   # list[Region | Boundary]
    systems:  list = field(default_factory=list)   # list[DungeonBlocks | OpenLOCK | ...]

    def __post_init__(self) -> None:
        if not self.systems:
            from .systems import DungeonBlocks, OpenLOCK
            self.systems = [DungeonBlocks(), OpenLOCK()]

    # Convenience views (read-only iteration; used by build helpers and tests)
    @property
    def regions(self) -> list[Region]:
        """All Region entries in ``areas``, in declaration order."""
        return [a for a in self.areas if isinstance(a, Region)]

    @property
    def boundaries(self) -> list[Boundary]:
        """All Boundary entries in ``areas``, in declaration order."""
        return [a for a in self.areas if isinstance(a, Boundary)]


# ── Multi-tile helpers ────────────────────────────────────────────────────────

def repeat_sizes(base: Tile, sizes: list[tuple[int, int]]) -> list[Tile]:
    """Return one :class:`Tile` per ``(cols, rows)`` size pair.

    Each tile is a copy of *base* with ``surface.cols`` and
    ``surface.rows`` set to the given values.  Bind the result to
    ``tiles`` in your spec file::

        tiles = repeat_sizes(tile, [(1, 1), (3, 3)])

    The orchestrator picks up both ``tile`` (single) and ``tiles`` (list)
    bindings from spec files.
    """
    return [
        dataclasses.replace(
            base,
            surface=dataclasses.replace(base.surface, cols=c, rows=r),
        )
        for c, r in sizes
    ]


def load_tile(path: Path) -> list[Tile]:
    """Load a ``.tile.py`` file and return its :class:`Tile` instance(s).

    Looks for a module-level ``tiles`` binding first (a list of
    ``Tile`` instances), then falls back to a single ``tile`` binding.
    Always returns a list — callers iterate over it.

    The tile file is loaded as a real Python module via ``importlib`` so that:

    - the module appears in ``sys.modules`` and stack traces show its
      filename;
    - the tile file can ``import`` sibling helper modules from the same
      directory, e.g. ``from . import shared_helpers``;
    - tooling that introspects modules (debuggers, IDEs) sees the tile
      the way it sees any other Python file.
    """
    path = Path(path).resolve()
    if path.suffix != '.py':
        raise ValueError(
            f"{path}: only .tile.py Python specs are supported."
        )

    parent = str(path.parent)
    if parent not in sys.path:
        sys.path.insert(0, parent)

    package_name = '_dharmatiles_spec_pkg_' + ''.join(
        c if c.isalnum() else '_' for c in str(path.parent)
    )
    package = sys.modules.get(package_name)
    if package is None:
        package = types.ModuleType(package_name)
        package.__path__ = [parent]  # type: ignore[attr-defined]
        package.__package__ = package_name
        sys.modules[package_name] = package
    else:
        package.__path__ = [parent]  # type: ignore[attr-defined]

    mod_name = package_name + '.' + ''.join(
        c if c.isalnum() else '_' for c in path.name
    )
    py_spec = importlib.util.spec_from_file_location(mod_name, path)
    if py_spec is None or py_spec.loader is None:
        raise ImportError(f"{path}: could not create module loader")
    module = importlib.util.module_from_spec(py_spec)
    sys.modules[mod_name] = module
    py_spec.loader.exec_module(module)

    # Prefer explicit ``tiles`` list, then fall back to single ``tile``
    tiles = getattr(module, 'tiles', None)
    if tiles is not None:
        if not isinstance(tiles, list) or not all(isinstance(t, Tile) for t in tiles):
            raise ValueError(
                f"{path}: 'tiles' must be a list of Tile instances"
            )
        return tiles

    tile = getattr(module, 'tile', None)
    if isinstance(tile, Tile):
        return [tile]

    raise ValueError(f"{path}: tile file must bind a Tile to 'tile' or a list to 'tiles'")


# Backward-compat alias — prefer load_tile
load_spec = load_tile
