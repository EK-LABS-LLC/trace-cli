#!/usr/bin/env bash
set -euo pipefail

tag="${GITHUB_REF_NAME:-${1:-}}"

if [[ -z "$tag" ]]; then
  echo "Usage: GITHUB_REF_NAME=vX.Y.Z $0" >&2
  exit 2
fi

if [[ "$tag" != v* ]]; then
  echo "::error::Release tag must start with 'v'." >&2
  exit 1
fi

tag_version="${tag#v}"
cargo_version="$(sed -nE 's/^version = "([^"]+)"/\1/p' Cargo.toml | head -n 1)"
lock_version="$(awk '
  $0 == "name = \"pulse\"" { in_pulse = 1; next }
  in_pulse && /^version = / { gsub(/"/, "", $3); print $3; exit }
' Cargo.lock)"

if [[ "$cargo_version" != "$lock_version" ]]; then
  echo "::error::Cargo.toml version ($cargo_version) does not match Cargo.lock version ($lock_version)." >&2
  exit 1
fi

if [[ "$tag_version" != "$cargo_version" ]]; then
  echo "::error::Release tag $tag does not match Cargo.toml version $cargo_version." >&2
  exit 1
fi

echo "Release version OK: $tag matches Cargo.toml and Cargo.lock."
