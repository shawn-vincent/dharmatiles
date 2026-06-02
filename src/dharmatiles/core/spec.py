"""
TileSpec: dataclasses for the YAML tile specification format, plus loader.

A .tile file is a YAML document that describes surface dimensions, named
regions, and the boundaries between them.  Loaded into a TileSpec, it drives
``build_tile_from_spec()`` in ``dharmatiles.terrains.tile``.

Height defaults by layer type
------------------------------
height_mm is the total slab thickness from the flat bottom face to the
region surface.  When two adjacent regions have different heights the
boundary slope interpolates between them.

    grass / soil  →  5.0 mm   (natural ground level)
    water         →  3.0 mm   (2 mm below ground — the depression)
    floor         → 10.0 mm   (raised masonry / dungeon floor)

Override ``height_mm`` on the region to depart from the default.

Python escape hatch
-------------------
A .tile.py file is executed as Python; it must bind a module-level variable
named ``tile`` to a TileSpec instance.  All TileSpec dataclasses are
importable from ``dharmatiles.core.spec``.
"""
from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from .config import SurfaceConfig


# ── Height defaults by layer type ─────────────────────────────────────────────

HEIGHT_DEFAULTS: dict[str, float] = {
    'grass': 5.0,
    'soil':  5.0,
    'water': 3.0,
    'floor': 10.0,
    'wall':  5.0,   # base slab; wall geometry rises above this
}


# ── Spec dataclasses ─────────────────────────────────────────────────────────

@dataclass
class LayerSpec:
    """One content layer sprayed into a region."""
    type:   str                        # "grass" | "soil" | "water" | "floor" | …
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
    type: str = 'soil'    # terrain type: "soil" | "water" | "wall" | …


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
    amplitude_mm:  float = 3.0
    wavelength_mm: float = 10.0
    seed_offset:   int   = 0
    width_mm:      float = 0.0       # strip width in mm; 0 = zero-width (no layer)
    layer: BoundaryLayerSpec | None = None   # content of the strip; requires width_mm > 0

    def __post_init__(self) -> None:
        if self.layer is not None and self.width_mm <= 0.0:
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


# ── YAML loader ───────────────────────────────────────────────────────────────

def load_spec(path: Path) -> TileSpec:
    """Load a .tile (YAML) or .tile.py (Python) file and return a TileSpec."""
    path = Path(path)
    if path.suffix == '.py':
        return _load_python_spec(path)
    return _load_yaml_spec(path)


def _load_yaml_spec(path: Path) -> TileSpec:
    import yaml
    with open(path) as fh:
        data = yaml.safe_load(fh)
    return _parse(data)


def _load_python_spec(path: Path) -> TileSpec:
    ns: dict[str, Any] = {}
    exec(compile(path.read_text(), path, 'exec'), ns)
    spec = ns.get('tile')
    if not isinstance(spec, TileSpec):
        raise ValueError(f"{path}: Python spec must bind a TileSpec to 'tile'")
    return spec


def _parse(data: dict) -> TileSpec:
    # ── Surface ───────────────────────────────────────────────────────────────
    s = data.get('surface', {})
    surface = SurfaceConfig(
        cols            = s.get('cols', 1),
        rows            = s.get('rows', 1),
        cells_per_square= s.get('cells_per_square', 256),
        base_h          = s.get('base_h', 0.0),
        seed            = s.get('seed', 377),
        flat_terrain    = s.get('flat_terrain', True),
    )

    # ── Regions ───────────────────────────────────────────────────────────────
    regions: list[RegionSpec] = []
    for rid, rdata in data.get('regions', {}).items():
        layers = [
            LayerSpec(type=str(ld.pop('type')), params=dict(ld))
            for ld in [dict(l) for l in rdata.get('layers', [])]
        ]
        regions.append(RegionSpec(
            id        = rid,
            contains  = tuple(rdata['contains']),
            layers    = layers,
            height_mm = rdata.get('height_mm'),
        ))

    # ── Boundaries ────────────────────────────────────────────────────────────
    boundaries: list[BoundarySpec] = []
    for bid, bdata in data.get('boundaries', {}).items():
        width_mm  = float(bdata.get('width_mm', 0.0))
        bnd_layer = None
        if 'layer' in bdata:
            ld = bdata['layer']
            if 'width_mm' in ld:
                raise ValueError(
                    f"Boundary '{bid}': put width_mm on the boundary, "
                    f"not inside its layer."
                )
            bnd_layer = BoundaryLayerSpec(type=str(ld.get('type', 'soil')))
        boundaries.append(BoundarySpec(
            id            = bid,
            from_anchor   = (bdata['from']['edge'], float(bdata['from']['t'])),
            to_anchor     = (bdata['to']['edge'],   float(bdata['to']['t'])),
            path          = bdata.get('path', 'organic'),
            amplitude_mm  = float(bdata.get('amplitude_mm', 3.0)),
            wavelength_mm = float(bdata.get('wavelength_mm', 10.0)),
            seed_offset   = int(bdata.get('seed_offset', 0)),
            width_mm      = width_mm,
            layer         = bnd_layer,
        ))

    return TileSpec(surface=surface, regions=regions, boundaries=boundaries)
