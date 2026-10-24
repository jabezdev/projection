#!/usr/bin/env bash
# Publishes the Windows installer that build-windows.sh made as a GitHub
# release, so installed copies of PALTO offer it as an update.
#
#   tools/release.sh
#
# Run it after build-windows.sh, once the bumped version is committed and
# pushed: it tags that commit as v<version> and uploads the installer, its
# updater signature and latest.json (whose notes are the release notes).
# Needs the GitHub CLI (gh), logged in.
set -euo pipefail
desktop=$(readlink -f "$(dirname "$(readlink -f "$0")")/..")
bundle="$desktop/target/x86_64-pc-windows-msvc/release/bundle/nsis"
version=$(sed -n 's/^version = "\(.*\)"$/\1/p' "$desktop/crates/app/Cargo.toml" | head -n1)
installer="$bundle/PALTO_${version}_x64-setup.exe"
tag="v$version"

for file in "$installer" "$installer.sig" "$bundle/latest.json"; do
    [ -f "$file" ] || { echo "Missing $file: run tools/build-windows.sh (with the signing key) first." >&2; exit 1; }
done
grep -q "\"version\": \"$version\"" "$bundle/latest.json" || { echo "latest.json is not for $version; rebuild." >&2; exit 1; }
commit=$(git -C "$desktop" rev-parse HEAD)
if ! git -C "$desktop" diff --quiet HEAD -- crates/app/Cargo.toml; then
    echo "Commit the version bump in crates/app/Cargo.toml first." >&2; exit 1
fi
if [ -z "$(git -C "$desktop" branch -r --contains "$commit")" ]; then
    echo "Push $commit first; the release tags it on GitHub." >&2; exit 1
fi
notes=$(python3 -c 'import json, sys; print(json.load(open(sys.argv[1]))["notes"])' "$bundle/latest.json")

gh release create "$tag" --target "$commit" --title "PALTO $version" --notes "${notes:-PALTO $version}" \
    "$installer" "$installer.sig" "$bundle/latest.json"
git -C "$desktop" fetch --quiet --tags
echo "Released $tag: $(gh release view "$tag" --json url -q .url)"
