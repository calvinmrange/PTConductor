# PTConductor

PTConductor is a local-first penetration-testing workflow application for authorized targets. A shared Rust engine powers the React/Tauri desktop and CLI: load a versioned workflow, validate inputs, collect evidence, summarize it with an AI provider, save a run, and export a report.

## Implemented MVP

- Versioned JSON workflows; creation, discovery, validation and redacted previews.
- Typed text, textarea, URL, number, boolean, file-path and secret inputs.
- Sequential `ai_prompt`, `nmap_scan` and `http_headers` steps; `<STEPS>` passes collected evidence to a later prompt.
- Codex CLI with ChatGPT authentication, OpenAI-compatible APIs, local Ollama and an offline mock.
- Bounded Nmap TCP/service scans and HTTP security-header collection, each requiring active-target authorization.
- JSON run artifacts and SQLite history, including partially completed and failed runs.
- Standalone Markdown/HTML reports from saved runs, available in both interfaces.
- Desktop workflow search, history filters, provider readiness check and export controls.
- Packaged desktop data paths and embedded example workflows; CLI release archive and Linux `.deb` packaging.

## Kali quick start: Codex

Install Rust stable, Node.js 22+, Nmap and Codex as described in [Kali setup](docs/kali-testing.md). Run as your regular Kali user, not with `sudo`:

```bash
npm install -g @openai/codex@latest
codex login --device-auth
codex login status
cargo build --locked -p ptconductor-cli
./target/debug/ptconductor doctor
```

The doctor checks installation, required CLI flags and ChatGPT login without calling a model. Leave the Codex model at `default` to use the CLI's available default; use `--model` only for an explicitly available model. No OpenAI API key is needed for this provider. Model access and usage allowance are checked by Codex during live execution.

Scan one authorized host and summarize actual Nmap XML:

```bash
./target/debug/ptconductor run workflows/examples/nmap-scan-summary.json \
  --provider codex --allow-remote --allow-active \
  --input TARGET=127.0.0.1 --input PORTS=22,80,443 \
  --output-json nmap-summary.json
```

In another terminal start a local demonstration target with `python3 -m http.server 8000 --bind 127.0.0.1`, then assess its headers:

```bash
./target/debug/ptconductor run workflows/examples/security-headers-summary.json \
  --provider codex --allow-remote --allow-active \
  --input TARGET=http://127.0.0.1:8000 \
  --output-json headers-summary.json
```

`--allow-active` authorizes collection against the supplied target. `--allow-remote` confirms sending evidence to Codex/OpenAI. Nmap runs a TCP connect scan with light service detection, at most 1,024 ports and a 60-second host timeout. Header collection makes one GET, verifies TLS and does not follow redirects. Neither workflow delegates collection commands to the model. Results are AI assessments for operator review, not proof of vulnerabilities.

Full troubleshooting and desktop steps: [Codex on Kali](docs/codex-kali.md).

## Workflows and history

```bash
./target/debug/ptconductor workflow list
./target/debug/ptconductor workflow validate workflows/examples/nmap-scan-summary.json
./target/debug/ptconductor prepare workflows/examples/nmap-scan-summary.json
./target/debug/ptconductor history --json
./target/debug/ptconductor show RUN_ID
./target/debug/ptconductor report RUN_ID --format markdown --output report.md
./target/debug/ptconductor report RUN_ID --format html --output report.html
```

Replace `RUN_ID` with a saved UUID. Export commands create new files and refuse overwrites. `prepare` performs no scan or model call. The original `technology-fingerprint.json` analyzes pasted headers/HTML rather than fetching a target. The mock provider tests plumbing; it does not produce a meaningful assessment.

Create a prompt workflow from the CLI or the desktop's **New workflow** form:

```bash
./target/debug/ptconductor workflow create \
  --id endpoint-review --name 'Endpoint review' \
  --field 'TARGET:url:required:Target URL' \
  --field 'NOTES:textarea:optional:Observations' \
  --prompt 'Review supplied observations for <TARGET>: <NOTES>'
```

Collector workflows can be edited as JSON using [the workflow schema](schemas/workflow-v1alpha1.schema.json). File inputs validate a local file path; they do not automatically upload or embed its contents.

CLI paths default to `workflows/examples` and `runs` relative to the current directory. Override with `--workflows-dir`, `--runs-dir`, `PTCONDUCTOR_WORKFLOWS_DIR` or `PTCONDUCTOR_RUNS_DIR`. Desktop paths default to `$XDG_DATA_HOME/dev.ptconductor.desktop` (normally `~/.local/share/dev.ptconductor.desktop`), with `workflows`, `runs` and `reports` subdirectories. Set `PTCONDUCTOR_DATA_DIR` to change the desktop root. Set both directory variables to absolute paths to share CLI and desktop history. JSON artifacts are authoritative; see [recovery](docs/kali-testing.md#history-recovery).

## Other providers

- Ollama: `--provider ollama --model YOUR_INSTALLED_MODEL`; loopback only, default `http://127.0.0.1:11434`.
- OpenAI-compatible API: `--provider openai --model YOUR_AVAILABLE_MODEL --allow-remote`; read `OPENAI_API_KEY` in the launching shell. API billing is separate from ChatGPT/Codex usage.
- Mock: `--provider mock`; no credentials or model call. Active workflows still collect evidence and need `--allow-active`.

The CLI accepts `--base-url` for HTTP providers. Desktop endpoint overrides are `PTCONDUCTOR_OPENAI_BASE_URL` and `PTCONDUCTOR_OLLAMA_BASE_URL`. Codex uses its own authenticated CLI route and does not accept a base URL. Never commit credentials, real evidence or run outputs.

## Development and packaging

```bash
cargo fmt --all --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build -p ptconductor-cli
python3 tests/integration.py
# Optional: real Nmap, only against the test's loopback HTTP server
PTC_TEST_REAL_NMAP=1 python3 tests/integration.py
bash scripts/build-release.sh

cd apps/desktop
npm ci
npm run build
npm run tauri dev
# Build an installable Linux package on Kali with native dependencies installed
npm run tauri build -- --bundles deb
```

GitHub Actions checks Rust/frontend/native desktop builds, runs real loopback Nmap plus simulated Codex contracts, and uploads CLI/desktop packages. Live Codex account access and interactive Kali desktop acceptance remain manual release gates; [the final checklist](docs/final-testing.md) covers them and the demonstration.

## Project layout

`apps/cli` and `apps/desktop` compose the shared crates: `ptc-domain`, `ptc-engine`, `ptc-providers`, `ptc-persistence`, `ptc-reporting`. See [architecture](docs/architecture.md) and [security boundaries](docs/security.md).

The MVP supports sequential, bounded collectors and summaries. General shell steps, NSE scripts, multi-target scopes, exploit execution, branching, encrypted storage and automated verification of AI findings are outside this release.

Licensed under [GNU GPL v3.0](LICENSE).
