# Testing PTConductor on Kali Linux

These steps exercise the workflow loader, schema validation, generated GUI fields, and CLI input handling. Preparation does not make a provider or network request, and the `mock` provider returns deterministic output. For a Codex test using a ChatGPT sign-in, see [codex-kali.md](codex-kali.md).

## 1. Install build prerequisites

```bash
sudo apt update
sudo apt install -y \
  build-essential curl file git libayatana-appindicator3-dev librsvg2-dev \
  libssl-dev libwebkit2gtk-4.1-dev libxdo-dev wget
```

Install Rust with [rustup](https://rustup.rs/) and Node.js 22 or later. Tauri's current Linux dependencies are documented in the [official prerequisites](https://v2.tauri.app/start/prerequisites/).

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"

node --version
npm --version
cargo --version
```

If Kali's Node.js package is older than version 22, install a current LTS release before continuing.

## 2. Clone and verify the code

```bash
git clone https://github.com/calvinmrange/PTConductor.git
cd PTConductor

cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace
```

## 3. Test workflow discovery and schema validation

```bash
cargo run -p ptconductor-cli -- workflow list
cargo run -p ptconductor-cli -- workflow validate \
  workflows/examples/web-endpoint-review.json
```

The list should include `web-endpoint-review`, `typed-input-demo`, and `technology-fingerprint`. Validation should print the workflow version and a `sha256:` content hash.

## 4. Test typed inputs and secret redaction

Create a harmless local evidence file, then prepare the all-types example:

```bash
printf 'authorized test evidence\n' > /tmp/ptconductor-evidence.txt

cargo run -p ptconductor-cli -- prepare \
  workflows/examples/typed-input-demo.json \
  --input 'TITLE=Kali VM smoke test' \
  --input TARGET=https://example.test \
  --input MAX_ITEMS=5 \
  --input INCLUDE_INFORMATIONAL=true \
  --input EVIDENCE_FILE=/tmp/ptconductor-evidence.txt \
  --input API_TOKEN=not-a-real-secret
```

Confirm that the output contains typed values, that `API_TOKEN` is shown as `***REDACTED***`, and that the actual token never appears. Then verify a rejection case:

```bash
cargo run -p ptconductor-cli -- prepare \
  workflows/examples/typed-input-demo.json \
  --input TITLE=test \
  --input TARGET=not-a-url \
  --input API_TOKEN=test
```

This command should exit with an invalid URL error.

## 5. Test workflow creation

Use a temporary workflow directory so the repository checkout stays unchanged:

```bash
mkdir -p /tmp/ptconductor-workflows

cargo run -p ptconductor-cli -- \
  --workflows-dir /tmp/ptconductor-workflows \
  workflow create \
  --id kali-smoke-test \
  --name 'Kali smoke test' \
  --field 'TARGET:url:required:Target URL' \
  --field 'TOKEN:secret:required:Temporary token' \
  --prompt 'Review <TARGET> using <TOKEN>'

cargo run -p ptconductor-cli -- \
  --workflows-dir /tmp/ptconductor-workflows workflow list
```

## 6. Launch the desktop GUI

```bash
cd apps/desktop
npm ci
npm run build
npm run tauri dev
```

In the GUI:

1. Select each example from the workflow library.
2. Confirm the form changes to match its declared field types.
3. Enter valid values and choose **Validate & prepare**.
4. Confirm secrets are redacted in both resolved inputs and the prompt preview.
5. Enter an invalid URL or nonexistent file and confirm validation blocks preparation.
6. Use the **+** button to create a workflow, then select and prepare it.

To test a separate workflow folder in the GUI, set the environment variable before launching:

```bash
PTCONDUCTOR_WORKFLOWS_DIR=/tmp/ptconductor-workflows npm run tauri dev
```
