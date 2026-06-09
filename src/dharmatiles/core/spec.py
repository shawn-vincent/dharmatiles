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
    type:   str = 'soil'                       # "soil" | "water" | "rocks" | …
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


# ── Size string helpers ───────────────────────────────────────────────────────

def _parse_size_string(s: str) -> tuple[int, int]:
    """Parse ``'2x3'`` or ``'2X3'`` → ``(2, 3)``."""
    parts = str(s).lower().split('x')
    if len(parts) != 2:
        raise ValueError(
            f"Invalid size string {s!r} — expected NxM format (e.g. '1x1', '3x3')"
        )
    try:
        return (int(parts[0]), int(parts[1]))
    except ValueError:
        raise ValueError(
            f"Invalid size string {s!r} — N and M must be integers"
        )


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
    _surface_defaults = SurfaceConfig()

    # ── Sizes: parse 'sizes' list, 'size' singular, or legacy 'cols'/'rows' ──
    if 'sizes' in s:
        sizes: list[tuple[int, int]] = [_parse_size_string(sz) for sz in s['sizes']]
    elif 'size' in s:
        sizes = [_parse_size_string(s['size'])]
    elif 'cols' in s or 'rows' in s:
        sizes = [(int(s.get('cols', 1)), int(s.get('rows', 1)))]
    else:
        sizes = [(1, 1)]

    first_cols, first_rows = sizes[0]

    surface = SurfaceConfig(
        cols            = first_cols,
        rows            = first_rows,
        cells_per_square= s.get('cells_per_square', _surface_defaults.cells_per_square),
        base_h          = s.get('base_h',           _surface_defaults.base_h),
        seed            = s.get('seed',             _surface_defaults.seed),
        flat_terrain    = s.get('flat_terrain',     _surface_defaults.flat_terrain),
        terrain_simplify_threshold = s.get(
            'terrain_simplify_threshold',
            _surface_defaults.terrain_simplify_threshold),
        terrain_simplify_stride    = s.get(
            'terrain_simplify_stride',
            _surface_defaults.terrain_simplify_stride),
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
        bnd_layers: list[BoundaryLayerSpec] = []
        if 'layer' in bdata:
            ld = bdata['layer']
            if 'width_mm' in ld:
                raise ValueError(
                    f"Boundary '{bid}': put width_mm on the boundary, "
                    f"not inside its layer."
                )
            params = dict(ld)
            layer_type = str(params.pop('type', 'soil'))
            bnd_layers.append(BoundaryLayerSpec(type=layer_type, params=params))
        for raw_layer in bdata.get('layers', []):
            ld = dict(raw_layer)
            if 'width_mm' in ld:
                raise ValueError(
                    f"Boundary '{bid}': put width_mm on the boundary, "
                    f"not inside its layers."
                )
            layer_type = str(ld.pop('type'))
            bnd_layers.append(BoundaryLayerSpec(type=layer_type, params=ld))
        boundaries.append(BoundarySpec(
            id              = bid,
            from_anchor     = (bdata['from']['edge'], float(bdata['from']['t'])),
            to_anchor       = (bdata['to']['edge'],   float(bdata['to']['t'])),
            path            = bdata.get('path', 'organic'),
            amplitude_mm    = float(bdata.get('amplitude_mm', 3.6)),
            wavelength_mm   = float(bdata.get('wavelength_mm', 10.0)),
            detail_fraction = float(bdata.get('detail_fraction', 0.25)),
            seed_offset     = int(bdata.get('seed_offset', 0)),
            width_mm        = width_mm,
            layers          = bnd_layers,
        ))

    return TileSpec(surface=surface, regions=regions, boundaries=boundaries, sizes=sizes)
