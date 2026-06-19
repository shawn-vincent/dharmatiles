# stl/openlock/

OpenLOCK-compatible terrain tiles (25.4 mm square grid, T-slot underside).

This directory is empty in the repository. STL files are generated locally and
not committed — they are large binary files that reproduce exactly from source.

To generate:

```bash
dharmatiles-gen
```

Or for a single tile:

```bash
dharmatiles-gen --tile src/tiles/ground/1x1-soil+grass.tile.py
```

Files are written to `ground/` and `water/` subdirectories mirroring
`src/tiles/`.
