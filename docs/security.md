# Security model

PTConductor is designed for explicitly authorized penetration testing. Safety controls belong in deterministic code, not prompt text.

## Current boundary

The scaffold permits only AI prompt steps. It does not execute shell commands, send testing HTTP requests, or perform active scanning.

## Required controls

- The frontend calls narrow Rust commands and never receives raw provider credentials.
- Secret inputs are marked at definition time and replaced with `***REDACTED***` in stored run inputs.
- Logs and errors pass through a centralized redaction layer before release builds.
- Remote providers must be visibly identified before potentially sensitive target data is transmitted.
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

