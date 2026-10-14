#!/usr/bin/env bash
set -euo pipefail
repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
cargo build --locked --release -p ptconductor-cli
package="ptconductor-0.1.0-linux-$(uname -m)"
mkdir -p dist
stage="$(mktemp -d)"
trap 'rm -rf "$stage"' EXIT
mkdir -p "$stage/$package/bin" "$stage/$package/workflows"
cp target/release/ptconductor "$stage/$package/bin/"
cp -R workflows/examples "$stage/$package/workflows/"
cp -R docs "$stage/$package/"
cp README.md LICENSE "$stage/$package/"
cat > "$stage/$package/ptconductor" <<'LAUNCHER'
#!/usr/bin/env bash
set -euo pipefail
package_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
export PTCONDUCTOR_WORKFLOWS_DIR="${PTCONDUCTOR_WORKFLOWS_DIR:-$package_root/workflows/examples}"
export PTCONDUCTOR_RUNS_DIR="${PTCONDUCTOR_RUNS_DIR:-${XDG_DATA_HOME:-$HOME/.local/share}/dev.ptconductor.desktop/runs}"
exec "$package_root/bin/ptconductor" "$@"
LAUNCHER
chmod +x "$stage/$package/ptconductor"
tar -C "$stage" -czf "dist/$package.tar.gz" "$package"
printf 'Created %s\n' "$repo_root/dist/$package.tar.gz"
