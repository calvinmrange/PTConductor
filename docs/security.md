# Security model

PTConductor is designed for explicitly authorized penetration testing. Safety controls belong in deterministic code, not prompt text.

## Current boundary

The workflow engine permits only AI prompt steps; it has no active scan or command step. The optional Codex provider launches the Codex agent locally. Even in a read-only sandbox, an agent can use available read tools; treat supplied observations as untrusted and inspect generated output.

## Required controls

- The frontend calls narrow Rust commands and never receives raw provider credentials.
- Secret inputs are marked at definition time and replaced with `***REDACTED***` in stored run inputs.
- Logs and errors pass through a centralized redaction layer before release builds.
- Remote providers must be visibly identified before potentially sensitive target data is transmitted.
- Week 5 provider execution uses a separate explicit remote-transmission confirmation. API keys are read only by the Rust backend from `OPENAI_API_KEY`; the frontend never receives them. Remove tokens, cookies, and personal data from observations before sending a prompt.
- The Codex provider requires ChatGPT CLI sign-in and rejects API-key sign-in. It removes `OPENAI_API_KEY` and `CODEX_API_KEY` from the child environment, passes prompts via stdin, disables user CLI configuration, chooses a read-only sandbox and no approvals, and uses an empty temporary working directory. These controls limit accidental access and writes but are not a guarantee that an agent will obey prompt instructions. No Codex credential file is read by PTConductor.
- Active steps must pass engagement-scope validation before execution.
- AI responses are untrusted content; application state is produced by deterministic code.
- Structured AI suggestions are schema-validated before being promoted to findings.

## Future command execution

Command steps must launch a fixed executable with an argument array rather than interpolating a shell command. Every invocation requires:

- execution mode and scope authorization
- explicit executable allowlisting or operator approval
- bounded runtime and output size
- controlled working directory and environment
- captured exit status, stdout, stderr, timestamps, and purpose
- secret-aware redaction

Dry-run and documentation modes must remain available even when active execution is implemented.
