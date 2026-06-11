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


def render_header(
    title: str = "dharmatiles",
    subtitle: str = "procedural 3D-printable terrain tiles",
    version: str | None = None,
    url: str = _GITHUB_URL,
) -> None:
    """Print the logo header.

    *version* defaults to ``git describe``; pass ``""`` to suppress it.
    Width fills the terminal up to 80 columns.

    Example::

        render_header("generate-tile-stl", "v0.1.0")
    """
    console = Console()
    frame_w = min(console.width or 80, 80)
    if version is None:
        version = get_build_info().version_line(width=frame_w - 2 - _OVERHEAD)
    _print_frame(console, frame_w, title, subtitle, version, url)



def _print_frame(console: Console, width: int,
                 title: str, subtitle: str,
                 version: str, url: str) -> None:
    inner = width - 2
    half  = (inner - 1) // 2
    top    = f"╭{'─' * half}┬{'─' * (inner - half - 1)}╮"
    bottom = f"╰{'─' * half}┴{'─' * (inner - half - 1)}╯"

    body_cols = width - _OVERHEAD  # display cols available for body text

    # ── build content lines ────────────────────────────────────────────────
    # Each entry is a Text covering just the icon + body area (_ICON_COLS + body_cols).
    # Border chars are added later once we know the notch position.

    lines: list[Text] = []

    # title line — icon = 空  (空 is 2 display cols but 1 str char; body_cols already
    # accounts for all 8 overhead display cols, so no extra -1 needed here)
    _SEP = "  ·  "
    sub_avail = max(0, body_cols - len(title) - len(_SEP))
    sub_str   = subtitle[:sub_avail] if subtitle else ""
    body_str  = title + (_SEP + sub_str if sub_str else "")
    pad1      = max(0, body_cols - len(body_str))

    t1 = Text(no_wrap=True)
    t1.append("空",     style="bold green")
    t1.append("  ")
    t1.append(title,   style="bold green")
    if sub_str:
        t1.append("  ·  ", style="bright_black")
        t1.append(sub_str, style="white")
    t1.append(" " * pad1)
    lines.append(t1)

    # URL line — icon = spaces
    if url:
        upad = max(0, body_cols - len(url))
        t3 = Text(no_wrap=True)
        t3.append("    ")
        t3.append(url[:body_cols], style=f"link {url} cyan")
        t3.append(" " * upad)
        lines.append(t3)

    # version line — icon = spaces
    if version:
        vpad = max(0, body_cols - len(version))
        t2 = Text(no_wrap=True)
        t2.append("    ")
        t2.append(version[:body_cols], style="dim white")
        t2.append(" " * vpad)
        lines.append(t2)

    # ── place notch at vertical midpoint ──────────────────────────────────
    # (N-1)//2 rounds toward top when N is even.
    notch_idx = (len(lines) - 1) // 2

    # ── print ──────────────────────────────────────────────────────────────
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
