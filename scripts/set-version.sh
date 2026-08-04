#!/usr/bin/env bash
#
# Write a version across every manifest that carries one.
#
#   Cargo.toml              [workspace.package] version — the source of truth
#   package.json            root npm wrapper (Tauri CLI only)
#   frontend/package.json   SvelteKit app
#   Cargo.lock              refreshed so the workspace members' versions match
#
# src-tauri/Cargo.toml inherits via `version.workspace = true`, and
# src-tauri/tauri.conf.json has no `version` key at all so Tauri falls back to
# the Cargo.toml version. Neither needs rewriting — that is the point of
# leaving them empty, and a second stored copy of the version would only drift.
#
# Usage: scripts/set-version.sh 0.2.0

set -euo pipefail

REPO_ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
VERSION="${1:-}"

if [[ ! $VERSION =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "set-version: expected a X.Y.Z version, got '${VERSION}'" >&2
  exit 2
fi

# Replace `version = "..."` inside the [workspace.package] table only, so the
# unrelated version constraints under [workspace.dependencies] are untouched.
set_cargo_version() {
  local file="$1" tmp
  tmp=$(mktemp)
  awk -v version="$VERSION" '
    /^\[workspace\.package\]/ { in_section = 1; print; next }
    /^\[/                     { in_section = 0 }
    in_section && /^[[:space:]]*version[[:space:]]*=/ && !done {
      print "version = \"" version "\""
      done = 1
      next
    }
    { print }
    END { if (!done) exit 3 }
  ' "$file" > "$tmp"
  mv "$tmp" "$file"
}

# Replace the first top-level `"version": "..."`. package.json puts it in the
# first few lines, well before any dependency block that might also match.
set_npm_version() {
  local file="$1" tmp
  tmp=$(mktemp)
  awk -v version="$VERSION" '
    !done && /^[[:space:]]*"version"[[:space:]]*:/ {
      match($0, /^[[:space:]]*/)
      indent = substr($0, 1, RLENGTH)
      comma = ($0 ~ /,[[:space:]]*$/) ? "," : ""
      print indent "\"version\": \"" version "\"" comma
      done = 1
      next
    }
    { print }
    END { if (!done) exit 3 }
  ' "$file" > "$tmp"
  mv "$tmp" "$file"
}

set_cargo_version "$REPO_ROOT/Cargo.toml"
set_npm_version   "$REPO_ROOT/package.json"
set_npm_version   "$REPO_ROOT/frontend/package.json"

# Cargo.lock records the workspace members' own versions, so it goes stale the
# moment the manifest changes. Refresh it, or the release build fails under
# --locked and every artifact carries the old version.
if command -v cargo >/dev/null 2>&1; then
  ( cd "$REPO_ROOT" && cargo update --workspace --quiet )
fi

echo "set-version: repository set to $VERSION"
