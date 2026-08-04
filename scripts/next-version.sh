#!/usr/bin/env bash
#
# Decide the next semantic version from the Conventional Commit subjects
# written since the last release tag.
#
# Prints `key=value` lines, which is what the release workflow feeds into
# $GITHUB_OUTPUT:
#
#     current=0.1.0
#     bump=minor
#     next=0.2.0
#     tag=v0.2.0
#     range=v0.1.0..HEAD
#
# `bump=none` means nothing user-visible landed and no release should happen.
#
# Usage:
#   scripts/next-version.sh                 # classify commits since the last tag
#   scripts/next-version.sh --force minor   # override the classification
#
# Written for bash 3.2 so it runs on stock macOS as well as CI: no associative
# arrays, no ${var,,}.

set -euo pipefail

REPO_ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)

# --- commit type -> bump ----------------------------------------------------
#
# Scrat's commit vocabulary is wider than the Conventional Commits spec's — it
# also uses `ux:`, `ops:`, `refacto:`, `clean:` and `tests:`. Every type is
# listed explicitly and anything unrecognised releases *nothing*, so a typo'd
# prefix can never silently ship a version. Adding a new prefix to the repo's
# vocabulary means adding it here too.
bump_for_type() {
  case "$1" in
    feat)                                   echo minor ;;
    fix|perf|ux|revert)                     echo patch ;;
    docs|test|tests|chore|ops|ci|build|style|clean|refacto|refactor)
                                            echo none ;;
    *)                                      echo none ;;
  esac
}

bump_rank() {
  case "$1" in
    major) echo 3 ;;
    minor) echo 2 ;;
    patch) echo 1 ;;
    *)     echo 0 ;;
  esac
}

# Read the one true version: [workspace.package] version in the root Cargo.toml.
# Every other version in the repo is derived from it (see scripts/set-version.sh).
read_current_version() {
  awk '
    /^\[workspace\.package\]/ { in_section = 1; next }
    /^\[/                     { in_section = 0 }
    in_section && /^[[:space:]]*version[[:space:]]*=/ {
      sub(/.*=[[:space:]]*"/, ""); sub(/".*/, ""); print; exit
    }
  ' "$REPO_ROOT/Cargo.toml"
}

# The most recent vX.Y.Z tag reachable from HEAD, or empty if none exists yet.
last_release_tag() {
  git -C "$REPO_ROOT" describe --tags --abbrev=0 --match 'v[0-9]*.[0-9]*.[0-9]*' 2>/dev/null || true
}

# Classify a stream of NUL-separated full commit messages read from stdin.
# Full messages, not just subjects, because `BREAKING CHANGE:` lives in the body.
classify_bump() {
  local highest="none" highest_rank=0
  local message subject type bang this this_rank

  while IFS= read -r -d '' message; do
    subject=${message%%$'\n'*}

    # A release commit describes the release, not a change in it.
    case "$subject" in
      'chore(release):'*) continue ;;
    esac

    this="none"
    if [[ $subject =~ ^([a-zA-Z]+)(\([^\)]*\))?(!)?:[[:space:]] ]]; then
      type=$(printf '%s' "${BASH_REMATCH[1]}" | tr '[:upper:]' '[:lower:]')
      bang=${BASH_REMATCH[3]}
      if [ -n "$bang" ]; then
        this="major"
      else
        this=$(bump_for_type "$type")
      fi
    fi

    # A `BREAKING CHANGE:` footer promotes any commit, including one whose type
    # would otherwise release nothing. Anchored to a line start so a subject
    # that merely *mentions* the phrase does not trigger it.
    if printf '%s\n' "$message" | grep -qE '^BREAKING[ -]CHANGE:'; then
      this="major"
    fi

    this_rank=$(bump_rank "$this")
    if [ "$this_rank" -gt "$highest_rank" ]; then
      highest_rank=$this_rank
      highest=$this
    fi
  done

  printf '%s\n' "$highest"
}

# Apply a bump to a version. Below 1.0.0 a breaking change bumps the minor,
# not the major: 0.x is the "anything may still change" range, and burning
# 1.0.0 on the first rename would make the number meaningless.
next_version() {
  local current="$1" bump="$2" major minor patch

  if [[ ! $current =~ ^([0-9]+)\.([0-9]+)\.([0-9]+)$ ]]; then
    echo "next-version: current version '$current' is not X.Y.Z" >&2
    return 1
  fi
  major=${BASH_REMATCH[1]}
  minor=${BASH_REMATCH[2]}
  patch=${BASH_REMATCH[3]}

  case "$bump" in
    major)
      if [ "$major" -eq 0 ]; then
        printf '0.%d.0\n' "$((minor + 1))"
      else
        printf '%d.0.0\n' "$((major + 1))"
      fi
      ;;
    minor) printf '%d.%d.0\n' "$major" "$((minor + 1))" ;;
    patch) printf '%d.%d.%d\n' "$major" "$minor" "$((patch + 1))" ;;
    none)  printf '%s\n' "$current" ;;
    *)     echo "next-version: unknown bump '$bump'" >&2; return 1 ;;
  esac
}

main() {
  local forced="" current tag range bump next

  while [ $# -gt 0 ]; do
    case "$1" in
      --force) forced="${2:-}"; shift 2 ;;
      *) echo "next-version: unknown argument '$1'" >&2; return 2 ;;
    esac
  done

  current=$(read_current_version)
  if [ -z "$current" ]; then
    echo "next-version: no [workspace.package] version in Cargo.toml" >&2
    return 1
  fi

  tag=$(last_release_tag)
  if [ -n "$tag" ]; then
    range="$tag..HEAD"
  else
    range="HEAD"
  fi

  if [ -n "$forced" ] && [ "$forced" != "auto" ]; then
    bump="$forced"
  else
    # -z separates commits with NUL, so a commit message containing blank lines
    # or control characters cannot forge a record boundary.
    bump=$(git -C "$REPO_ROOT" log -z --no-merges --format=%B "$range" -- | classify_bump)
  fi

  next=$(next_version "$current" "$bump")

  printf 'current=%s\n' "$current"
  printf 'bump=%s\n' "$bump"
  printf 'next=%s\n' "$next"
  printf 'tag=v%s\n' "$next"
  printf 'previous_tag=%s\n' "$tag"
  printf 'range=%s\n' "$range"
}

# Only run when executed; `source`ing exposes the functions for the tests.
if [ "${BASH_SOURCE[0]}" = "$0" ]; then
  main "$@"
fi
