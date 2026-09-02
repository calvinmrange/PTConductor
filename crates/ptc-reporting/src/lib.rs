use ptc_domain::RunRecord;

pub trait ReportRenderer {
    fn media_type(&self) -> &str;
    fn render(&self, run: &RunRecord) -> String;
}

#[derive(Debug, Clone, Default)]
pub struct MarkdownRenderer;

impl ReportRenderer for MarkdownRenderer {
    fn media_type(&self) -> &str {
        "text/markdown"
    }

    fn render(&self, run: &RunRecord) -> String {
        let mut output = format!(
            "# PTConductor Run {}\n\n- Workflow: `{}` v{}\n- Status: `{:?}`\n- Started: {}\n",
            run.id, run.workflow.id, run.workflow.version, run.status, run.started_at
        );

        for step in &run.steps {
            output.push_str(&format!(
                "\n## Step: {}\n\nProvider: `{}`\n\n{}\n",
                step.id, step.provider, step.output
            ));
        }

        if !run.findings.is_empty() {
            output.push_str("\n## Findings\n");
            for finding in &run.findings {
                output.push_str(&format!(
                    "\n### {} ({:?})\n\n{}\n\n**Recommendation:** {}\n",
                    finding.title,
                    finding.severity,
                    finding.description,
                    finding.recommendation
                ));
            }
        }
        output
    }
}

