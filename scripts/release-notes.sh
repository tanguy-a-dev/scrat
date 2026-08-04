#!/usr/bin/env bash
#
# Render the commits in a range as Markdown release notes, grouped by
# Conventional Commit type.
#
# Usage:
#   scripts/release-notes.sh                      # since the last release tag
#   scripts/release-notes.sh v0.1.0..HEAD         # an explicit range
#   scripts/release-notes.sh v0.1.0..HEAD v0.2.0  # ...and the tag being released
#
# Written for bash 3.2 so it runs on stock macOS as well as CI.

set -euo pipefail

REPO_ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)

RANGE="${1:-}"
NEW_TAG="${2:-}"

PREVIOUS_TAG=$(git -C "$REPO_ROOT" describe --tags --abbrev=0 --match 'v[0-9]*.[0-9]*.[0-9]*' 2>/dev/null || true)

if [ -z "$RANGE" ]; then
  if [ -n "$PREVIOUS_TAG" ]; then
    RANGE="$PREVIOUS_TAG..HEAD"
  else
    RANGE="HEAD"
  fi
fi

# One line per commit: "<short sha>\x1f<subject>". %s is a single line by
# definition, so no commit message can inject extra list items. The previous
# release's own `chore(release):` bookkeeping commit is dropped — it describes
# the last release, not a change in this one.
commits=$(git -C "$REPO_ROOT" log --no-merges --format='%h%x1f%s' "$RANGE" -- |
  grep -v "$(printf '\037')chore(release):" || true)

# Print a "## Heading" section listing every commit whose type matches, with the
# `type(scope):` prefix stripped — the heading already says what they are.
section() {
  local heading="$1" pattern="$2" body

  body=$(printf '%s\n' "$commits" | awk -F'\037' -v pattern="$pattern" '
    NF < 2 { next }
    {
      subject = $2
      # Only the conventional "type(scope)!: " form counts as a prefix; prose
      # that happens to contain a colon is not a typed commit.
      if (match(subject, /^[a-zA-Z]+(\([^)]*\))?!?: /)) {
        type = subject
        sub(/[(!:].*$/, "", type)
        type = tolower(type)
        if (type ~ pattern) {
          printf "- %s (`%s`)\n", substr(subject, RSTART + RLENGTH), $1
        }
      } else if (pattern == "__untyped__") {
        printf "- %s (`%s`)\n", subject, $1
      }
    }
  ')

  if [ -n "$body" ]; then
    printf '## %s\n\n%s\n\n' "$heading" "$body"
  fi
}

# Breaking changes lead, because they are the reason anyone reads release notes.
# Read as NUL-separated whole messages rather than lines: a `BREAKING CHANGE:`
# footer sits several lines into the body, so line-oriented parsing misses it.
breaking=$(git -C "$REPO_ROOT" log -z --no-merges --format='%h%x1f%B' "$RANGE" -- |
  while IFS= read -r -d '' record; do
    sha=${record%%$'\037'*}
    message=${record#*$'\037'}
    subject=${message%%$'\n'*}

    # A `case` pattern containing ")" trips bash 3.2's $( ) parser, so this
    # stays a glob test.
    if [[ $subject == 'chore(release):'* ]]; then
      continue
    fi

    if [[ $subject =~ ^[a-zA-Z]+(\([^\)]*\))?!: ]] ||
       printf '%s\n' "$message" | grep -qE '^BREAKING[ -]CHANGE:'; then
      printf -- '- %s (`%s`)\n' "$subject" "$sha"
    fi
  done)

if [ -n "$breaking" ]; then
  printf '## ⚠ Breaking changes\n\n%s\n\n' "$breaking"
fi

section 'Features'    '^feat$'
section 'Fixes'       '^fix$'
section 'UX'          '^ux$'
section 'Performance' '^perf$'
section 'Reverts'     '^revert$'
section 'Docs'        '^docs$'
section 'Internal'    '^(tests?|chore|ops|ci|build|style|clean|refacto|refactor)$'
section 'Other'       '__untyped__'

printf -- '---\n\n'
printf 'Built from `%s`.\n' "$(git -C "$REPO_ROOT" rev-parse --short HEAD)"

# The compare link only exists once there is a previous release to compare to.
if [ -n "$PREVIOUS_TAG" ] && [ -n "$NEW_TAG" ] && [ -n "${GITHUB_REPOSITORY:-}" ]; then
  printf '\n**Full changelog**: https://github.com/%s/compare/%s...%s\n' \
    "$GITHUB_REPOSITORY" "$PREVIOUS_TAG" "$NEW_TAG"
fi
