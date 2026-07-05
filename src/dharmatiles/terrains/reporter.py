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

    def rebuild_begin(self, suffix: str, square_mm: float) -> None:
        """Called before building each system's mesh (primary and secondary scales)."""
        pass

    # ── Phase events ─────────────────────────────────────────────────────────

    def phase_header(self, label: str) -> None:
        """Print a static phase heading (for phases followed by a Live display)."""
        pass

    def phase_begin(self, label: str) -> None:
        """Begin a top-level phase with an animated spinner."""
        pass

    def phase_end(self, label: str, elapsed: float, detail: str = "") -> None:
        """Stop the phase spinner and print a bold completion line."""
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

    def batch_begin(self, n_tiles: int) -> None:
        pass

    def batch_tile_begin(self, tile_name: str) -> None:
        """Called before building a tile; reporters reset per-tile output tracking here."""
        pass

    def batch_tile_done(self, tile_name: str, elapsed: float) -> None:
        """Called after all variants in a tile file have been built and exported."""
        pass

    def batch_end(self, n_tiles: int, elapsed: float) -> None:
        pass

    def inject_batch_row(self, row: dict) -> None:
        """Inject a pre-built batch row from a parallel worker.

        Called instead of the normal tile_begin / step_end / batch_tile_done
        sequence when tiles are built in separate worker processes.
        """
        pass

    def record_batch_rows(self, rows: list) -> None:
        """Record multiple rows without printing — used after a Live display
        has already rendered visual output and we just need the data for the
        summary table in batch_end().
        """
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

    def rebuild_begin(self, suffix: str, square_mm: float) -> None:
        sq = int(square_mm) if square_mm == int(square_mm) else square_mm
        print(f"\n  ── building {suffix} at {sq}mm/sq ──")

    # ── Phases ───────────────────────────────────────────────────────────────

    def phase_header(self, label: str) -> None:
        print(f"\n── {label}")

    def phase_begin(self, label: str) -> None:
        print(f"\n── {label}…")

    def phase_end(self, label: str, elapsed: float, detail: str = "") -> None:
        suffix = f"  {detail}" if detail else ""
        print(f"✓ {label}  {elapsed:.1f}s{suffix}")

    # ── Export ───────────────────────────────────────────────────────────────

    def export_done(self, suffix, path, n_verts, n_faces, watertight, elapsed) -> None:
        wt = ("not checked" if watertight is None
              else "watertight" if watertight else "NOT watertight")
        label = f"Export [{suffix}]"
        detail = f"{wt}  {n_verts:,} verts · {n_faces:,} faces  → {path}"
        self.step_end(label, elapsed, detail)

    # ── Batch ────────────────────────────────────────────────────────────────

    def inject_batch_row(self, row: dict) -> None:
        print(f"  ✓ {row['name']:<38} {row['elapsed']:.1f}s")

    def record_batch_rows(self, rows: list) -> None:
        # TextReporter has no table, so just print completions in order.
        for row in rows:
            print(f"  ✓ {row['name']:<38} {row['elapsed']:.1f}s")

    def batch_end(self, n_tiles: int, elapsed: float) -> None:
        print(f"\n{n_tiles} tile{'s' if n_tiles != 1 else ''} "
              f"processed in {elapsed:.1f}s  ({elapsed/max(n_tiles,1):.1f}s/tile)")


# ── Animated spinner renderable ───────────────────────────────────────────────

class _SpinnerLine:
    """Rich renderable: '  ⠙ {label}' — spinner indented to match the ✓ column.

    ``rich.Status`` always places its spinner at column 0 with the label
    text to its right.  This renderable puts the two-space indent *before*
    the spinner character so the animated frame lines up with the ``✓`` that
    replaces it when the step completes.

    Passed to ``rich.Live(transient=True)`` so it is erased automatically
    when the Live context exits, leaving the cursor exactly where
    ``console.print("  ✓ …")`` will write the permanent completion line.
    """

    _FRAMES = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]
    _FPS    = 12.5   # frames per second

    def __init__(self, label: str) -> None:
        from rich.markup import escape
        self._label = escape(label)
        self._t0    = time.monotonic()

    def __rich_console__(self, console, options):
        from rich.text import Text
        elapsed = time.monotonic() - self._t0
        frame   = self._FRAMES[int(elapsed * self._FPS) % len(self._FRAMES)]
        t_str = "    s" if elapsed < 0.005 else f"{elapsed:.2f}s"
        yield Text.from_markup(
            f"  [cyan]{frame}[/cyan] {self._label:<38}"
            f" [cyan]{t_str}[/cyan]"
        )


# ── Phase spinner renderable ──────────────────────────────────────────────────

class _PhaseSpinnerLine:
    """Rich renderable for a top-level phase spinner (no indent, bold label).

    Visually distinct from ``_SpinnerLine`` (which is indented and used for
    per-step output): this one sits flush-left with a bolder look so phases
    stand out above the per-tile / per-step lines nested under them.
    """

    _FRAMES = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]
    _FPS    = 12.5

    def __init__(self, label: str) -> None:
        from rich.markup import escape
        self._label = escape(label)
        self._t0    = time.monotonic()

    def __rich_console__(self, console, options):
        from rich.text import Text
        elapsed = time.monotonic() - self._t0
        frame   = self._FRAMES[int(elapsed * self._FPS) % len(self._FRAMES)]
        t_str   = f"{elapsed:.1f}s" if elapsed >= 0.05 else "  …"
        yield Text.from_markup(
            f"[cyan]{frame}[/cyan] [bold]{self._label}[/bold]"
            f"  [cyan]{t_str}[/cyan]"
        )


# ── Rich ──────────────────────────────────────────────────────────────────────

class RichReporter(TileReporter):
    """Rich-powered output: spinner per step, coloured stats, batch summary table."""

    verbose_layers: bool = False

    def __init__(self) -> None:
        from rich.console import Console
        self._console = Console(highlight=False)
        self._live    = None          # active rich Live context (spinner)
        self._t0_tile:  float | None  = None
        self._t0_batch: float | None  = None
        self._batch_rows: list[dict]  = []
        self._current_tile: str       = ""
        self._current_outputs: list[dict] = []
        self._current_cols: int       = 1
        self._current_rows: int       = 1

    # ── Internal ─────────────────────────────────────────────────────────────

    def _stop_status(self) -> None:
        if self._live is not None:
            self._live.__exit__(None, None, None)
            self._live = None

    def _start_status(self, label: str) -> None:
        from rich.live import Live
        self._live = Live(
            _SpinnerLine(label),
            console=self._console,
            refresh_per_second=12,
            transient=True,   # erases spinner line on exit so ✓ can overwrite
        )
        self._live.__enter__()

    def _time_color(self, elapsed: float) -> str:
        if elapsed < 2.0:
            return "dim white"
        if elapsed < 10.0:
            return "yellow"
        return "bold #ff8800"

    @staticmethod
    def _table_time_color(elapsed: float) -> str:
        """Smooth gradient for table: #666666 @ 0s → #ffff00 @ 2s → #ff8800 @ 4s+."""
        _DIM = (102, 102, 102)
        _YEL = (255, 255, 0)
        _RED = (255, 136, 0)
        if elapsed <= 1.0:
            r, g, b = _DIM
        elif elapsed <= 2.0:
            t = elapsed - 1.0          # 0..1 over the 1s→2s range
            r = int(_DIM[0] + t * (_YEL[0] - _DIM[0]))
            g = int(_DIM[1] + t * (_YEL[1] - _DIM[1]))
            b = int(_DIM[2] + t * (_YEL[2] - _DIM[2]))
        elif elapsed <= 4.0:
            t = (elapsed - 2.0) / 2.0
            r = int(_YEL[0] + t * (_RED[0] - _YEL[0]))
            g = int(_YEL[1] + t * (_RED[1] - _YEL[1]))
            b = int(_YEL[2] + t * (_RED[2] - _YEL[2]))
        else:
            r, g, b = _RED
        return f"#{r:02x}{g:02x}{b:02x}"

    @staticmethod
    def _table_geo_color(value: int, max_val: int) -> str:
        """Smooth gradient for verts/faces: #666666 @ 0–100k → #ffff00 @ max_val."""
        _DIM  = (102, 102, 102)
        _YEL  = (255, 255, 0)
        FLOOR = 100_000
        if max_val <= FLOOR:
            return f"#{_DIM[0]:02x}{_DIM[1]:02x}{_DIM[2]:02x}"
        t = max(0.0, min(1.0, (value - FLOOR) / (max_val - FLOOR)))
        r = int(_DIM[0] + t * (_YEL[0] - _DIM[0]))
        g = int(_DIM[1] + t * (_YEL[1] - _DIM[1]))
        b = int(_DIM[2] + t * (_YEL[2] - _DIM[2]))
        return f"#{r:02x}{g:02x}{b:02x}"

    # ── Tile ─────────────────────────────────────────────────────────────────

    def tile_begin(self, name, cols, rows, grid_w, grid_h,
                   region_ids, boundary_ids) -> None:
        from rich.rule import Rule
        from rich.text import Text

        self._current_cols = cols
        self._current_rows = rows
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
        tc = self._table_time_color(elapsed)
        self._console.print(
            f"  [dim]──[/dim] [{tc}]{elapsed:.1f}s[/] [dim]──[/dim]"
        )

    # ── Steps ────────────────────────────────────────────────────────────────

    def step_begin(self, label: str) -> None:
        self._stop_status()
        self._start_status(label)

    def step_end(self, label: str, elapsed: float, detail: str = "") -> None:
        self._stop_status()
        tc = self._table_time_color(elapsed)
        detail_str = f"  [dim]{detail}[/dim]" if detail else ""
        self._console.print(
            f"  [green]✓[/green] {label:<38}"
            f" [{tc}]{elapsed:.2f}s[/]"
            f"{detail_str}"
        )

    # ── Rebuild ──────────────────────────────────────────────────────────────

    def rebuild_begin(self, suffix: str, square_mm: float) -> None:
        self._stop_status()
        sq = int(square_mm) if square_mm == int(square_mm) else square_mm
        self._console.print(
            f"  [#0078d4]── building {suffix} at {sq}mm/sq ──[/#0078d4]"
        )

    # ── Phases ───────────────────────────────────────────────────────────────

    def phase_header(self, label: str) -> None:
        """Print a static phase heading (for phases where a Live display follows)."""
        self._stop_status()
        self._console.print(f"\n[bold]{label}[/bold]")

    def phase_begin(self, label: str) -> None:
        """Begin a top-level phase with an animated spinner."""
        self._stop_status()
        from rich.live import Live
        self._live = Live(
            _PhaseSpinnerLine(label),
            console=self._console,
            refresh_per_second=12,
            transient=True,
        )
        self._live.__enter__()

    def phase_end(self, label: str, elapsed: float, detail: str = "") -> None:
        """Stop the phase spinner and print a bold completion line."""
        self._stop_status()
        tc     = self._table_time_color(elapsed)
        suffix = f"  [dim]{detail}[/dim]" if detail else ""
        self._console.print(
            f"[bold green]✓[/bold green] [bold]{label}[/bold]"
            f"  [{tc}]{elapsed:.1f}s[/]"
            f"{suffix}"
        )

    # ── Export ───────────────────────────────────────────────────────────────

    def export_done(self, suffix, path, n_verts, n_faces, watertight, elapsed) -> None:
        self._stop_status()
        if watertight is None:
            wt_icon, wt_label = "[dim]○[/dim]", "not checked"
        else:
            wt_icon  = "[green]●[/green]" if watertight else "[bold red]✗[/bold red]"
            wt_label = "watertight" if watertight else "NOT watertight"
        tc = self._table_time_color(elapsed)
        self._console.print(
            f"  [green]✓[/green] Export [{suffix}]"
            f"  {wt_icon} {wt_label}"
            f"  [dim]{n_verts:,} verts · {n_faces:,} faces[/dim]"
            f"  [{tc}]{elapsed:.2f}s[/]"
        )
        self._console.print(f"      [#0078d4]{path}[/#0078d4]")
        self._current_outputs.append(dict(
            suffix=suffix, path=path,
            n_verts=n_verts, n_faces=n_faces,
            watertight=watertight,
        ))

    # ── Batch ────────────────────────────────────────────────────────────────

    def batch_begin(self, n_tiles: int) -> None:
        self._t0_batch = time.perf_counter()
        self._batch_rows = []
        self._console.print(
            f"[bold]Batch:[/bold] {n_tiles} tile{'s' if n_tiles != 1 else ''}"
        )

    def batch_tile_begin(self, tile_name: str) -> None:
        self._current_tile    = tile_name
        self._current_outputs = []   # reset per-tile export list

    def batch_tile_done(self, tile_name: str, elapsed: float) -> None:
        self._batch_rows.append(dict(
            name=tile_name, elapsed=elapsed, outputs=list(self._current_outputs),
            cols=self._current_cols, rows=self._current_rows,
        ))

    def inject_batch_row(self, row: dict) -> None:
        """Accept a pre-built result row from a parallel worker process.

        Prints a ✓ completion line and records for the summary table.
        Used in non-TTY (pipe) fallback.  On a TTY the Live display in
        ``main()`` handles visual output; use ``record_batch_rows()`` instead.
        """
        self._batch_rows.append(row)
        tc = self._table_time_color(row['elapsed'])
        self._console.print(
            f"  [green]✓[/green] {row['name']:<38} [{tc}]{row['elapsed']:.1f}s[/]"
        )

    def record_batch_rows(self, rows: list) -> None:
        """Record rows from parallel workers without printing.

        Called after the Live display has already rendered all ✓ lines to
        the terminal.  Only populates ``_batch_rows`` for ``batch_end()``.
        """
        self._batch_rows.extend(rows)

    def batch_end(self, n_tiles: int, elapsed: float) -> None:
        self._stop_status()
        if not self._batch_rows:
            return

        from rich.table import Table

        max_verts = max((o['n_verts'] for r in self._batch_rows for o in r['outputs']), default=1)
        max_faces = max((o['n_faces'] for r in self._batch_rows for o in r['outputs']), default=1)

        table = Table(show_header=True, header_style="bold dim",
                      border_style="dim", box=_MINIMAL_BOX)
        table.add_column("Tile",      style="cyan")
        table.add_column("Size",      justify="right")
        table.add_column("Time",      justify="right")
        table.add_column("Type",      style="dim")
        table.add_column("Verts",     justify="right")
        table.add_column("Faces",     justify="right")
        table.add_column("Watertight")

        for row in self._batch_rows:
            outputs = row["outputs"]
            verts_str  = "\n".join(
                f"[{self._table_geo_color(o['n_verts'], max_verts)}]{o['n_verts']:,}[/]"
                for o in outputs
            )
            faces_str  = "\n".join(
                f"[{self._table_geo_color(o['n_faces'], max_faces)}]{o['n_faces']:,}[/]"
                for o in outputs
            )
            wt_str     = "\n".join(
                "[green]✓[/green]" if o["watertight"] else "[red]✗[/red]"
                for o in outputs
            )
            suffix_str = "\n".join(o["suffix"] for o in outputs)
            tc = self._table_time_color(row["elapsed"])
            cols, rows = row["cols"], row["rows"]
            if cols == 1 and rows == 1:
                size_str = f"[dim]{cols}×{rows}[/dim]"
            else:
                size_str = f"[yellow]{cols}×{rows}[/yellow]"
            table.add_row(
                row["name"],
                size_str,
                f"[{tc}]{row['elapsed']:.1f}s[/]",
                suffix_str,
                verts_str,
                faces_str,
                wt_str,
            )

        self._console.print()
        self._console.print(table)
        per_tile = elapsed / max(n_tiles, 1)
        tc_total    = self._table_time_color(elapsed)
        tc_per_tile = self._table_time_color(per_tile)
        self._console.print(
            f"\n[bold]{n_tiles}[/bold] tile{'s' if n_tiles != 1 else ''} "
            f"in [{tc_total}]{elapsed:.1f}s[/]"
            f"  [dim]([/dim][{tc_per_tile}]{per_tile:.1f}s[/][dim]/tile)[/dim]"
        )


# ── Rich box style (compact) ──────────────────────────────────────────────────

try:
    from rich.box import Box as _Box
    _MINIMAL_BOX = _Box(
        "    \n"
        "    \n"
        "    \n"
        "    \n"
        "    \n"
        "    \n"
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
