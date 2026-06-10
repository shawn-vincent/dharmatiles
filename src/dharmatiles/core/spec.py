"""
TileSpec: dataclasses for the Python tile specification format, plus loader.

A .tile.py file is executed as Python; it must bind a module-level variable
named ``tile`` to a TileSpec instance.  All types needed to build a spec are
importable from this module::

    from dharmatiles.core.spec import (
        TileSpec, RegionSpec, LayerSpec,
        BoundarySpec, BoundaryLayerSpec,
        SurfaceConfig,
    )

Height defaults by layer type
------------------------------
height_mm is the total slab thickness from the flat bottom face to the
region surface.  When two adjacent regions have different heights the
boundary slope interpolates between them.

    grass_carpet / soil_carpet  →  5.0 mm   (natural ground level)
    water                       →  3.0 mm   (2 mm below ground — the depression)
    floor                       → 10.0 mm   (raised masonry / dungeon floor)

Override ``height_mm`` on the region to depart from the default.
"""
from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from .config import SurfaceConfig, SpeciesConfig

# Re-export so tile files need only one import line.
__all__ = [
    'TileSpec', 'RegionSpec', 'LayerSpec',
    'BoundarySpec', 'BoundaryLayerSpec',
    'SurfaceConfig', 'SpeciesConfig',
    'HEIGHT_DEFAULTS',
]


# ── Height defaults by layer type ─────────────────────────────────────────────

HEIGHT_DEFAULTS: dict[str, float] = {
    'grass_carpet': 5.0,
    'soil_carpet':  5.0,
    'grass':        5.0,   # 3D-only grass region (no carpet layer)
    'water':        3.0,
    'floor':        10.0,
    'wall':         5.0,   # base slab; wall geometry rises above this
}


# ── Spec dataclasses ─────────────────────────────────────────────────────────

@dataclass
class LayerSpec:
    """One content layer sprayed into a region."""
    type:   str  # "grass_carpet" | "soil_carpet" | "grass" | "rocks" | "water" | "floor" | …
    params: dict = field(default_factory=dict)   # forwarded to the layer config


@dataclass
class RegionSpec:
    """A named contiguous area of the tile with its own content."""
    id:        str
    contains:  tuple[float, float]     # normalised (x, y) — must lie inside
    layers:    list[LayerSpec] = field(default_factory=list)
    height_mm: float | None = None     # None → use HEIGHT_DEFAULTS for first layer type

    @property
    def effective_height_mm(self) -> float:
        if self.height_mm is not None:
            return self.height_mm
        if self.layers:
            return HEIGHT_DEFAULTS.get(self.layers[0].type, 5.0)
        return 5.0  # bare soil default


@dataclass
class BoundaryLayerSpec:
    """Content type that fills a boundary strip (slope, river channel, …).

    The strip *width* is a property of the BoundarySpec, not this layer.
    This class only describes what terrain type lives inside the strip.
    """
    type:   str = 'soil_carpet'                # "soil_carpet" | "water" | "rocks" | …
    params: dict = field(default_factory=dict) # forwarded to layer config


@dataclass
class BoundarySpec:
    """A curve that divides the tile into two regions.

    width_mm = 0 (default) → zero-width dividing line; no layer is allowed.
    width_mm > 0           → physical strip; a layer spec defines its content.

    Assigning a layer to a zero-width boundary is an error: you cannot put
    content into a strip that has no extent.
    """
    id:          str
    from_anchor: tuple[str, float]   # (edge, t)  edge ∈ {top, bottom, left, right}
    to_anchor:   tuple[str, float]   # (edge, t)
    path:        str   = 'organic'   # "organic" | "straight"
    amplitude_mm:  float = 3.6
    wavelength_mm: float = 10.0      # organic smoothness/correlation length
    detail_fraction: float = 0.25    # relative amplitude of 4× detail layer (0 = off)
    seed_offset:   int   = 0
    width_mm:      float = 0.0       # strip width in mm; 0 = zero-width (no layer)
    layers: list[BoundaryLayerSpec] = field(default_factory=list)

    def __post_init__(self) -> None:
        if self.layers and self.width_mm <= 0.0:
            raise ValueError(
                f"Boundary '{self.id}': a layer requires width_mm > 0. "
                f"Set width_mm on the boundary, not inside the layer."
            )


@dataclass
class TileSpec:
    """Complete specification for one tile."""
    surface:    SurfaceConfig
    regions:    list[RegionSpec]    = field(default_factory=list)
    boundaries: list[BoundarySpec]  = field(default_factory=list)
    sizes:      list[tuple[int, int]] = field(default_factory=lambda: [(1, 1)])
    # TODO: variants — future upgrade path for per-size overrides (seed, density, etc.)
    #   Each entry could override surface params for one specific size, e.g.:
    #   variants: [{size: 3x3, seed: 99, groups_per_square: 180}]


# ── Loader ────────────────────────────────────────────────────────────────────

def load_spec(path: Path) -> TileSpec:
    """Load a .tile.py Python spec file and return a TileSpec."""
    path = Path(path)
    if path.suffix != '.py':
        raise ValueError(
            f"{path}: only .tile.py Python specs are supported "
            f"(YAML .tile files have been retired)."
        )
    ns: dict[str, Any] = {}
    exec(compile(path.read_text(), path, 'exec'), ns)
    spec = ns.get('tile')
    if not isinstance(spec, TileSpec):
        raise ValueError(f"{path}: Python spec must bind a TileSpec to 'tile'")
    return spec
