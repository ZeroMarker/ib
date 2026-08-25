#!/usr/bin/env bash
# Install Oracle Instant Client (required by the Rust `oracle` crate).
# Usage: ./scripts/setup-instantclient.sh [linux.arm64|linux.x64]
set -euo pipefail

arch=${1:-linux.arm64}
version=23.26.3.0.0
base="https://download.oracle.com/otn_software/linux/instantclient/2326300"
prefix="$HOME/instantclient"

mkdir -p "$prefix"
for z in basiclite sdk; do
  url="$base/instantclient-${z}-${arch}-${version}.zip"
  echo "downloading $url"
  curl -fsSL "$url" -o "/tmp/opencode/${z}.zip"
  unzip -qo "/tmp/opencode/${z}.zip" -d "$prefix"
done

dir=$(echo "$prefix"/instantclient* | head -1)
cat <<EOF

Installed at: $dir
Add to your shell profile:

  export LD_LIBRARY_PATH=$dir:\$LD_LIBRARY_PATH
EOF
