#!/usr/bin/env bash
# Cross-builds the Windows installer (NSIS setup.exe) on Linux.
#
#   tools/build-windows.sh            # bumps the build number first
#   tools/build-windows.sh --no-bump  # rebuilds the current version
#
# Updates: the installer is signed for the in-app updater with the key in
# TAURI_SIGNING_PRIVATE_KEY (CI secret) or ~/.tauri/palto-updater.key, and
# latest.json is written next to it for the GitHub release. Release notes
# shown before installing come from RELEASE_NOTES (text) if set. Without a
# key the installer still builds, but cannot be offered as an update.
#
# The first run fetches its toolchain into ~/.local/share/projection-win and
# rustup into ~/.cargo, all without sudo; the system Rust stays the default.
# Needs curl, bsdtar, clang, lld and llvm (llvm-rc) on the system. The MSI
# needs WiX, which only runs on Windows, so only the NSIS installer is built.
set -euo pipefail
here=$(dirname "$(readlink -f "$0")")
desktop=$(readlink -f "$here/..")
tools=${PROJECTION_WIN_TOOLS:-$HOME/.local/share/projection-win}
target=x86_64-pc-windows-msvc
# Must match the ffmpeg-next major.minor in Cargo.toml.
ffmpeg_build=ffmpeg-n8.1-latest-win64-lgpl-shared-8.1
nsis_version=3.12-1

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

# Debian's NSIS build, unpacked rather than installed (NSIS is AUR-only on Arch).
nsis="$tools/nsis-$nsis_version"
if [ ! -x "$nsis/usr/bin/makensis" ]; then
    echo "Fetching NSIS $nsis_version"
    work=$(mktemp -d)
    mkdir -p "$nsis"
    for pkg in "nsis_${nsis_version}_amd64" "nsis-common_${nsis_version}_all"; do
        curl -sSfL -o "$work/$pkg.deb" "https://deb.debian.org/debian/pool/main/n/nsis/$pkg.deb"
        bsdtar -xf "$work/$pkg.deb" -C "$work"
        bsdtar -xf "$work"/data.tar.* -C "$nsis"
        rm "$work"/data.tar.* "$work"/control.tar.*
    done
    rm -rf "$work"
fi
# The bundler overrides NSISDIR, so a wrapper pins it to the unpacked data.
mkdir -p "$tools/bin"
cat >"$tools/bin/makensis" <<EOF
#!/bin/sh
NSISDIR="$nsis/usr/share/nsis" exec "$nsis/usr/bin/makensis" "\$@"
EOF
chmod +x "$tools/bin/makensis"
export PATH="$tools/bin:$PATH"

# The installer places these next to PALTO.exe (tauri.windows.conf.json).
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
