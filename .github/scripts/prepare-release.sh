#!/usr/bin/env bash
# The first step of a release: makes the commit that sets the new version (Cargo.toml and Cargo.lock), so that a release
# needs nothing but running the workflow. Run in a checkout of the commit to release from, by the Release workflow.
#
#   prepare-release.sh 1.2.3
#
# Writes `sha` (the commit to build, test and release) and `bumped` (whether a commit was made) to $GITHUB_OUTPUT.
#
# The commit is pushed to the temporary branch release/<version>, not to master: every job of the release checks out
# that exact commit, so what is tested is what is released, and master is not touched until all of it has passed. If the
# version is already the one in Cargo.toml there is nothing to commit and the commit to release from is the one given.
set -euo pipefail

version=${1:?usage: prepare-release.sh X.Y.Z}
cd "$(dirname "$0")/../.."

"${PYTHON:-python3}" .github/scripts/set_version.py "$version"

if [ -z "$(git status --porcelain)" ]; then
  bumped=false
  echo "Cargo.toml already says $version: nothing to commit"
else
  unexpected=$(git status --porcelain | grep -v -E ' (Cargo\.toml|Cargo\.lock)$' || true)
  if [ -n "$unexpected" ]; then
    echo "::error::the checkout has changes besides the version:" >&2
    echo "$unexpected" >&2
    exit 1
  fi
  git config user.name "github-actions[bot]"
  git config user.email "41898282+github-actions[bot]@users.noreply.github.com"
  git commit -q -a -m "Release $version" -m "Sets the version of every crate to $version (Cargo.toml and Cargo.lock). Made by the Release workflow."
  # A branch of an earlier try that did not finish is replaced.
  git push --force origin "HEAD:refs/heads/release/$version"
  bumped=true
fi

sha=$(git rev-parse HEAD)
echo "release commit: $sha (bumped: $bumped)"
if [ -n "${GITHUB_OUTPUT:-}" ]; then
  echo "sha=$sha" >> "$GITHUB_OUTPUT"
  echo "bumped=$bumped" >> "$GITHUB_OUTPUT"
fi
