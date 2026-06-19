# stl/

Generated STL files are not committed to this repository — they are large
binary files and are reproduced exactly from source on demand.

To generate all tiles locally:

```bash
pip install -e .
dharmatiles-gen
```

Output lands in:

```
stl/db/      DungeonBlocks base system (db — 35 mm square grid)
stl/ol/      OpenLOCK base system (ol — 25.4 mm square grid)
stl/extras/  Standalone non-terrain utilities
```

Individual tiles can be regenerated with:

```bash
dharmatiles-gen --tile src/tiles/ground/1x1-soil+grass.tile.py
```

See the project [README](../README.md) and [CLAUDE.md](../CLAUDE.md) for
full usage instructions.
