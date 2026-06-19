"""
Tile template library — canonical region and boundary factories.

Import these in ``.tile.py`` files to compose tiles from well-tuned,
named building blocks.  Existing tiles can continue to define layers
inline; these factories exist so new tiles don't repeat the same
configuration drifts.

Usage example::

    from dharmatiles.spec   import Tile, Boundary, FloodFill, SurfaceConfig, Edge
    from tiles.shared        import meadow_region, soil_region, soil_margin_boundary

    tile = Tile(
        surface=SurfaceConfig(seed=42),
        areas=[
            meadow_region(FloodFill(0.25, 0.5)),
            soil_margin_boundary(Edge.TOP(0.48), Edge.BOTTOM(0.52)),
            soil_region(FloodFill(0.75, 0.5)),
        ],
    )
"""
from __future__ import annotations

from dharmatiles.spec import Region, Boundary
from dharmatiles.spec import SpeciesConfig, D
from dharmatiles.layers import SoilCarpet, GrassCarpet, Water
from dharmatiles.scatter import Rocks, Grass, Grouped, Uniform


# ── Ground factories ──────────────────────────────────────────────────────────

def meadow_region(
    selector,
    *,
    id: str = 'meadow',
    species: SpeciesConfig | None = None,
    carpet_groups: int = 240,
    grass_groups: int = 24,
    rock_density: float = 15,
    rock_r=D[1.82:2.40].power(2.5),
    height_mm: float = 5.0,
) -> Region:
    """Grass meadow: GrassCarpet → Rocks → Grass (layer order is the contract).

    Parameters
    ----------
    selector : FloodFill
        Region seed point(s) in normalised tile coordinates.
    species : SpeciesConfig | None
        Blade geometry shared by ``GrassCarpet`` and ``Grass``.
        Defaults to ``SpeciesConfig()`` (standard floppy grass).
    carpet_groups : int
        Voronoi groups per square for the embossed 2D carpet.
    grass_groups : int
        Voronoi groups per square for the 3D blades.
    rock_density : float
        Rocks per square (uniform scatter).  0 = no rocks.
    rock_r : Sample[float]
        Rock radius distribution.
    height_mm : float
        Region terrain height above the tile base.
    """
    _species = species or SpeciesConfig()
    layers = [
        GrassCarpet(species=_species, placement=Grouped(groups_per_square=carpet_groups)),
    ]
    if rock_density > 0:
        layers.append(Rocks(r=rock_r, placement=Uniform(count_per_square=rock_density)))
    layers.append(Grass(species=_species, placement=Grouped(groups_per_square=grass_groups)))
    return Region(id=id, selector=selector, layers=layers, height_mm=height_mm)


def soil_region(
    selector,
    *,
    id: str = 'dirt',
    height_mm: float = 5.0,
) -> Region:
    """Bare soil region: SoilCarpet only.

    Parameters
    ----------
    selector : FloodFill
        Region seed point(s).
    height_mm : float
        Region terrain height above the tile base.
    """
    return Region(id=id, selector=selector, layers=[SoilCarpet()], height_mm=height_mm)


# ── Water factories ───────────────────────────────────────────────────────────

def water_pool_region(
    selector,
    *,
    id: str = 'pool',
    depth_mm: float = 3.0,
    embed_mm: float = 2.5,
    rock_density: float = 2,
    rock_r=D[3.0:5.0],
    rock_flat=D[1.725:1.86],
) -> Region:
    """Water pool with submerged rocks and a Water layer.

    Parameters
    ----------
    selector : FloodFill
        Region seed point(s).
    depth_mm : float
        Terrain height of the pool floor above the tile base.
    embed_mm : float
        Water surface displacement above the pool floor.
    rock_density : float
        Rocks per square inside the pool.  0 = no rocks.
    rock_r : Sample[float]
        Rock radius distribution.
    rock_flat : Sample[float]
        Rock flatness distribution (larger = flatter / more submerged).
    """
    layers = []
    if rock_density > 0:
        layers.append(
            Rocks(r=rock_r, flat=rock_flat, n_cuts=3,
                  placement=Uniform(count_per_square=rock_density))
        )
    layers.append(Water(embed_mm=embed_mm))
    return Region(id=id, selector=selector, layers=layers, height_mm=depth_mm)


# ── Boundary factories ────────────────────────────────────────────────────────

def shoreline_boundary(
    from_anchor,
    to_anchor,
    *,
    id: str = 'shoreline',
    width_mm: float = 2.5,
    amplitude_mm: float = 5.0,
    wavelength_mm: float = 12.0,
    rock_density: float = 60,
    rock_r=D[0.8:2.2].power(1.5),
) -> Boundary:
    """Organic shoreline between a water pool and a ground region.

    Layers: SoilCarpet + Rocks (dense scatter for a pebble-beach look).

    Parameters
    ----------
    from_anchor, to_anchor : Anchor
        Start and end points on the tile perimeter.
    width_mm : float
        Physical width of the shoreline strip.
    amplitude_mm : float
        Perlin noise amplitude for the organic path.
    wavelength_mm : float
        Perlin noise wavelength for the organic path.
    rock_density : float
        Rocks per square inside the shoreline strip.
    rock_r : Sample[float]
        Rock radius distribution.
    """
    layers = [SoilCarpet()]
    if rock_density > 0:
        layers.append(Rocks(r=rock_r, placement=Uniform(count_per_square=rock_density)))
    return Boundary(
        id=id,
        from_anchor=from_anchor,
        to_anchor=to_anchor,
        path='organic',
        amplitude_mm=amplitude_mm,
        wavelength_mm=wavelength_mm,
        width_mm=width_mm,
        layers=layers,
    )


def soil_margin_boundary(
    from_anchor,
    to_anchor,
    *,
    id: str = 'margin',
    amplitude_mm: float = 5.0,
    wavelength_mm: float = 10.0,
) -> Boundary:
    """Zero-width organic soil margin — a dividing curve with no layers.

    Use between a meadow and a soil region to create a natural edge.
    No physical strip; the boundary is just a height-blend seam.

    Parameters
    ----------
    from_anchor, to_anchor : Anchor
        Start and end points on the tile perimeter.
    amplitude_mm : float
        Perlin noise amplitude for the organic path.
    wavelength_mm : float
        Perlin noise wavelength.
    """
    return Boundary(
        id=id,
        from_anchor=from_anchor,
        to_anchor=to_anchor,
        path='organic',
        amplitude_mm=amplitude_mm,
        wavelength_mm=wavelength_mm,
        width_mm=0,
        layers=[],
    )
