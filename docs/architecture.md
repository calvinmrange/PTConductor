# Architecture

PTConductor uses a ports-and-adapters architecture. Domain types have no dependency on Tauri, React, databases, filesystems, or a specific AI provider.

## Dependency direction

```text
apps/desktop ─┐
              ├──> ptc-engine ──> ptc-domain
apps/cli ─────┘        ^
                       │
        adapters implement engine ports
```

The engine owns the use cases and declares ports. Adapter crates implement those ports. Applications select and compose adapters at startup.

## Crate responsibilities

### `ptc-domain`

Versioned serialized models: workflows, input definitions, steps, runs, findings, and evidence references. It contains data and invariants but no I/O.

### `ptc-engine`

Workflow validation, typed input resolution, template rendering, provider invocation, run state transitions, and adapter traits. This is the single implementation used by every interface.

### `ptc-providers`

Built-in `AiProvider` implementations. The scaffold begins with a deterministic mock. Ollama and OpenAI-compatible HTTP adapters follow.

### `ptc-persistence`

Portable workflow loading and immutable JSON run artifacts. SQLite will index workflows, configuration, and run history without replacing the portable artifacts.

### `ptc-reporting`

Deterministic transformations from stored runs to Markdown and HTML. A report must be regenerable without the original AI conversation.

## Compatibility rules

- `schemaVersion` selects the parser and migration path.
- Workflow `id` is stable; `version` changes with behavior.
- Every run retains its workflow version and SHA-256 content hash.
- Completed run artifacts are immutable.
- Unknown fields fail validation in v1alpha1.
- New step kinds are added as tagged enum variants and executor adapters.

## Planned execution model

The initial executor supports `ai_prompt`. Later step types implement the same execution boundary:

- `command`
- `http_request`
- `transform`
- `approval`
- `report`

Branching and dependency graphs should be added only after sequential multi-step runs have stable semantics.

