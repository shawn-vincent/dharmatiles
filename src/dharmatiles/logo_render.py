"""Terminal logo header.

render_header()  — 3–5-line Unicode frame art with title, version, and URL.

Frame anatomy:
  ╭───┬───╮   rounded corners + top/bottom midpoint T-notch
  │ … │   │   plain side  (above notch)
  ├ 空 … ┤   ← side notch always at vertical midpoint of content lines
  │ … │   │   plain side  (below notch)
  ╰───┴───╯
"""

from __future__ import annotations

import argparse

from rich.console import Console
from rich.text import Text

from dharmatiles.build_info import get_build_info

_GITHUB_URL = "https://github.com/shawn-vincent/dharmatiles"

# Left prefix display widths:
#   notch line : "├ 空  " = │(1) + sp(1) + 空(2) + sp(1) + sp(1) = 6 cols
#   plain line : "│     " = │(1) + sp(5)                          = 6 cols
_LEFT_COLS  = 2   # "├ " or "│ "
_ICON_COLS  = 4   # "空  " (空 is 2 display cols + 2 spaces) or "    " (4 spaces)
_RIGHT_COLS = 2   # " ┤" or " │"
_OVERHEAD   = _LEFT_COLS + _ICON_COLS + _RIGHT_COLS  # 8 display cols

# Graceful-degradation thresholds (body_cols required to show each element):
_MIN_SUBTITLE_COLS = 12   # drop subtitle if fewer than this many chars would appear
_MIN_URL_COLS      = 20   # drop URL line entirely below this body width
_MIN_VERSION_COLS  = 16   # drop version line entirely below this body width


def render_header(
    title: str = "dharmatiles",
    subtitle: str = "procedural 3D-printable terrain tiles",
    version: str | None = None,
    url: str = _GITHUB_URL,
) -> None:
    """Print the logo header at full terminal width.

    *version* defaults to ``git describe``; pass ``""`` to suppress it.
    Content degrades gracefully on narrow terminals: subtitle is dropped
    below ``_MIN_SUBTITLE_COLS``, URL and version below their own thresholds.

    Example::

        render_header("generate-tile-stl", "v0.1.0")
    """
    console = Console()
    frame_w = console.width or 80
    if version is None:
        version = get_build_info().version_line(width=frame_w - 2 - _OVERHEAD)
    _print_frame(console, frame_w, title, subtitle, version, url)


def _print_frame(console: Console, width: int,
                 title: str, subtitle: str,
                 version: str, url: str) -> None:
    inner     = max(0, width - 2)
    half      = (inner - 1) // 2
    top       = f"╭{'─' * half}┬{'─' * max(0, inner - half - 1)}╮"
    bottom    = f"╰{'─' * half}┴{'─' * max(0, inner - half - 1)}╯"
    body_cols = max(0, width - _OVERHEAD)

    lines: list[Text] = []

    # ── Title line ────────────────────────────────────────────────────────────
    _SEP      = "  ·  "
    title_str = title[:body_cols]
    sub_avail = max(0, body_cols - len(title_str) - len(_SEP))
    sub_str   = subtitle[:sub_avail] if subtitle and sub_avail >= _MIN_SUBTITLE_COLS else ""
    used      = len(title_str) + (len(_SEP) + len(sub_str) if sub_str else 0)

    t1 = Text(no_wrap=True)
    t1.append("空", style="bold green")
    t1.append("  ")
    t1.append(title_str, style="bold green")
    if sub_str:
        t1.append(_SEP, style="bright_black")
        t1.append(sub_str, style="white")
    t1.append(" " * max(0, body_cols - used))
    lines.append(t1)

    # ── URL line ──────────────────────────────────────────────────────────────
    if url and body_cols >= _MIN_URL_COLS:
        url_str = url[:body_cols]
        t2 = Text(no_wrap=True)
        t2.append("    ")
        t2.append(url_str, style=f"link {url} cyan")
        t2.append(" " * max(0, body_cols - len(url_str)))
        lines.append(t2)

    # ── Version line ──────────────────────────────────────────────────────────
    if version and body_cols >= _MIN_VERSION_COLS:
        ver_str = version[:body_cols]
        t3 = Text(no_wrap=True)
        t3.append("    ")
        t3.append(ver_str, style="dim white")
        t3.append(" " * max(0, body_cols - len(ver_str)))
        lines.append(t3)

    # ── Notch at vertical midpoint, then print ────────────────────────────────
    notch_idx = (len(lines) - 1) // 2

    console.print(Text(top, style="bright_white"))
    for i, content in enumerate(lines):
        is_notch = (i == notch_idx)
        line = Text(no_wrap=True)
        line.append("├ " if is_notch else "│ ", style="bright_white")
        line.append_text(content)
        line.append(" ┤" if is_notch else " │", style="bright_white")
        console.print(line)
    console.print(Text(bottom, style="bright_white"))


def main() -> None:
    ap = argparse.ArgumentParser(description="Print the dharmatiles logo header.")
    ap.add_argument("--title",    default="dharmatiles")
    ap.add_argument("--subtitle", default="procedural 3D-printable terrain tiles")
    ap.add_argument("--version",  default=None,
                    help="version string (default: git describe)")
    ap.add_argument("--url",      default=_GITHUB_URL)
    args = ap.parse_args()
    render_header(args.title, args.subtitle, args.version, args.url)


if __name__ == "__main__":
    main()
