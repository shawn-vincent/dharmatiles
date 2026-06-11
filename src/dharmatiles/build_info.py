"""Build provenance: stable runtime interface to embedded build metadata.

Consumers should use only get_build_info() and BuildInfo — never import
_build_info directly or call git at runtime.

The _build_info module is generated at build time by ``generate-build-info``
and must not be committed to source control.  In development (editable
install, no generated file) this module falls back to querying git live so
the header always shows meaningful information.

Canonical display format (from build-provenance-spec.md):

    Ver: <version> · <buildTimestamp>

Examples:
    Ver: v1.0.0 · 2026-06-11T09:34:12Z
    Ver: v1.0.0-12-gabc1234-dirty · 2026-06-11T09:34:12Z
    Ver: gabc1234 · 2026-06-11T09:34:12Z
    Ver: unknown · 2026-06-11T09:34:12Z
"""

from __future__ import annotations

import re
import subprocess
from dataclasses import dataclass, field
from datetime import datetime, timezone


@dataclass
class BuildInfo:
    """Captured build provenance.  All fields are strings unless noted."""

    version: str           # human-readable; primary user-facing identifier
    commit: str            # full SHA
    commit_short: str      # abbreviated SHA
    branch: str            # source branch at build time
    tag: str               # most recent reachable tag (may be empty)
    commits_since_tag: int # 0 on a tagged release
    build_timestamp: str   # ISO-8601 UTC, e.g. "2026-06-11T09:34:12Z"
    is_dirty: bool         # uncommitted changes present at build time
    commits: list = field(default_factory=list)  # commit list since last tag

    def display_version(self, max_len: int | None = None) -> str:
        """Version string, trimming the tag with ... only as much as needed.

        When max_len is set and the version is too long, trims the tag portion
        (not the end of the string) so the diagnostic -N-gHASH[-dirty] suffix
        stays fully visible.
        """
        if max_len is None or len(self.version) <= max_len:
            return self.version
        if not self.tag or not self.version.startswith(self.tag):
            return self.version[:max_len - 3] + "..."
        suffix = self.version[len(self.tag):]
        tag_budget = max_len - len(suffix) - 3  # 3 for "..."
        if tag_budget <= 0:
            return "..." + suffix
        return self.tag[:tag_budget] + "..." + suffix

    def version_line(self, width: int | None = None) -> str:
        """Canonical display string per build-provenance-spec.md.

        If width is given, the tag is trimmed only as much as needed so the
        full line fits within width — leaving as much tag visible as possible.
        """
        prefix = "Ver: "
        sep    = " · "
        if width is not None:
            max_len = max(0, width - len(prefix) - len(sep) - len(self.build_timestamp))
            version_str = self.display_version(max_len)
        else:
            version_str = self.display_version()
        return f"{prefix}{version_str}{sep}{self.build_timestamp}"


_UNKNOWN = BuildInfo(
    version="unknown",
    commit="unknown",
    commit_short="unknown",
    branch="unknown",
    tag="unknown",
    commits_since_tag=0,
    build_timestamp="unknown",
    is_dirty=False,
)


def get_build_info() -> BuildInfo:
    """Return provenance for the running artifact.

    Prefers the embedded _build_info module written by ``generate-build-info``.
    Falls back to a live git query when that file is absent (development only).
    """
    try:
        from dharmatiles._build_info import DATA  # generated artifact
        return BuildInfo(**DATA)
    except ImportError:
        return _build_from_git()


# ── development fallback ──────────────────────────────────────────────────────

def _git(cwd: str | None = None, *args: str) -> str:
    try:
        r = subprocess.run(
            ["git", *args], capture_output=True, text=True, cwd=cwd,
        )
        return r.stdout.strip() if r.returncode == 0 else ""
    except FileNotFoundError:
        return ""


def _build_from_git() -> BuildInfo:
    """Query git at runtime — development convenience only."""
    now = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")

    commit       = _git(None, "rev-parse", "HEAD")            or "unknown"
    commit_short = _git(None, "rev-parse", "--short", "HEAD") or "unknown"
    branch       = _git(None, "rev-parse", "--abbrev-ref", "HEAD") or "unknown"
    is_dirty     = bool(_git(None, "status", "--porcelain"))

    describe = _git(None, "describe", "--tags", "--long", "--always")
    if not describe:
        return BuildInfo(
            version="unknown", commit=commit, commit_short=commit_short,
            branch=branch, tag="", commits_since_tag=0,
            build_timestamp=now, is_dirty=is_dirty,
        )

    # "v1.0.0-12-gabcdef7" or bare hash "abcdef7"
    m = re.match(r'^(.+)-(\d+)-g([0-9a-f]+)$', describe)
    if m:
        tag              = m.group(1)
        commits_since    = int(m.group(2))
        short            = m.group(3)
        if commits_since == 0:
            version = tag + ("-dirty" if is_dirty else "")
        else:
            version = f"{tag}-{commits_since}-g{short}" + ("-dirty" if is_dirty else "")
    else:
        # No reachable tag — bare hash
        tag           = ""
        commits_since = 0
        short         = describe
        version       = f"g{short}" + ("-dirty" if is_dirty else "")

    return BuildInfo(
        version=version,
        commit=commit,
        commit_short=commit_short,
        branch=branch,
        tag=tag,
        commits_since_tag=commits_since,
        build_timestamp=now,
        is_dirty=is_dirty,
    )
