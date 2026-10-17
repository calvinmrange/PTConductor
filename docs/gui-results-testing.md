# Readable desktop results: testing

The desktop now displays JSON properties as labeled fields, findings/technologies as cards, severity as badges, and lists as readable bullet points. Assessments appear first; collected evidence is expandable. Nmap XML is summarized in a port/service table, and HTTP headers appear as labeled values. Custom properties, nested data, empty values, plain text and errors are supported. Raw JSON and scan XML remain available under disclosure controls. Both new results and saved history use the same component; existing run files need no migration and no new model request.

## Manual Kali check

Close the running desktop and update the branch:

```bash
cd ~/PTConductor
git fetch origin
git switch week7-8-kali-release
git pull --ff-only origin week7-8-kali-release
export PTCONDUCTOR_CODEX_BIN="$(command -v codex)"
export PTCONDUCTOR_WORKFLOWS_DIR="$PWD/workflows/examples"
export PTCONDUCTOR_RUNS_DIR="$PWD/runs"
cd apps/desktop
npm ci
npm run tauri dev
```

1. Open **Run history** and select an existing Nmap or security-header run. The assessment should appear before the collected evidence.
2. Look for **Summary**, finding titles, severity badges, **Evidence**, **Recommendation**, and **Limitations** wherever those properties exist in your response. Values should be readable text rather than JSON syntax. Properties are displayed only when present; the UI does not invent missing findings or severity.
3. Expand **Collected evidence**. For Nmap, confirm the host, port, state, service and version match the saved XML. For headers, confirm the returned header names/values are readable. Closed/filtered port groups are also shown if Nmap reports them.
4. Expand **View raw JSON** and **View scan XML** to compare original evidence. Collapse them and verify the friendly view remains available.
5. Open a failed or text-output run. Errors and plain text should remain visible. Empty findings should say **No findings reported**. Open **Run inputs · secrets redacted** to inspect input values.
6. Run one new workflow and confirm its immediate result uses the same layout. Resize the desktop to its minimum width and check text wrapping and the horizontally scrollable port table.
7. Export a report to confirm export still works. This change affects the desktop presentation; stored JSON, CLI output and report formats are unchanged.

Opening existing runs incurs no Codex usage. To generate fresh test evidence, use the loopback server and workflows in [codex-kali.md](codex-kali.md).

## Automated browser checks

From `apps/desktop`:

```bash
npm ci
npx playwright install --with-deps chromium
npm run test:ui
npm run build
```

The tests mount the real app with a simulated Tauri command bridge. They cover both result locations, finding/evidence rendering, raw disclosures, nested/custom/empty values, safe literal rendering of hostile HTML, plain text failures, HTTP headers, Nmap tables, long lists and minimum-width layout. They neither contact a provider nor scan a target. A screenshot from the representative result view is written under `test-results` for visual review. The native Kali GUI remains the acceptance environment.
