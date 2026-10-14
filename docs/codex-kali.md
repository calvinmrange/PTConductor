# Codex CLI integration on Kali

PTConductor invokes the installed Codex CLI using its existing ChatGPT login. It does not use an OpenAI Platform API key or read/copy Codex credential files. Requests still go to OpenAI and consume whatever Codex allowance is available to that account.

## Setup and preflight

Use the same regular Kali account for login and for launching PTConductor:

```bash
npm install -g @openai/codex@latest
codex --version
codex login --device-auth
codex login status
cargo build --locked -p ptconductor-cli
./target/debug/ptconductor doctor
```

Interactive `codex login` is also supported. If status reports API-key authentication, sign out with `codex logout` and sign in with ChatGPT. PTConductor rejects API-key sessions for the Codex provider and removes `OPENAI_API_KEY` and `CODEX_API_KEY` from its child environment.

The adapter requires `exec --help` to advertise `--output-last-message`, `--ephemeral` and `--ignore-user-config`. Update Codex if preflight reports missing flags. The adapter uses `default` to omit the model flag, avoiding a hardcoded model that may be unavailable. Because user configuration is ignored, this means the CLI default, not a custom configured provider/model. An explicit model can be selected in either interface.

If a desktop launch cannot find Codex, launch from the logged-in terminal or set an absolute executable path before starting it:

```bash
export PTCONDUCTOR_CODEX_BIN="$(command -v codex)"
export PTCONDUCTOR_WORKFLOWS_DIR="$PWD/workflows/examples"
export PTCONDUCTOR_RUNS_DIR="$PWD/runs"
cd apps/desktop
npm run tauri dev
```

Do not use `sudo` to launch the desktop or CLI: that changes the account, PATH and authentication location. The TCP connect Nmap collector does not need root privileges.

## Live acceptance

1. Start `python3 -m http.server 8000 --bind 127.0.0.1` in a separate terminal.
2. Run both README examples with `--provider codex`; use `PORTS=8000` for a demonstrably open Nmap port. Use fresh `--output-json` filenames each time.
3. Confirm two completed steps: collection followed by a Codex JSON object. Inspect the first step's XML/headers and check that the summary matches them.
4. Run `history`, `show RUN_ID`, then export Markdown and HTML. Stop and restart the app; the same run must remain available.
5. In the desktop select either example, leave model `default`, click **Check Codex**, **Validate & prepare**, confirm active authorization and remote transmission, then **Run workflow**. Open the run in history and export both reports. Report location appears in the UI.

The mock and simulated-Codex tests cannot establish account entitlement or model response quality. A successful live run in your Kali VM is the final integration acceptance step.

## Failure handling

| Symptom | Action |
| --- | --- |
| CLI not found / unresponsive | Check `command -v codex`, PATH and `PTCONDUCTOR_CODEX_BIN`; update Codex. |
| Login rejected | Run `codex login status` in the same account; use ChatGPT authentication. |
| Unsupported option | Update `@openai/codex`; `doctor` detects the required flags. |
| Model unavailable | Set model to `default` or choose a model your Codex CLI supports. |
| Quota / rate limit | Wait for the account's allowance to recover or select local Ollama; API credits do not change this route. |
| Sandbox failure | Update Codex and check the VM's user namespace / sandbox support. The adapter does not disable the sandbox. |
| Timeout | Check connectivity; reduce evidence or the scan workload. Codex inference has a four-minute process limit. |
| Missing, empty or invalid JSON response | Inspect the failed run in history; retry after checking the workflow prompt and model availability. |

Codex runs in a private temporary directory, read-only sandbox, no interactive approvals, ephemeral mode and with user configuration ignored. The prompt is sent through stdin. Only the `--output-last-message` file is consumed; stdout progress and raw stderr are never stored as the answer. Temporary files are removed after success or failure. Prompts are capped at 256 KiB, final responses and each captured stream at 1 MiB.

Read-only sandboxing and prompt instructions are not a guarantee that an agent cannot read accessible files. Use an ordinary, dedicated testing account and review the evidence you authorize for remote transmission. PTConductor performs the bounded scan/HTTP requests itself; Codex is asked only to assess the resulting evidence.
