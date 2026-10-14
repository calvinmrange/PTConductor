use ptc_domain::RunRecord;

pub trait ReportRenderer {
    fn media_type(&self) -> &str;
    fn render(&self, run: &RunRecord) -> String;
}

#[derive(Debug, Clone, Default)]
pub struct MarkdownRenderer;
#[derive(Debug, Clone, Default)]
pub struct HtmlRenderer;

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
fn literal(value: &str) -> String {
    // Longer than every run of backticks in the evidence, so evidence cannot close the fence.
    let longest = value.split(|c| c != '`').map(str::len).max().unwrap_or(0);
    let fence = "`".repeat(3.max(longest + 1));
    format!("{fence}\n{value}\n{fence}\n")
}
fn metadata(run: &RunRecord) -> String {
    format!(
        "Run: {}\nWorkflow: {} v{}\nDefinition hash: {}\nStatus: {:?}\nStarted: {}\nCompleted: {}",
        run.id,
        run.workflow.id,
        run.workflow.version,
        run.workflow.content_hash,
        run.status,
        run.started_at,
        run.completed_at
            .map(|v| v.to_rfc3339())
            .unwrap_or_else(|| "Not completed".into())
    )
}
fn step_label(step: &ptc_domain::StepRun) -> String {
    format!(
        "Step: {}\nStatus: {:?}\nProvider: {}\nModel: {}\nStarted: {}\nCompleted: {}",
        step.id,
        step.status,
        step.provider,
        step.model.as_deref().unwrap_or("Not reported"),
        step.started_at,
        step.completed_at
            .map(|v| v.to_rfc3339())
            .unwrap_or_else(|| "Not completed".into())
    )
}
impl ReportRenderer for MarkdownRenderer {
    fn media_type(&self) -> &str {
        "text/markdown"
    }
    fn render(&self, run: &RunRecord) -> String {
        let mut output = format!(
            "# PTConductor Run Report\n\n{}\n## Redacted inputs\n\n{}",
            literal(&metadata(run)),
            literal(&serde_json::to_string_pretty(&run.inputs).unwrap_or_default())
        );
        for (i, step) in run.steps.iter().enumerate() {
            output.push_str(&format!(
                "\n## Step {}\n\n{}\n{}",
                i + 1,
                literal(&step_label(step)),
                literal(&step.output)
            ));
            if let Some(error) = &step.error {
                output.push_str(&format!("\nError:\n\n{}", literal(error)));
            }
        }
        if !run.findings.is_empty() {
            output.push_str(&format!(
                "\n## Findings\n\n{}",
                literal(&serde_json::to_string_pretty(&run.findings).unwrap_or_default())
            ));
        }
        output.push_str("\nAI summaries require operator review. Collector evidence and AI interpretation are recorded separately.\n");
        output
    }
}
impl ReportRenderer for HtmlRenderer {
    fn media_type(&self) -> &str {
        "text/html"
    }
    fn render(&self, run: &RunRecord) -> String {
        let mut output = String::from("<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width\"><meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; style-src 'unsafe-inline'\"><title>PTConductor Run Report</title><style>body{font:16px system-ui;max-width:1000px;margin:3rem auto;padding:0 1rem;color:#222}pre{white-space:pre-wrap;overflow-wrap:anywhere;padding:1rem;background:#f3f3f3;border:1px solid #ccc}h2{margin-top:2rem}.error{border-color:#b22}footer{margin-top:2rem;color:#555}@media print{pre{break-inside:auto}body{margin:0}}</style></head><body><h1>PTConductor Run Report</h1>");
        output.push_str(&format!(
            "<pre>{}</pre><h2>Redacted inputs</h2><pre>{}</pre>",
            escape(&metadata(run)),
            escape(&serde_json::to_string_pretty(&run.inputs).unwrap_or_default())
        ));
        for (i, step) in run.steps.iter().enumerate() {
            output.push_str(&format!(
                "<section><h2>Step {}</h2><pre>{}</pre><pre>{}</pre>",
                i + 1,
                escape(&step_label(step)),
                escape(&step.output)
            ));
            if let Some(error) = &step.error {
                output.push_str(&format!("<pre class=\"error\">{}</pre>", escape(error)));
            }
            output.push_str("</section>");
        }
        if !run.findings.is_empty() {
            output.push_str(&format!(
                "<h2>Findings</h2><pre>{}</pre>",
                escape(&serde_json::to_string_pretty(&run.findings).unwrap_or_default())
            ));
        }
        output.push_str("<footer>AI summaries require operator review. Collector evidence and AI interpretation are recorded separately.</footer></body></html>");
        output
    }
}

pub fn render_report(run: &RunRecord, format: &str) -> Result<String, String> {
    match format {
        "markdown" => Ok(MarkdownRenderer.render(run)),
        "html" => Ok(HtmlRenderer.render(run)),
        _ => Err("report format must be markdown or html".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn untrusted_evidence_cannot_escape_html_or_markdown() {
        let payload = "</pre><script>alert(1)</script>\n```\n# injected";
        assert!(!escape(payload).contains("<script>"));
        assert!(literal(payload).starts_with("````\n"));
        let raw = r#"{"schemaVersion":"ptconductor.dev/run/v1alpha1","id":"00000000-0000-0000-0000-000000000000","workflow":{"id":"sample","version":"1.0.0","contentHash":"hash"},"status":"failed","startedAt":"2026-10-14T00:00:00Z","inputs":{"TOKEN":"***REDACTED***"},"steps":[{"id":"analyze","status":"failed","startedAt":"2026-10-14T00:00:00Z","provider":"codex","output":"<script>bad</script>","error":"quota exceeded"}]}"#;
        let run: RunRecord = serde_json::from_str(raw).unwrap();
        for result in [MarkdownRenderer.render(&run), HtmlRenderer.render(&run)] {
            assert!(result.contains("quota exceeded"));
            assert!(result.contains("REDACTED"));
            assert!(result.contains("codex"));
        }
        assert!(!HtmlRenderer.render(&run).contains("<script>bad"));
        assert!(render_report(&run, "pdf").is_err());
    }
}
