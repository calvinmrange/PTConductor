# Week 5: OpenAI technology-analysis smoke test

This workflow **does not run WhatWeb, fetch a URL, or scan a target**. It interprets the headers and optional HTML excerpt you supply from an authorized engagement. Remove session cookies, tokens, user information, and other sensitive content before sending it to a remote provider. The OpenAI API has separate billing from a ChatGPT subscription.

From the repository root on Kali:

```bash
git pull
cargo test --workspace
cargo run -p ptconductor-cli -- workflow validate workflows/examples/technology-fingerprint.json

read -rsp 'OpenAI API key: ' OPENAI_API_KEY; export OPENAI_API_KEY; echo

cargo run -p ptconductor-cli -- prepare workflows/examples/technology-fingerprint.json \
  --input TARGET=https://example.test \
  --input 'HEADERS=Server: nginx' \
  --input 'HTML=<html><title>Test</title></html>'

cargo run -p ptconductor-cli -- run workflows/examples/technology-fingerprint.json \
  --provider openai --model gpt-6-luna --allow-remote \
  --input TARGET=https://example.test \
  --input 'HEADERS=Server: nginx' \
  --input 'HTML=<html><title>Test</title></html>' \
  --output-json runs/technology-result.json
```

Inspect `runs/technology-result.json` and the run ID printed to the terminal. The result should be a JSON object, with an nginx technology backed by the supplied `Server` header; it must not claim to have visited the URL. The `runs/<id>.json` artifact contains status and the JSON output as a string. `--output-json` refuses to overwrite an existing file, so choose a fresh path for subsequent tests.

For GUI testing, launch `npm run tauri dev` from `apps/desktop` **in the same terminal that has `OPENAI_API_KEY` exported**. Choose **Technology Fingerprint Analysis**, enter the same harmless observations, select **Validate & prepare**, review the preview, select OpenAI, confirm remote transmission, and select **Run workflow**. The JSON result and run ID should appear in the window. If the app was already running before the key was exported, restart it.

Troubleshooting: `OPENAI_API_KEY is not set` means the application process did not inherit the variable. HTTP 401/403 indicates a key or project access problem; HTTP 429 usually indicates rate limits or quota. A model or endpoint error may require choosing another model accessible to your API project. No API error body or key is printed. You can use `--provider mock` with the same workflow to validate the local flow without charges, or `--provider ollama --model <installed-model>` after installing and starting Ollama.
