# Codex CLI smoke test on Kali

PTConductor's Codex provider uses a local Codex CLI **signed in with ChatGPT**, so it uses your Codex plan allowance rather than OpenAI Platform API credits. The prompt and supplied observations go to the ChatGPT service. This workflow asks for WhatWeb-style analysis of supplied evidence; it does not intentionally fetch or scan the target. Use harmless authorized observations and remove tokens, cookies, and personal data first. Codex is an agent and may use read tools even in its read-only sandbox; prompt instructions alone are not a security boundary.

From the PTConductor repository root in a Kali terminal:

```bash
git pull
npm install -g @openai/codex
codex login
codex login status
cargo test --workspace
cargo run -p ptconductor-cli -- workflow validate workflows/examples/technology-fingerprint.json

cargo run -p ptconductor-cli -- run workflows/examples/technology-fingerprint.json \
  --provider codex --model gpt-6-sol --allow-remote \
  --input TARGET=https://example.test \
  --input 'HEADERS=Server: nginx' \
  --input 'HTML=<html><title>Test</title></html>' \
  --output-json runs/codex-technology-result.json
```

At login choose **Sign in with ChatGPT**. If the browser callback does not work in your VM, run `codex login --device-auth` instead. `codex login status` must report a ChatGPT sign-in. If it reports API key authentication, run `codex logout` followed by `codex login` and select ChatGPT. PTConductor rejects API-key Codex sessions and removes API key environment variables from the Codex child process. Do not copy your Codex credential files into the project or paste them into chat.

Inspect `runs/codex-technology-result.json` and the run ID printed in the CLI. The response must be a JSON object with evidence for any claimed technologies; it must not claim to have visited `example.test`. The run artifact is stored at `runs/<id>.json`. `--output-json` refuses to overwrite an existing file, so use a new name for another run. If Codex returns prose instead of a JSON object, the engine marks the run failed and saves an error artifact.

For the desktop, launch `npm run tauri dev` from `apps/desktop` in a terminal where `codex login status` works. Select **Technology Fingerprint Analysis**, enter the same observations, select **Validate & prepare**, choose **Codex (ChatGPT sign-in)**, confirm remote transmission, and select **Run workflow**. No `OPENAI_API_KEY` is needed.

If you see `Codex CLI not found`, install it and make sure `codex` is on `PATH` in the terminal launching PTConductor. If the login check fails, verify that the CLI uses ChatGPT rather than an API key. Codex plan usage limits can still interrupt runs; an API billing balance does not affect this route. The adapter deliberately omits Codex stderr from artifacts and the UI to avoid echoing submitted observations.
