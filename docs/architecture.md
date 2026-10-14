# Architecture

The React/Tauri desktop and Rust CLI compose the same engine, provider, persistence and report crates. The frontend calls narrow Tauri commands; execution decisions live in Rust.

| Component | Responsibility |
| --- | --- |
| `ptc-domain` | Versioned workflows, typed input definitions, tagged steps, runs and findings models. |
| `ptc-engine` | Validation, single-pass variable resolution, redaction, sequential execution, bounded Nmap/HTTP collectors, provider/repository traits. |
| `ptc-providers` | Mock, loopback Ollama, OpenAI-compatible HTTP and authenticated Codex CLI adapters. |
| `ptc-persistence` | Workflow files, SHA-256 hashes, atomic JSON run artifacts and SQLite history index. |
| `ptc-reporting` | Deterministic escaped Markdown/HTML exports from stored evidence and outputs. |
| `apps/cli` | Creation/list/validate/prepare/run/history/show/report/doctor commands and explicit consent flags. |
| `apps/desktop` | Workflow search and creation, generated forms, previews, provider settings, execution, history filters, report export and readiness checks. |

## Execution

1. Load and validate the workflow; resolve required/default typed inputs. Reject undeclared variables and invalid collector targets before any collection.
2. Check provider binding and active authorization. For active Codex workflows, verify installation/authentication before starting the scan.
3. Create a running JSON artifact and index it. Execute steps sequentially.
4. Collect bounded Nmap XML or selected HTTP headers. Redact and persist each completed step before invoking the next.
5. Resolve `<STEPS>` to JSON containing prior saved steps and send the prompt to the chosen provider. Variable substitution is single-pass; placeholders inside evidence are not evaluated. `<STEPS>` is reserved and cannot be a user input name.
6. Validate JSON-object outputs, redact results and persist completed state. On error, persist a failed step/run and return the saved run ID. Validation/authentication failures before a run starts do not produce a run artifact.
7. Generate reports later from stored runs without a new provider request.

The collector's raw evidence is inside the step output JSON; the AI summary is the following step. This makes the assessment reviewable even after a provider failure. Workflow version/hash, step provider/model, status and timestamps are retained. With Codex `default`, the exact resolved model is unknown to PTConductor and is stored as null rather than guessed.

## Storage and packaging

CLI paths are configurable through flags or environment variables. Desktop data lives in the OS application-data directory, not the source checkout. Five examples are embedded in the binary and seeded only when their files do not already exist. Existing customized examples are preserved. Set absolute workflow/run directory overrides to share data between interfaces.

JSON artifacts are authoritative. SQLite stores redacted summaries for sorted history and can be recreated from valid artifacts. Import skips malformed UUID-named artifacts with a warning. Interrupted processes may leave a run marked running; this is retained for operator review rather than falsely marked complete. See the recovery procedure in the Kali guide.

## Compatibility and scope

Schemas remain `v1alpha1`; existing `ai_prompt` workflows are unchanged. Added tagged step kinds are `nmap_scan` and `http_headers`. Unknown fields and unsupported step kinds fail validation. Definitions are sequential; branching, arbitrary command steps and automated vulnerability promotion are outside this MVP. The schema's structural rules are supplemented by runtime checks for identifiers, placeholders, filesystem inputs and bounded target/port values.
