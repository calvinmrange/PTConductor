# Final sprint acceptance and demonstration

This checklist covers the Week 7 reporting/security/interface goals and Week 8 integration, documentation, packaging and demonstration goals. Calvin performs the Kali acceptance tests; the instructor reviews the demonstration. Implementation and automated checks do not by themselves establish a successful live Codex session.

## Test coverage

| Area | Check | Expected result / evidence |
| --- | --- | --- |
| Workflows | List all five examples; create a prompt workflow; reject duplicate ID, unknown fields/step type and undeclared placeholders. | Valid definitions load in both interfaces; errors are actionable; no file overwrite. |
| Typed inputs | Required/optional text, URL, number, boolean, existing/missing file and secret. Try one numeric-looking text port (`PORTS=8000`). | GUI/CLI produce equivalent typed values; invalid input stops before activity. File contents are not implicitly uploaded. |
| Preparation | Prepare both active examples; include a secret in a prompt. | Redacted preview; no target request or inference. |
| Codex readiness | Missing executable, outdated flags, ChatGPT vs API-key login, executable path with spaces. | Helpful errors; no scan when preflight fails; no direct API-key route. |
| Live Codex | Run both collectors with the real CLI signed in to ChatGPT, model `default`. | Evidence collected, JSON summary received, two completed steps; account/model allowance verified in Kali. |
| Nmap | Loopback HTTP port, closed port, invalid target/ports/range, missing Nmap and failed process. | Fixed TCP scan; actual XML retained; no shell expansion; failure persisted; no AI call after collector failure. |
| HTTP | Local 200 response with security headers and cookie; 302 redirect; unreachable endpoint. | Selected headers only; cookie/body omitted; redirect not followed; failure saved. |
| Providers | Mock smoke test; available Ollama model; OpenAI API only if funded credentials are available. | Correct adapter/model; JSON validation; remote confirmation for Codex/OpenAI. Record unavailable optional live routes as not tested. |
| Error handling | Simulated quota, missing/empty/invalid/oversized Codex result, process timeout/output cap. | Error guidance without raw stderr; saved failed run and completed collector evidence; temp directory removed. |
| Persistence | Restart app, reload history, compare CLI/GUI using same run directory, rebuild SQLite after backup, add malformed JSON artifact. | Runs/status/times/outputs retained; valid history survives corrupt neighboring artifact. |
| Redaction | Secret echoed in input copy, JSON/text result, key, preview and error. | Exact secret absent from stored runs/index/reports and preview; no claim of encoded-secret detection. |
| Reports | Export Markdown/HTML from success and failure; evidence includes script tags and backticks. | Metadata, timestamps, outputs/errors intact; escaped HTML; intact Markdown fences; no model call; no overwrite. |
| GUI | Search workflows, create, validate, change provider, authorize, run, history filters, inspect and export. | No duplicate submissions while busy; clear status/errors; evidence and summary readable. |
| Packaged Kali app | Install `.deb`, launch outside source checkout, restart, verify all five seeded examples and persisted data. | Writable user data paths; no build-machine source dependency; Codex found under same user. |
| CLI archive | Extract, launch from another directory, list examples, run mock with absolute workflow path. | Binary/examples/docs present; launcher resolves data paths consistently. |

Automated gates: Rust unit/provider/persistence/report tests, Clippy, frontend TypeScript/Vite build, Python CLI contracts, real loopback Nmap in CI, and a Linux Tauri package build. The Python suite intentionally substitutes Codex to make failures reproducible without consuming allowance. Manual cases above cover actual model quality, native GUI interaction and installed Kali behavior.

## Week 7 and Week 8 completion schedule

| When | Work / deliverable | Exit condition |
| --- | --- | --- |
| Week 7 closeout | Markdown/HTML exports, GUI search/navigation/error handling, active collectors, secret/error/rendering checks. | Code and automated regression checks pass; sample evidence is reviewable. |
| Week 8 beginning | Codex compatibility, deterministic integration tests, real loopback Nmap CI, packaging/data-path fixes. | CI builds CLI and desktop packages and tests the complete saved-run path. |
| Week 8 middle | Calvin installs in Kali and runs both live Codex workflows plus GUI/history/report acceptance. | Capture test outcomes, run IDs, screenshots and any remaining defects; fix blocking defects. |
| Week 8 end | Regression of fixes, final docs/package, demo rehearsal and instructor presentation. | No unresolved blocker in load → collect → summarize → save → history → report; any limitations explicitly recorded. |

## Demonstration script

1. Show the repository, shared-engine architecture and `ptconductor doctor` success.
2. Start a local Python HTTP server on port 8000. Show the Nmap workflow inputs and redacted preview.
3. Run Nmap against `127.0.0.1`, `PORTS=8000`, using Codex. Show XML evidence and compare the JSON summary to it.
4. Run the security-header workflow against `http://127.0.0.1:8000`. Explain that missing headers on this intentionally simple server are observations, not demonstrated exploits; HSTS is not enforced over HTTP.
5. Reopen both saved runs in desktop history; export and open Markdown/HTML reports. Show timestamps, provider and workflow hash.
6. Demonstrate one input/consent error without starting a scan. Restart and show history remains available.
7. If live allowance/network fails, show the saved evidence and clearly label a mock run as an offline demonstration. Do not present a mock summary as real model analysis.

## Result record

For each manual check record the build commit, Kali version/architecture, Codex version, expected/actual outcome, pass/fail/not-tested, run ID and issue reference if applicable. Do not include credentials or private targets. Final release acceptance requires the live Codex and packaged Kali GUI checks; they cannot be truthfully marked passed by the offline suite.
