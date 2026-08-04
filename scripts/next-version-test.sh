#!/usr/bin/env bash
#
# Tests for the version-bump logic in next-version.sh.
#
# Commit subjects are free text a human types under time pressure, so this
# leans on the awkward cases — an unknown prefix, a colon in prose, a subject
# that merely mentions "BREAKING CHANGE" — rather than the happy path. Run with
# `make release-test` or directly.

set -uo pipefail

# shellcheck source=./next-version.sh
. "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/next-version.sh"

passed=0
failed=0

# Feed commit messages (each argument is one whole commit message) through the
# classifier the same way `git log -z` would.
classify() {
  local message
  for message in "$@"; do
    printf '%s\0' "$message"
  done | classify_bump
}

expect() {
  local label="$1" expected="$2" actual="$3"
  if [ "$expected" = "$actual" ]; then
    passed=$((passed + 1))
  else
    failed=$((failed + 1))
    printf 'FAIL  %s\n        expected: %s\n        actual:   %s\n' \
      "$label" "$expected" "$actual"
  fi
}

# --- type -> bump -----------------------------------------------------------

expect "feat is a minor"            minor "$(classify 'feat: add CSV import')"
expect "fix is a patch"             patch "$(classify 'fix: correct balance')"
expect "ux is a patch"              patch "$(classify 'ux: tighten paddings')"
expect "perf is a patch"            patch "$(classify 'perf: faster query')"
expect "revert is a patch"          patch "$(classify 'revert: undo the donut')"
expect "docs release nothing"       none  "$(classify 'docs: readme')"
expect "tests release nothing"      none  "$(classify 'tests: add sample db')"
expect "ops release nothing"        none  "$(classify 'ops: add github ci')"
expect "refacto releases nothing"   none  "$(classify 'refacto: rename vocabulary')"
expect "clean releases nothing"     none  "$(classify 'clean: remove 1pass try')"

# A scope must not change the classification.
expect "scoped feat is a minor"     minor "$(classify 'feat(csv): detect columns')"

# --- highest bump wins ------------------------------------------------------

expect "minor beats patch" minor \
  "$(classify 'fix: a' 'feat: b' 'docs: c')"
expect "patch alone stays patch" patch \
  "$(classify 'docs: a' 'fix: b' 'chore: c')"
expect "nothing releasable is none" none \
  "$(classify 'docs: a' 'tests: b' 'ops: c')"
expect "no commits at all is none" none "$(classify)"

# --- breaking changes -------------------------------------------------------

expect "bang marks a breaking change" major \
  "$(classify 'feat!: drop multi-currency')"
expect "bang with a scope" major \
  "$(classify 'feat(domain)!: drop multi-currency')"
expect "bang on a normally-silent type" major \
  "$(classify 'refacto!: collapse the id newtypes')"
expect "BREAKING CHANGE footer" major \
  "$(classify 'feat: rework opening balance

BREAKING CHANGE: migration 0008 is required.')"
expect "BREAKING-CHANGE hyphen spelling" major \
  "$(classify 'feat: x

BREAKING-CHANGE: y')"

# The phrase only counts as a footer, at the start of a line. A subject that
# talks *about* a breaking change is an ordinary commit — getting this wrong
# would silently ship a major release for a documentation fix.
expect "mentioning the phrase mid-line does not promote" none \
  "$(classify 'docs: explain what BREAKING CHANGE: means')"
expect "mentioning it mid-body does not promote" patch \
  "$(classify 'fix: a

We should note that BREAKING CHANGE: is a footer.')"

# --- malformed and adversarial subjects -------------------------------------

expect "unknown type releases nothing" none \
  "$(classify 'wibble: something')"
expect "no conventional prefix releases nothing" none \
  "$(classify 'just fixing a thing')"
expect "colon without a space is not a prefix" none \
  "$(classify 'feat:no-space')"
expect "prose containing a colon is not a prefix" none \
  "$(classify 'note: feat: this is not a feature')"
expect "uppercase type still classifies" minor \
  "$(classify 'FEAT: shouty')"
expect "leading blank line is not a subject" none \
  "$(classify '
feat: indented past the subject line')"

# A commit body cannot forge a second commit: records are NUL-separated, so
# blank lines and stray text in the body stay part of the same commit.
expect "body text cannot forge a feat" none \
  "$(classify 'docs: a

feat: this is body text, not a second commit')"

# The workflow writes `chore(release): vX.Y.Z` back to main. It must never
# count towards the *next* release, or every release would beget another.
expect "release commit is ignored" none \
  "$(classify 'chore(release): v0.2.0')"
expect "release commit does not mask real work" patch \
  "$(classify 'chore(release): v0.2.0' 'fix: a real fix')"

# --- version arithmetic -----------------------------------------------------

expect "patch below 1.0"        0.1.1 "$(next_version 0.1.0 patch)"
expect "minor below 1.0"        0.2.0 "$(next_version 0.1.4 minor)"
expect "minor resets the patch" 0.2.0 "$(next_version 0.1.9 minor)"
expect "no bump keeps version"  0.1.0 "$(next_version 0.1.0 none)"

# Below 1.0.0 a breaking change takes the minor. Bumping to 1.0.0 is a
# statement about stability, and it should be made deliberately, not by a
# commit message.
expect "breaking below 1.0 takes the minor" 0.2.0 "$(next_version 0.1.7 major)"
expect "breaking at 1.x takes the major"    2.0.0 "$(next_version 1.4.2 major)"
expect "minor at 1.x"                       1.5.0 "$(next_version 1.4.2 minor)"

# Two-digit components must not be compared as strings.
expect "patch past nine"  0.1.10 "$(next_version 0.1.9 patch)"
expect "minor past nine"  0.10.0 "$(next_version 0.9.3 minor)"

expect "a non-semver current version is rejected" 1 \
  "$(next_version 'v0.1.0' patch >/dev/null 2>&1; echo $?)"
expect "an empty current version is rejected" 1 \
  "$(next_version '' patch >/dev/null 2>&1; echo $?)"

# --- the repo's own manifest ------------------------------------------------

current=$(read_current_version)
if [[ $current =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  passed=$((passed + 1))
else
  failed=$((failed + 1))
  printf 'FAIL  Cargo.toml workspace version is readable and semver\n        actual: %s\n' "$current"
fi

# --- summary ----------------------------------------------------------------

printf '\n%d passed, %d failed\n' "$passed" "$failed"
[ "$failed" -eq 0 ]
