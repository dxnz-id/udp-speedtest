#!/bin/sh
# Run udp-speedtest straight from a GitHub release without installing.
# Only writes to ~/.cache/udp-speedtest — delete that folder to remove everything.
set -eu

REPO=dxnz-id/udp-speedtest
CACHE="${XDG_CACHE_HOME:-$HOME/.cache}/udp-speedtest"

sha256() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$@"
  else
    shasum -a 256 "$@"
  fi
}

os=$(uname -s | tr '[:upper:]' '[:lower:]')
case "$os" in
  linux)  os=linux ;;
  darwin) os=darwin ;;
  *) echo "udp-speedtest: unsupported os '$os' (windows: use the zip from releases)" >&2; exit 1 ;;
esac
case "$(uname -m)" in
  x86_64)               arch=x86_64 ;;
  aarch64 | arm64)      arch=aarch64 ;;
  *) echo "udp-speedtest: unsupported arch '$(uname -m)'" >&2; exit 1 ;;
esac
# musl on x86_64 linux so any distro works without matching glibc
if [ "$os" = linux ] && [ "$arch" = x86_64 ]; then
  target="$arch-unknown-linux-musl"
elif [ "$os" = linux ]; then
  target="$arch-unknown-linux-gnu"
else
  target="$arch-apple-darwin"
fi

base="https://github.com/$REPO/releases/latest/download"
asset="udp-speedtest-$target.tar.gz"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
curl -fsSL "$base/SHA256SUMS" -o "$tmp/SHA256SUMS"
tar_sha=$(grep "$asset" "$tmp/SHA256SUMS" | cut -d' ' -f1)
[ -n "$tar_sha" ] || { echo "udp-speedtest: no asset for $target in latest release" >&2; exit 1; }

# cache key = tarball checksum, so a new release is picked up automatically
dir="$CACHE/$target-$tar_sha"
# extract to dir.new, then rename: an interrupted run never leaves a
# half-written binary in the cache
if [ ! -x "$dir/udp-speedtest" ]; then
  curl -fsSL "$base/$asset" -o "$tmp/$asset"
  grep "$asset" "$tmp/SHA256SUMS" | (cd "$tmp" && sha256 -c) >/dev/null
  mkdir -p "$CACHE"
  rm -rf "$dir.new"
  mkdir -p "$dir.new"
  tar xzf "$tmp/$asset" -C "$dir.new"
  chmod +x "$dir.new/udp-speedtest"
  rm -rf "$dir"
  mv "$dir.new" "$dir"
fi

exec "$dir/udp-speedtest" "$@"
