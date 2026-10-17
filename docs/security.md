# Security boundaries

PTConductor is for explicitly authorized testing. The operator confirms the target and remote evidence transmission separately. This MVP validates a single target's syntax and bounds collection; it does not implement engagement allowlists or establish legal authorization automatically.

## Deterministic collection

- `nmap_scan` launches a fixed `nmap` executable via an argument array, never a shell. One IP/hostname, at most 1,024 TCP ports, TCP connect/light service detection, 60-second host timeout, 75-second process timeout and 192 KiB captured output. Target ranges/CIDR/options and arbitrary Nmap flags are rejected. No NSE scripts or exploit execution.
- `http_headers` makes one GET with a 20-second timeout, verifies TLS and refuses redirects. Only selected security/server headers are retained. Cookies, authentication headers and bodies are omitted. A redirect response is assessed as that response, not as the destination's policy.
- Both require `--allow-active` or desktop authorization. Codex readiness is checked before collection. `prepare` validates and previews without performing network activity.
- Evidence is untrusted data. Summaries must distinguish observations from assumptions; banners alone do not prove vulnerabilities. AI JSON is validated as an object, not as a verified vulnerability report. Findings remain inside step output; they are not automatically promoted to trusted domain findings.

## Credentials, persistence and rendering

- OpenAI API credentials are backend environment values. The GUI never receives those provider keys. User-supplied secret input fields do temporarily exist in GUI memory and are transmitted if included in a prompt the user authorizes.
- Declared secret input values are redacted from prepared output, stored input values, step JSON keys/values, text output and step errors. This is exact-value redaction, not detection of all sensitive data or encoded/transformed secrets. Evidence can still be sensitive; redact it before collection/transmission where necessary.
- JSON run files use atomic replacement and mode `0600` on Unix. JSON is written before SQLite metadata. Storage is not encrypted; protect the user account and backups. Exported reports and workflow definitions follow the user's filesystem permissions.
- HTML reports escape evidence and include a restrictive content security policy. Markdown uses dynamically sized code fences so embedded fences cannot break out. React displays results as text.
- Malformed run artifacts are preserved and skipped on import so other history stays usable. No automatic destructive recovery is performed.

## Codex process

The adapter checks ChatGPT login, strips API-key environment variables, ignores user CLI configuration, uses a private temporary directory, stdin prompts, an ephemeral read-only sandbox and no approvals. It reads only the final-message file, checks file type/size and bounds runtime/output. Raw child stderr is mapped to generic diagnostic guidance rather than exposed. The authenticated CLI itself manages credentials. No `danger-full-access`, automatic sandbox bypass or shell interpolation is used.

Codex may still have read tools in its sandbox; prompt instructions are not a security boundary. Review submitted evidence and generated findings. Install trusted Codex/Nmap executables and do not place untrusted directories on PATH. General shell commands, encrypted storage, engagement scope policy, multi-target scanning and automatic exploitation are outside this release.
