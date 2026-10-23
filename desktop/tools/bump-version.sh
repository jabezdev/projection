#!/usr/bin/env bash
# Bumps the app version (0.MINOR.BUILD) in crates/app/Cargo.toml, which the
# installer, the About line in Settings and the log all read.
#
#   tools/bump-version.sh          # 0.1.4 -> 0.1.5 (every build)
#   tools/bump-version.sh minor    # 0.1.5 -> 0.2.0 (bigger releases, by hand)
#
# Installers upgrade in place only when the version increases, so
# build-windows.sh runs this before every build.
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
