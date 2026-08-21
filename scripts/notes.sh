#!/usr/bin/env bash
# Print one version's section of CHANGELOG.md, for a release body.
#
# Release notes generated from commit subjects say what was touched, not what
# changed for the person installing it. The changelog already says that, so the
# release quotes it rather than paraphrasing it — and when a version has no
# section, this exits non-zero so the caller can fall back rather than publish
# an empty release body.
#
# Usage: scripts/notes.sh 0.2.0 [path/to/CHANGELOG.md]
set -euo pipefail

version="${1:?usage: notes.sh <version> [changelog]}"
changelog="${2:-CHANGELOG.md}"

[ -f "$changelog" ] || { echo "no $changelog" >&2; exit 1; }

# From the "## <version>" heading to the line before the next "## ".
#
# Blank lines *inside* the section are content — they are what separates one
# paragraph from the next — so only the leading and trailing runs are trimmed.
section=$(awk -v v="$version" '
  $0 ~ "^## " v "( |$|—)" { found = 1; next }
  found && /^## / { exit }
  found { print }
' "$changelog" | awk '
  # buffer blank lines and emit them only once something follows
  /^[[:space:]]*$/ { if (started) pending++; next }
  { while (pending-- > 0) print ""; pending = 0; started = 1; print }
')

if [ -z "${section// }" ]; then
  echo "no section for $version in $changelog" >&2
  exit 1
fi

printf '%s\n' "$section"
