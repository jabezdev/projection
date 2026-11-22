#!/usr/bin/env bash
set -euo pipefail
here=$(dirname "$(readlink -f "$0")")
obs=$(readlink -f "$here/..")
tools=${PROJECTION_WIN_TOOLS:-$HOME/.local/share/projection-win}
target=x86_64-pc-windows-msvc
nsis_version=3.12-1

mkdir -p "$tools"
export PATH="$HOME/.cargo/bin:$PATH"
if [ ! -x "$HOME/.cargo/bin/rustup" ]; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --no-modify-path --profile minimal --default-toolchain stable
fi
rustup target add "$target" >/dev/null
command -v cargo-xwin >/dev/null || cargo install cargo-xwin --locked

nsis="$tools/nsis-$nsis_version"
if [ ! -x "$nsis/usr/bin/makensis" ]; then
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

cd "$obs"
cargo xwin build --release --locked --target "$target"

version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n1)
out="$obs/target/dist"
stage="$out/palto-obs"
rm -rf "$out"
mkdir -p "$stage/bin/64bit" "$stage/data"
cp "target/$target/release/palto_obs.dll" "$stage/bin/64bit/palto-obs.dll"
cp README.md "$stage/data/README.txt"
(cd "$out" && bsdtar -a -cf "palto-obs-${version}-windows-x64.zip" palto-obs)
NSISDIR="$nsis/usr/share/nsis" "$nsis/usr/bin/makensis" -V2 -DVERSION="$version" -DSTAGE="$stage" -DOUT="$out/palto-obs-${version}-windows-x64-setup.exe" "$here/installer.nsi"

echo
ls -1 "$out"/*.zip "$out"/*.exe
