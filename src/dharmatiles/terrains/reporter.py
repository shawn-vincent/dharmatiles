"""
TileReporter — progress and output for the tile-generation pipeline.

Three implementations:

  SilentReporter  — no output at all (``--quiet``)
  TextReporter    — plain timestamped lines (non-TTY / pipe mode)
  RichReporter    — coloured, timed output with live spinner (default TTY)

The orchestrator (``terrains/tile.py``) creates the right reporter and passes
it down through ``build_tile_from_spec`` and ``_build_tile_mesh``.  Layer
``apply()`` calls stay with their existing ``verbose: bool`` signature;
``reporter.verbose_layers`` decides whether to enable them.
"""
from __future__ import annotations

import pathlib
import time
from typing import Sequence


# ── Base ──────────────────────────────────────────────────────────────────────

class TileReporter:
    """Protocol base — all methods are no-ops in the default implementation."""

    #: Whether to pass ``verbose=True`` to layer ``apply()`` calls.
    #: RichReporter sets this False to suppress noisy sub-layer prints.
    verbose_layers: bool = False

    # ── Tile-level events ────────────────────────────────────────────────────

    def tile_begin(
        self,
        name:         str,
        cols:         int,
        rows:         int,
        grid_w:       int,
        grid_h:       int,
        region_ids:   Sequence[str],
        boundary_ids: Sequence[str],
    ) -> None:
        pass

    def tile_end(self, elapsed: float) -> None:
        pass

    # ── Per-step events ──────────────────────────────────────────────────────

    def step_begin(self, label: str) -> None:
        pass

    def step_end(self, label: str, elapsed: float, detail: str = "") -> None:
        pass

    # ── Per-scale events ─────────────────────────────────────────────────────

    def rebuild_begin(self, square_mm: float) -> None:
        """Called when the pipeline rebuilds the scene at a secondary scale."""
        pass

    # ── Export event ─────────────────────────────────────────────────────────

    def export_done(
        self,
        suffix:     str,
        path:       pathlib.Path,
        n_verts:    int,
        n_faces:    int,
        watertight: bool,
        elapsed:    float,
    ) -> None:
        pass

    # ── Batch events ─────────────────────────────────────────────────────────

    def batch_begin(self, n_specs: int) -> None:
        pass

    def batch_spec_begin(self, spec_name: str) -> None:
        """Called before building a spec; reporters reset per-spec output tracking here."""
        pass

    def batch_spec_done(self, spec_name: str, elapsed: float) -> None:
        """Called after all tiles in a spec file have been built and exported."""
        pass

    def batch_end(self, n_specs: int, elapsed: float) -> None:
        pass


# ── Silent ────────────────────────────────────────────────────────────────────

class SilentReporter(TileReporter):
    """Produces no output — used with ``--quiet``."""
    pass


# ── Text (plain) ──────────────────────────────────────────────────────────────

class TextReporter(TileReporter):
    """Line-oriented plain-text output; safe for pipes and non-TTY environments."""

    verbose_layers: bool = False

    # ── Tile ─────────────────────────────────────────────────────────────────

    def tile_begin(self, name, cols, rows, grid_w, grid_h,
                   region_ids, boundary_ids) -> None:
        print(f"\n{'─'*60}")
        print(f"  {name}  ({cols}×{rows} squares, grid {grid_w}×{grid_h})")
        print(f"{'─'*60}")
        if region_ids:
            print(f"  regions:    {list(region_ids)}")
        if boundary_ids:
            print(f"  boundaries: {list(boundary_ids)}")

    def tile_end(self, elapsed: float) -> None:
        print(f"  tile done in {elapsed:.1f}s")

    # ── Steps ────────────────────────────────────────────────────────────────

    def step_end(self, label: str, elapsed: float, detail: str = "") -> None:
        detail_str = f"  {detail}" if detail else ""
        print(f"  ✓ {label:<38} {elapsed:.2f}s{detail_str}")

    # ── Rebuild ──────────────────────────────────────────────────────────────

    def rebuild_begin(self, square_mm: float) -> None:
        print(f"\n  ── Rebuilding at {square_mm} mm/sq ──")

    # ── Export ───────────────────────────────────────────────────────────────

    def export_done(self, suffix, path, n_verts, n_faces, watertight, elapsed) -> None:
        wt = "watertight" if watertight else "NOT watertight"
        label = f"Export [{suffix}]"
        detail = f"{wt}  {n_verts:,} verts · {n_faces:,} faces  → {path}"
        self.step_end(label, elapsed, detail)

    # ── Batch ────────────────────────────────────────────────────────────────

    def batch_end(self, n_specs: int, elapsed: float) -> None:
        print(f"\n{n_specs} spec{'s' if n_specs != 1 else ''} "
              f"processed in {elapsed:.1f}s  ({elapsed/max(n_specs,1):.1f}s/spec)")


# ── Rich ──────────────────────────────────────────────────────────────────────

class RichReporter(TileReporter):
    """Rich-powered output: spinner per step, coloured stats, batch summary table."""

    verbose_layers: bool = False

    def __init__(self) -> None:
        from rich.console import Console
        self._console = Console(highlight=False)
        self._status  = None          # active rich Status context
        self._t0_tile:  float | None  = None
        self._t0_batch: float | None  = None
        self._batch_rows: list[dict]  = []
        self._current_spec: str       = ""
        self._current_outputs: list[dict] = []

    # ── Internal ─────────────────────────────────────────────────────────────

    def _stop_status(self) -> None:
        if self._status is not None:
            self._status.__exit__(None, None, None)
            self._status = None

    def _start_status(self, label: str) -> None:
        from rich.status import Status
        self._status = Status(f"  {label}", console=self._console, spinner="dots")
        self._status.__enter__()

    def _time_color(self, elapsed: float) -> str:
        if elapsed < 2.0:
            return "dim white"
        if elapsed < 10.0:
            return "yellow"
        return "bold red"

    # ── Tile ─────────────────────────────────────────────────────────────────

    def tile_begin(self, name, cols, rows, grid_w, grid_h,
                   region_ids, boundary_ids) -> None:
        from rich.rule import Rule
        from rich.text import Text

        self._t0_tile = time.perf_counter()
        self._current_outputs = []

        region_str = ", ".join(region_ids) if region_ids else "—"
        bnd_str    = ", ".join(boundary_ids) if boundary_ids else "—"

        self._console.print()
        self._console.print(Rule(
            f"[bold cyan]{name}[/bold cyan]"
            f"  [dim]·  {cols}×{rows}  ·  {grid_w}×{grid_h} grid[/dim]",
            style="cyan",
        ))
        self._console.print(
            f"  [dim]regions:[/dim] {region_str}"
            f"   [dim]boundaries:[/dim] {bnd_str}"
        )

    def tile_end(self, elapsed: float) -> None:
        self._stop_status()
        self._console.print(
            f"  [dim]── {elapsed:.1f}s ──[/dim]"
        )

    # ── Steps ────────────────────────────────────────────────────────────────

    def step_begin(self, label: str) -> None:
        self._stop_status()
        self._start_status(label)

    def step_end(self, label: str, elapsed: float, detail: str = "") -> None:
        self._stop_status()
        tc = self._time_color(elapsed)
        detail_str = f"  [dim]{detail}[/dim]" if detail else ""
        self._console.print(
            f"  [green]✓[/green] {label:<38}"
            f" [{tc}]{elapsed:.2f}s[/{tc}]"
            f"{detail_str}"
        )

    # ── Rebuild ──────────────────────────────────────────────────────────────

    def rebuild_begin(self, square_mm: float) -> None:
        self._stop_status()
        self._console.print(
            f"  [dim]── rebuilding at {square_mm} mm/sq ──[/dim]"
        )

    # ── Export ───────────────────────────────────────────────────────────────

    def export_done(self, suffix, path, n_verts, n_faces, watertight, elapsed) -> None:
        self._stop_status()
        wt_icon  = "[green]●[/green]" if watertight else "[bold red]✗[/bold red]"
        wt_label = "watertight" if watertight else "NOT watertight"
        tc = self._time_color(elapsed)
        self._console.print(
            f"  [green]✓[/green] Export [{suffix}]"
            f"  {wt_icon} {wt_label}"
            f"  [dim]{n_verts:,} verts · {n_faces:,} faces[/dim]"
            f"  [{tc}]{elapsed:.2f}s[/{tc}]"
            f"  [blue]{path}[/blue]"
        )
        self._current_outputs.append(dict(
            suffix=suffix, path=path,
            n_verts=n_verts, n_faces=n_faces,
            watertight=watertight,
        ))

    # ── Batch ────────────────────────────────────────────────────────────────

    def batch_begin(self, n_specs: int) -> None:
        self._t0_batch = time.perf_counter()
        self._batch_rows = []
        self._console.print(
            f"[bold]Batch:[/bold] {n_specs} spec{'s' if n_specs != 1 else ''}"
        )

    def batch_spec_begin(self, spec_name: str) -> None:
        self._current_spec    = spec_name
        self._current_outputs = []   # reset per-spec export list

    def batch_spec_done(self, spec_name: str, elapsed: float) -> None:
        self._batch_rows.append(dict(
            name=spec_name, elapsed=elapsed, outputs=list(self._current_outputs)
        ))

    def batch_end(self, n_specs: int, elapsed: float) -> None:
        self._stop_status()
        if not self._batch_rows:
            return

        from rich.table import Table

        table = Table(show_header=True, header_style="bold dim",
                      border_style="dim", box=_MINIMAL_BOX)
        table.add_column("Spec",      style="cyan")
        table.add_column("Time",      justify="right")
        table.add_column("Verts",     justify="right", style="dim")
        table.add_column("Faces",     justify="right", style="dim")
        table.add_column("Watertight")
        table.add_column("Outputs",   style="dim")

        for row in self._batch_rows:
            outputs = row["outputs"]
            verts_str  = " / ".join(f"{o['n_verts']:,}"    for o in outputs)
            faces_str  = " / ".join(f"{o['n_faces']:,}"    for o in outputs)
            wt_str     = " ".join(
                "[green]✓[/green]" if o["watertight"] else "[red]✗[/red]"
                for o in outputs
            )
            suffix_str = " / ".join(o["suffix"] for o in outputs)
            tc = self._time_color(row["elapsed"])
            table.add_row(
                row["name"],
                f"[{tc}]{row['elapsed']:.0f}s[/{tc}]",
                verts_str,
                faces_str,
                wt_str,
                suffix_str,
            )

        self._console.print()
        self._console.print(table)
        per_spec = elapsed / max(n_specs, 1)
        self._console.print(
            f"\n[bold]{n_specs}[/bold] spec{'s' if n_specs != 1 else ''} "
            f"in [bold]{elapsed:.1f}s[/bold]"
            f"  [dim]({per_spec:.1f}s/spec)[/dim]"
        )


# ── Rich box style (compact) ──────────────────────────────────────────────────

try:
    from rich.box import Box as _Box
    _MINIMAL_BOX = _Box(
        "    \n"
        " ── \n"
        "    \n"
        " ── \n"
        "    \n"
        " ── \n"
        "    \n"
        "    \n"
    )
except Exception:
    _MINIMAL_BOX = None   # type: ignore[assignment]


# ── Factory ───────────────────────────────────────────────────────────────────

def make_reporter(quiet: bool = False) -> TileReporter:
    """Return the most capable available reporter.

    * ``quiet=True``    → :class:`SilentReporter`
    * TTY with rich     → :class:`RichReporter`
    * pipe / no rich    → :class:`TextReporter`
    """
    if quiet:
        return SilentReporter()
    try:
        import rich  # noqa: F401
        import sys
        if sys.stdout.isatty():
            return RichReporter()
        return TextReporter()
    except ImportError:
        return TextReporter()
