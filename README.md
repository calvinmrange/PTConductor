# PTConductor

PTConductor is a local-first penetration-testing workflow orchestrator. It loads reusable, versioned workflows, collects typed inputs, invokes interchangeable AI providers, and stores structured run artifacts for evidence and reporting.

> [!IMPORTANT]
> PTConductor is intended only for systems you own or have explicit authorization to test.

## Architecture

The desktop application and CLI are thin adapters over the same Rust workflow engine:

```text
React/Tauri GUI ─┐
                 ├─> Application API -> Workflow engine -> provider/storage/report adapters
Rust CLI ────────┘
```

The frontend never owns workflow state, executes tools, or reads secrets directly. See [docs/architecture.md](docs/architecture.md) for the module boundaries and [docs/security.md](docs/security.md) for the security model.

## Repository layout

- `apps/cli` — `ptconductor` command-line interface
- `apps/desktop` — React frontend and thin Tauri host
- `crates/ptc-domain` — serialized domain models
- `crates/ptc-engine` — validation, variable resolution, and orchestration
- `crates/ptc-providers` — AI provider adapters
- `crates/ptc-persistence` — workflow and run repositories
- `crates/ptc-reporting` — deterministic report renderers
- `schemas` — versioned workflow and run JSON Schemas
- `workflows/examples` — safe sample workflows

## Initial commands

```bash
cargo run -p ptconductor-cli -- workflow list
cargo run -p ptconductor-cli -- workflow validate workflows/examples/web-endpoint-review.json
cargo run -p ptconductor-cli -- run workflows/examples/web-endpoint-review.json \
  --input TARGET=https://example.test \
  --provider mock
```

The scaffold currently includes a deterministic mock provider so the engine can be exercised without credentials. Ollama and OpenAI-compatible adapters are the next implementation milestone.

## Development

```bash
cargo fmt --all --check
cargo check --workspace
cargo test --workspace

cd apps/desktop
npm install
npm run build
```

## MVP boundary

The initial engine permits only `ai_prompt` workflow steps. Command execution, HTTP testing, scope enforcement, and evidence capture will be introduced behind explicit interfaces after the AI-only vertical slice is stable.

## License

PTConductor is licensed under the [GNU General Public License v3.0](LICENSE).
