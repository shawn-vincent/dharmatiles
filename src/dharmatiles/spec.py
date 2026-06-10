"""
Tile spec: executable layer-based tile specifications.

A ``.tile.py`` file is executed as Python and must bind a module-level
``tile`` variable to a :class:`Tile` instance.

The spec is the implementation language: ``Region.layers`` holds real
layer instances (``SoilCarpetLayer``, ``GrassCarpetLayer``,
``ScatterLayer``, ``WaterLayer``), and the orchestrator runs their
``apply()`` methods in the order they appear.  No string types, no
``params=dict(...)``, no phase enum.

Example::

    from dharmatiles.spec import Tile, Region, Boundary, SurfaceConfig, SpeciesConfig
    from dharmatiles.layers import SoilCarpetLayer, GrassCarpetLayer, ScatterLayer
    from dharmatiles.scatter import Rocks, Grass

    species = SpeciesConfig()
    tile = Tile(
        surface=SurfaceConfig(seed=42),
        regions=[
            Region(id='meadow', contains=(0.25, 0.5), layers=[
                GrassCarpetLayer(species=species, groups_per_square=240),
                ScatterLayer(
                    Rocks(r_min=0.8, r_max=2.2),
                    Grass(species=species, groups_per_square=24),
                ),
            ]),
            Region(id='dirt', contains=(0.75, 0.5), layers=[
                SoilCarpetLayer(),
            ]),
        ],
    )
"""
from __future__ import annotations

import importlib.util
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import ClassVar, Protocol

import numpy as np
import trimesh

from .core.config import SurfaceConfig, SpeciesConfig


__all__ = [
    'Tile', 'Region', 'Boundary', 'TileLayer',
    'SurfaceConfig', 'SpeciesConfig',
    'load_spec',
]


class TileLayer(Protocol):
    """A content layer applied to a region or boundary.

    Layers are run in spec order by the orchestrator.  Each layer may
    mutate the scene (``terrain_z``, ``terrain_support_z``, ``rock_mask``)
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


@dataclass
class Region:
    """A named contiguous area of the tile.

    ``layers`` run in the order they appear here.  ``height_mm`` falls
    back to the first layer's ``height_default_mm`` when None.
    """
    id:        str
    contains:  tuple[float, float]
    layers:    list = field(default_factory=list)   # list[TileLayer]
    height_mm: float | None = None

    @property
    def effective_height_mm(self) -> float:
        if self.height_mm is not None:
            return self.height_mm
        if self.layers:
            return getattr(self.layers[0], 'height_default_mm', 5.0)
        return 5.0


@dataclass
class Boundary:
    """A curve that divides the tile into two regions.

    ``width_mm = 0`` (default) → zero-width dividing line; no layers allowed.
    ``width_mm > 0`` → physical strip; layers fill the strip.
    """
    id:          str
    from_anchor: tuple[str, float]
    to_anchor:   tuple[str, float]
    path:        str   = 'organic'
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

    One ``.tile.py`` file → one ``Tile`` → one output size.  Tile size
    is set on ``surface.cols`` / ``surface.rows``.  To emit several
    sizes of the same tile, write one spec file per size.
    """
    surface:    SurfaceConfig
    regions:    list[Region]   = field(default_factory=list)
    boundaries: list[Boundary] = field(default_factory=list)


def load_spec(path: Path) -> Tile:
    """Load a ``.tile.py`` Python spec file and return its ``Tile``.

    The spec is loaded as a real Python module via ``importlib`` (not
    ``exec()``) so that:

    - the module appears in ``sys.modules`` and stack traces show its
      filename;
    - the spec file can ``import`` sibling helper modules from the same
      directory, e.g. ``from _shared import SHARED_SPECIES``;
    - tooling that introspects modules (debuggers, IDEs) sees the spec
      the way it sees any other Python file.

    The spec's containing directory is added to ``sys.path`` so plain
    absolute imports against sibling files resolve.  The module name is
    derived from the resolved path (sanitised for the few characters
    Python forbids in module names — ``+``, ``.``, etc.).
    """
    path = Path(path).resolve()
    if path.suffix != '.py':
        raise ValueError(
            f"{path}: only .tile.py Python specs are supported."
        )

    parent = str(path.parent)
    if parent not in sys.path:
        sys.path.insert(0, parent)

    mod_name = '_dharmatiles_spec_' + ''.join(
        c if c.isalnum() else '_' for c in str(path)
    )
    py_spec = importlib.util.spec_from_file_location(mod_name, path)
    if py_spec is None or py_spec.loader is None:
        raise ImportError(f"{path}: could not create module loader")
    module = importlib.util.module_from_spec(py_spec)
    sys.modules[mod_name] = module
    py_spec.loader.exec_module(module)

    tile = getattr(module, 'tile', None)
    if not isinstance(tile, Tile):
        raise ValueError(f"{path}: spec must bind a Tile to 'tile'")
    return tile
