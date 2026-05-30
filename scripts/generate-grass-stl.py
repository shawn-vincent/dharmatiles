#!/usr/bin/env python3
"""CLI wrapper — delegates entirely to dharmatiles.terrains.grass_tile.main().

Run without installing:
    python scripts/generate-grass-stl.py [options]

Or after `pip install -e .`:
    generate-grass-stl [options]
"""
import sys
import pathlib

# Allow running directly from the scripts/ directory without installing the package.
sys.path.insert(0, str(pathlib.Path(__file__).parent.parent / "src"))

from dharmatiles.terrains.grass_tile import main

if __name__ == "__main__":
    main()
