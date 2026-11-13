#!/usr/bin/env bash
# The flags below are from NSH 0.5.0 f4d68f7 package --help.
# This lane deliberately produces a debug-signed, directly installable APK only.
set -euo pipefail
nsh_cli=${1:?Usage: bash scripts/package-android.sh /absolute/path/to/nsh}
repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"
version=$(python3 -c 'import tomllib; print(tomllib.load(open("Cargo.toml", "rb"))["workspace"]["package"]["version"])')
if [[ ${GITHUB_REF_TYPE:-} == tag && ${GITHUB_REF_NAME:-} != "v$version" ]]; then
  echo 'Release tag must match Cargo version' >&2
  exit 1
fi
python3 -c 'import json,tomllib; assert json.load(open("crates/yank-client/nsh.json"))["version"] == tomllib.load(open("Cargo.toml","rb"))["workspace"]["package"]["version"]'
mkdir -p dist
package_dir=$(mktemp -d)
trap 'rm -rf "$package_dir"' EXIT
"$nsh_cli" package crates/yank-client --target android --abi arm64-v8a --format apk --out "$package_dir"
test -s "$package_dir/Yank.apk"
asset="yank-client-$version-android-arm64-debug.apk"
cp "$package_dir/Yank.apk" "dist/$asset"
cat > dist/ANDROID-DEBUG.txt <<'NOTICE'
This arm64-v8a APK uses NSH's debug signing key. It is for direct installation
and testing, not Google Play distribution. The key is ephemeral in CI; upgrades
from a previous build may require uninstalling it (and losing local app data).
No release keystore or production signing credentials are configured.
NOTICE
(cd dist && sha256sum "$asset" ANDROID-DEBUG.txt > SHA256SUMS-android-arm64-debug.txt)
