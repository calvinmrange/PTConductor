import { test, expect, type Page } from "@playwright/test";

const xml = '<nmaprun><host><address addr="127.0.0.1"/><ports><extraports state="closed" count="2"/><port protocol="tcp" portid="8000"><state state="open"/><service name="http" product="Python http.server" version="3.12"/></port></ports></host></nmaprun>';
const assessment = {
  target: "127.0.0.1",
  summary: "One HTTP service is reachable on the authorized test host. Review its access requirements before exposing it beyond the lab.",
  findings: [{ title: "HTTP service exposed", severity: "medium", evidence: "Port 8000/tcp is open. Nmap identified Python http.server 3.12.", recommendation: "Restrict access to the intended test network." }],
  limitations: ["Service banners do not prove a vulnerability.", "Only the selected TCP ports were scanned."],
};

async function setup(page: Page, output: unknown = assessment, options: { raw?: boolean; http?: boolean; error?: string } = {}) {
  const collector = options.http ? { collector: "http_headers", url: "http://127.0.0.1:8000", status: 200, https: false, redirectFollowed: false, headers: { "x-content-type-options": ["nosniff"], "content-security-policy": ["default-src 'self'"] } }
    : { collector: "nmap", target: "127.0.0.1", ports: "8000", arguments: ["-sT", "-sV"], xml };
  const run = { id: "00000000-0000-4000-8000-000000000001", workflow: { id: "nmap-scan-summary", version: "1.0.0" }, status: options.error ? "failed" : "completed", startedAt: "2026-10-17T12:00:00Z", inputs: { TARGET: "127.0.0.1", TOKEN: "***REDACTED***" }, steps: [
    { id: "collect", status: "completed", provider: options.http ? "http" : "nmap", output: JSON.stringify(collector) },
    { id: "summarize", status: options.error ? "failed" : "completed", provider: "codex", output: options.raw ? String(output) : JSON.stringify(output), error: options.error },
  ] };
  await page.addInitScript(({ run }) => {
    const workflow = { schemaVersion: "ptconductor.dev/workflow/v1alpha1", id: "nmap-scan-summary", version: "1.0.0", name: "Nmap Scan Summary", inputs: [], steps: [{ id: "collect", name: "Collect", type: "nmap_scan" }, { id: "summarize", name: "Summarize", type: "ai_prompt" }] };
    Object.assign(window, { __TAURI_INTERNALS__: { invoke: async (command: string) => {
      if (command === "list_workflows") return [{ ...workflow, path: "fixture.json", inputCount: 0, stepCount: 2 }];
      if (command === "load_workflow") return { path: "fixture.json", definition: workflow };
      if (command === "prepare_workflow") return { workflowId: workflow.id, inputs: {}, steps: [] };
      if (command === "run_workflow" || command === "get_run") return run;
      if (command === "list_runs") return [{ ...run, workflowId: workflow.id, provider: "codex" }];
      throw new Error(`Unexpected test command: ${command}`);
    } } });
  }, { run });
  await page.goto("/");
  await expect(page.getByRole("button", { name: "Run history", exact: true })).toBeEnabled();
}
async function openHistory(page: Page) {
  await page.getByRole("button", { name: "Run history", exact: true }).click();
  await page.locator(".history-list").getByRole("button", { name: /nmap-scan-summary/ }).click();
}

test("history shows cards, preserved raw output and Nmap evidence table", async ({ page }) => {
  await setup(page);
  await openHistory(page);
  await expect(page.getByRole("heading", { name: "HTTP service exposed", exact: true })).toBeVisible();
  await expect(page.locator(".result-badge.tone-medium")).toHaveText("Medium");
  await expect(page.locator("dt", { hasText: /^Evidence$/ })).toBeVisible();
  await expect(page.getByText(assessment.findings[0].evidence, { exact: true })).toBeVisible();
  await expect(page.locator(".run-results pre:visible")).toHaveCount(0);
  await page.getByText("Collected evidence", { exact: false }).first().click();
  await expect(page.getByRole("cell", { name: "8000/tcp", exact: true })).toBeVisible();
  await expect(page.getByRole("cell", { name: "Python http.server 3.12", exact: true })).toBeVisible();
  await page.getByText("View raw JSON", { exact: true }).first().click();
  await expect(page.locator(".raw-data pre").first()).toHaveText(JSON.stringify(assessment));
});

test("new run uses the same presentation", async ({ page }) => {
  await setup(page);
  await page.getByRole("button", { name: /Validate & prepare/ }).click();
  await page.getByRole("checkbox", { name: /I authorize active/ }).check();
  await page.getByRole("checkbox", { name: /I authorize sending/ }).check();
  await page.getByRole("button", { name: /Run workflow/ }).click();
  await expect(page.getByRole("heading", { name: "HTTP service exposed", exact: true })).toBeVisible();
  await expect(page.locator(".result-summary")).toContainText(assessment.summary);
});

test("arbitrary nested fields, empty values and hostile strings stay readable and inert", async ({ page }) => {
  const hostile = '<img src=x onerror="window.injected=true">';
  await setup(page, { summary: hostile, findings: [], evidence: { requestId: "abc-123", valid: false, count: 0, version: null, notes: ["first", { detail: "nested evidence" }] }, custom_property: "preserved" });
  await openHistory(page);
  await expect(page.getByText(hostile, { exact: true })).toBeVisible();
  await expect(page.locator(".run-results img")).toHaveCount(0);
  await expect(page.getByText("No findings reported.", { exact: true })).toBeVisible();
  await expect(page.getByText("Not provided", { exact: true })).toBeVisible();
  await expect(page.locator("dt", { hasText: /^Request Id$/ })).toBeVisible();
  await expect(page.getByText("nested evidence", { exact: true })).toBeVisible();
  await expect(page.getByText("preserved", { exact: true })).toBeVisible();
  await expect(page.getByText("No", { exact: true })).toBeVisible();
  expect(await page.evaluate(() => (window as unknown as { injected?: boolean }).injected)).toBeUndefined();
});

test("header evidence and plain text failures remain accessible", async ({ page }) => {
  await setup(page, "Invalid JSON from provider: {unfinished", { raw: true, http: true, error: "Provider returned invalid JSON" });
  await openHistory(page);
  await expect(page.getByRole("alert")).toHaveText("Provider returned invalid JSON");
  await expect(page.getByText("Invalid JSON from provider: {unfinished", { exact: true })).toBeVisible();
  await page.getByText("Collected evidence", { exact: false }).first().click();
  await expect(page.getByText("nosniff", { exact: true })).toBeVisible();
  await expect(page.locator("dt", { hasText: /^X content type options$/ })).toBeVisible();
});

test("large lists expand and the minimum desktop width does not overflow", async ({ page }) => {
  await page.setViewportSize({ width: 900, height: 700 });
  await setup(page, { observations: Array.from({ length: 55 }, (_, i) => `Observation ${i + 1}`), evidence: "a".repeat(400) });
  await openHistory(page);
  await expect(page.getByText("Observation 55", { exact: true })).toHaveCount(0);
  await page.getByRole("button", { name: "Show more (5 remaining)" }).click();
  await expect(page.getByText("Observation 55", { exact: true })).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBeTruthy();
});

test("visual review fixture", async ({ page }, testInfo) => {
  await setup(page);
  await openHistory(page);
  await page.locator(".run-results").screenshot({ path: testInfo.outputPath("readable-results.png") });
});
