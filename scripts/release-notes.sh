#!/usr/bin/env bash
# Prints the CHANGELOG.md section of a version: scripts/release-notes.sh v0.3.0
set -euo pipefail
version="${1:?usage: release-notes.sh <version or tag>}"
version="${version#v}"
changelog="${2:-$(dirname "$0")/../CHANGELOG.md}"
notes=$(awk -v v="$version" '
  /^## \[/ { if (found) exit; if (index($0, "## [" v "]") == 1) { found = 1; next } }
  /^\[[^]]+\]: / { if (found) exit }
  found { print }
' "$changelog" | sed -e '/./,$!d' | sed -e :a -e '/^\n*$/{$d;N;ba' -e '}')
if [ -z "$notes" ]; then
  echo "No CHANGELOG.md section for $version" >&2
  exit 1
fi
printf '%s\n' "$notes"
