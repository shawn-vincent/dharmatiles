#!/bin/bash
# Publish a release: the macOS arm64 tarball as a GitHub release, and the
# Homebrew tap formula. Usage: release/publish.sh 0.1.0 path/to/tarball
# (what `dist` would do; cross-platform builds are phase P9)
set -euo pipefail
V=$1; TAR=$2; N=$(basename "$TAR" .tar.gz)
SHA=$(shasum -a 256 "$TAR" | cut -d' ' -f1)
gh release create "v$V" "$TAR" -R shawn-vincent/dharmatiles --target main --title "dharmatiles $V" \
  --notes "Rust release $V: macOS arm64 binary. Other platforms are phase P9."
T=$(mktemp -d)
gh repo view shawn-vincent/homebrew-dharmatiles >/dev/null 2>&1 || gh repo create shawn-vincent/homebrew-dharmatiles --public --description "Homebrew tap for dharmatiles"
git clone -q https://github.com/shawn-vincent/homebrew-dharmatiles "$T"
mkdir -p "$T/Formula"
sed -e "s/@VERSION@/$V/g" -e "s/@SHA@/$SHA/g" -e "s/@NAME@/$N/g" "$(dirname "$0")/dharmatiles.rb.in" > "$T/Formula/dharmatiles.rb"
(cd "$T" && git add -A && git commit -qm "dharmatiles $V" && git push -q origin HEAD)
