#!/usr/bin/env bash
set -euo pipefail
manifest="$(dirname "$(readlink -f "$0")")/../crates/app/Cargo.toml"
current=$(sed -n 's/^version = "\(.*\)"$/\1/p' "$manifest" | head -n1)
IFS=. read -r major minor build <<<"$current"
case "${1:-build}" in
    build) build=$((build + 1)) ;;
    minor) minor=$((minor + 1)); build=0 ;;
    *) echo "usage: $0 [build|minor]" >&2; exit 2 ;;
esac
next="$major.$minor.$build"
sed -i "0,/^version = \"$current\"$/s//version = \"$next\"/" "$manifest"
echo "$next"
