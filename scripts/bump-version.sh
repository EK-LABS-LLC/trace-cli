#!/usr/bin/env bash
set -euo pipefail

version="${1:-}"

if [[ ! "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "Usage: make bump VERSION=x.y.z" >&2
  exit 2
fi

tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT

awk -v version="$version" '
  !updated && /^version = / {
    print "version = \"" version "\""
    updated = 1
    next
  }
  { print }
' Cargo.toml > "$tmp"
mv "$tmp" Cargo.toml

awk -v version="$version" '
  $0 == "name = \"pulse\"" { in_pulse = 1; print; next }
  in_pulse && /^version = / {
    print "version = \"" version "\""
    in_pulse = 0
    next
  }
  { print }
' Cargo.lock > "$tmp"
mv "$tmp" Cargo.lock

echo "Updated CLI version to $version."
