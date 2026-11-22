#!/usr/bin/env bash
set -euo pipefail
here=$(dirname "$(readlink -f "$0")")
desktop=$(readlink -f "$here/..")
tools=${PROJECTION_WIN_TOOLS:-$HOME/.local/share/projection-win}
target=x86_64-pc-windows-msvc
ffmpeg_build=ffmpeg-n9.0-latest-win64-lgpl-shared-9.0
nsis_version=3.12-1
nsis_snapshot=20260504T084255Z

if [ "${1:-}" != "--no-bump" ]; then
    echo "Version $("$here/bump-version.sh")"
fi

mkdir -p "$tools"
export PATH="$HOME/.cargo/bin:$PATH"

if [ ! -x "$HOME/.cargo/bin/rustup" ]; then
    echo "Installing rustup into ~/.cargo"
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --no-modify-path --profile minimal --default-toolchain stable
fi
rustup target add "$target" >/dev/null
command -v cargo-xwin >/dev/null || cargo install cargo-xwin --locked

export FFMPEG_DIR="$tools/$ffmpeg_build"
if [ ! -d "$FFMPEG_DIR" ]; then
    echo "Fetching $ffmpeg_build"
    curl -sSfL -o "$tools/ffmpeg.zip" "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/$ffmpeg_build.zip"
    bsdtar -xf "$tools/ffmpeg.zip" -C "$tools"
    rm "$tools/ffmpeg.zip"
fi

nsis="$tools/nsis-$nsis_version"
if [ ! -x "$nsis/usr/bin/makensis" ]; then
    echo "Fetching NSIS $nsis_version"
    work=$(mktemp -d)
    mkdir -p "$nsis"
    for pkg in "nsis_${nsis_version}_amd64" "nsis-common_${nsis_version}_all"; do
        curl -sSfL -o "$work/$pkg.deb" "https://snapshot.debian.org/archive/debian/$nsis_snapshot/pool/main/n/nsis/$pkg.deb"
        bsdtar -xf "$work/$pkg.deb" -C "$work"
        bsdtar -xf "$work"/data.tar.* -C "$nsis"
        rm "$work"/data.tar.* "$work"/control.tar.*
    done
    rm -rf "$work"
fi
mkdir -p "$tools/bin"
cat >"$tools/bin/makensis" <<EOF
#!/bin/sh
NSISDIR="$nsis/usr/share/nsis" exec "$nsis/usr/bin/makensis" "\$@"
EOF
chmod +x "$tools/bin/makensis"
export PATH="$tools/bin:$PATH"

dlls="$desktop/crates/app/windows/ffmpeg"
rm -rf "$dlls"
mkdir -p "$dlls"
for lib in avcodec avformat avutil swresample swscale; do
    cp "$FFMPEG_DIR"/bin/$lib-*.dll "$dlls/"
done

key_file="$HOME/.tauri/palto-updater.key"
updater_config=()
if [ -z "${TAURI_SIGNING_PRIVATE_KEY:-}" ] && [ -f "$key_file" ]; then
    export TAURI_SIGNING_PRIVATE_KEY="$key_file"
fi
if [ -n "${TAURI_SIGNING_PRIVATE_KEY:-}" ]; then
    export TAURI_SIGNING_PRIVATE_KEY_PASSWORD="${TAURI_SIGNING_PRIVATE_KEY_PASSWORD:-}"
else
    echo "No updater signing key ($key_file or TAURI_SIGNING_PRIVATE_KEY): building without update signature." >&2
    updater_config=(--config '{"bundle":{"createUpdaterArtifacts":false}}')
fi

cd "$desktop/crates/app"
bun install --silent
bun install --silent --cwd frontend
bun tauri build --runner cargo-xwin --target "$target" --bundles nsis "${updater_config[@]}"

bundle="$desktop/target/$target/release/bundle/nsis"
version=$(sed -n 's/^version = "\(.*\)"$/\1/p' "$desktop/crates/app/Cargo.toml" | head -n1)
installer="PALTO_${version}_x64-setup.exe"
if [ -f "$bundle/$installer.sig" ]; then
    repo=${GITHUB_REPOSITORY:-jabezdev/projection}
    RELEASE_NOTES="${RELEASE_NOTES:-}" VERSION="$version" SIGNATURE="$(cat "$bundle/$installer.sig")" \
    URL="https://github.com/$repo/releases/download/v$version/$installer" \
    python3 -c 'import json, os, datetime; print(json.dumps({"version": os.environ["VERSION"], "notes": os.environ["RELEASE_NOTES"], "pub_date": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"), "platforms": {"windows-x86_64": {"signature": os.environ["SIGNATURE"], "url": os.environ["URL"]}}}, indent=2))' \
    >"$bundle/latest.json"
fi

echo
ls -1 "$bundle/"*.exe "$bundle/"*.sig "$bundle/latest.json" 2>/dev/null
