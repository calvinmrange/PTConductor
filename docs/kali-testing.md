# Kali setup and testing

Use a current Kali Linux VM, a regular user account, Rust stable and Node.js 22 or newer. For the desktop, install native development libraries:

```bash
sudo apt update
sudo apt install -y build-essential curl pkg-config libssl-dev \
  libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev \
  nmap python3
rustup update stable
node --version
npm --version
```

If Rust or Node is missing, install them using their official installation instructions. Keep `codex` and `nmap` on the PATH of the regular user launching PTConductor. Nmap's TCP connect mode requires no `sudo`.

## Build and deterministic checks

From the repository root:

```bash
cargo fmt --all --check
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo build --locked -p ptconductor-cli
PTC_TEST_REAL_NMAP=1 python3 tests/integration.py
```

The integration suite uses a temporary fake Codex executable and real local HTTP server, plus a real Nmap loopback test when enabled. It needs no credentials and never scans an external target. A path containing spaces is intentional. It exercises authorization, fixed scan arguments, evidence-to-prompt flow, final-message parsing, quota/invalid-output errors, history, exports, secret redaction and index rebuild. Live Codex login/inference is a separate acceptance gate in [codex-kali.md](codex-kali.md).

## Desktop and shared history

```bash
export PTCONDUCTOR_WORKFLOWS_DIR="$PWD/workflows/examples"
export PTCONDUCTOR_RUNS_DIR="$PWD/runs"
export PTCONDUCTOR_CODEX_BIN="$(command -v codex)"
cd apps/desktop
npm ci
npm run build
npm run tauri dev
```

The directory overrides are optional but allow the desktop and CLI to see the same history when exported in both launching terminals. Without overrides the desktop uses `~/.local/share/dev.ptconductor.desktop` (or the XDG data directory), seeds embedded examples and saves reports under `reports`. The GUI reports each export's full path.

Do not set `WEBKIT_DISABLE_COMPOSITING_MODE` or disable sandboxing by default. If the VM has rendering problems, first update its graphics drivers/guest tools and record the observed error. Test the native app, not only the Vite browser preview; the preview does not execute Rust commands.

## Packaging

```bash
# From repo root: CLI archive with examples, docs and launcher
bash scripts/build-release.sh
# Desktop, on the same Linux architecture as the target VM
cd apps/desktop
npm run tauri build -- --bundles deb
```

CLI archives appear in `dist`; desktop packages appear in `apps/desktop/src-tauri/target/release/bundle/deb`. Install the `.deb` using `sudo apt install ./PATH_TO_PACKAGE.deb`, then launch **PTConductor** as the logged-in user. Node and Rust are build dependencies; installed binaries still require their native runtime libraries, Nmap and the separate Codex CLI. The archive launcher sets an absolute examples directory and stores runs in the desktop's default data location. Workflow positional filenames still resolve against your current directory; use a path from `workflow list` when invoking outside the extracted package.

GitHub Actions uploads matching build artifacts on the pull request. Verify the downloaded package inside Kali before marking the final release accepted. A Linux build succeeding on Ubuntu is not a substitute for Kali GUI/account acceptance.

## History recovery

Close PTConductor and ensure no CLI runs are active. Back up the entire run directory. Preserve every UUID-named `.json` artifact. Move `index.sqlite` and any matching `index.sqlite-wal`/`index.sqlite-shm` files into a separate backup directory, then reopen history. The index is rebuilt from valid artifacts. Malformed artifacts are skipped with a warning and preserved for inspection. Existing stale index entries are removed by this rebuild. A corrupt artifact cannot be recovered from the index because it stores only metadata, not full evidence.

A run interrupted by process termination can remain `running`; inspect its saved steps and rerun with a new ID if needed. Do not edit history to imply completion. See [final-testing.md](final-testing.md) for acceptance cases and the demo sequence.
