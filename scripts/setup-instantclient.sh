#!/usr/bin/env bash
# Install Oracle Instant Client (required by the Rust `oracle` crate).
# Usage: ./scripts/setup-instantclient.sh [linux.arm64|linux.x64]
set -euo pipefail

arch=${1:-linux.arm64}
version=23.26.3.0.0
base="https://download.oracle.com/otn_software/linux/instantclient/2326300"
prefix="$HOME/instantclient"

case "$arch" in
  linux.arm64|linux.x64) ;;
  *) echo "unsupported architecture: $arch (use linux.arm64 or linux.x64)" >&2; exit 2 ;;
esac

mkdir -p "$prefix"
tmp_dir=$(mktemp -d)
trap 'rm -rf "$tmp_dir"' EXIT
for z in basiclite sdk; do
  url="$base/instantclient-${z}-${arch}-${version}.zip"
  echo "downloading $url"
  curl -fsSL "$url" -o "$tmp_dir/${z}.zip"
  unzip -qo "$tmp_dir/${z}.zip" -d "$prefix"
done

dir=$(find "$prefix" -mindepth 1 -maxdepth 1 -type d -name 'instantclient_*' -print \
  | sort -V | tail -n 1)
if [[ -z "$dir" ]]; then
  echo "could not locate extracted Instant Client directory under $prefix" >&2
  exit 1
fi
cat <<EOF

Installed at: $dir
Add to your shell profile:

  export LD_LIBRARY_PATH=$dir:\$LD_LIBRARY_PATH
EOF
