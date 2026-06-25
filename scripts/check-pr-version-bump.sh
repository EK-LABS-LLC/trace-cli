#!/usr/bin/env bash
set -euo pipefail

version_from_toml() {
  sed -nE 's/^version = "([^"]+)"/\1/p' "$1" | head -n 1
}

latest_tag_version() {
  git tag --list 'v[0-9]*' --sort=-v:refname | head -n 1 | sed 's/^v//'
}

version_gt() {
  local current="$1"
  local base="$2"
  local current_major current_minor current_patch
  local base_major base_minor base_patch

  IFS=. read -r current_major current_minor current_patch <<< "$current"
  IFS=. read -r base_major base_minor base_patch <<< "$base"

  for part in "$current_major" "$current_minor" "$current_patch" "$base_major" "$base_minor" "$base_patch"; do
    if [[ ! "$part" =~ ^[0-9]+$ ]]; then
      echo "::error::Expected semantic versions in X.Y.Z format; got current=$current base=$base." >&2
      exit 1
    fi
  done

  if (( 10#$current_major != 10#$base_major )); then
    (( 10#$current_major > 10#$base_major ))
    return
  fi

  if (( 10#$current_minor != 10#$base_minor )); then
    (( 10#$current_minor > 10#$base_minor ))
    return
  fi

  (( 10#$current_patch > 10#$base_patch ))
}

base_ref="${BASE_REF:-origin/${GITHUB_BASE_REF:-main}}"
current_version="$(version_from_toml Cargo.toml)"
lock_version="$(awk '
  $0 == "name = \"pulse\"" { in_pulse = 1; next }
  in_pulse && /^version = / { gsub(/"/, "", $3); print $3; exit }
' Cargo.lock)"

if [[ "$current_version" != "$lock_version" ]]; then
  echo "::error::Cargo.toml version ($current_version) does not match Cargo.lock version ($lock_version)." >&2
  exit 1
fi

base_toml="$(mktemp)"
trap 'rm -f "$base_toml"' EXIT
git show "$base_ref:Cargo.toml" > "$base_toml"
base_version="$(version_from_toml "$base_toml")"
latest_release_version="$(latest_tag_version)"

if ! version_gt "$current_version" "$base_version"; then
  echo "::error::CLI version must be bumped above $base_version before merging this PR. Current version is $current_version." >&2
  exit 1
fi

if [[ -n "$latest_release_version" ]] && ! version_gt "$current_version" "$latest_release_version"; then
  echo "::error::CLI version must be bumped above latest release v$latest_release_version before merging this PR. Current version is $current_version." >&2
  exit 1
fi

echo "CLI version bump OK: main $base_version, latest release v$latest_release_version -> $current_version."
