pub mod collectors;
mod error;
mod ports;
pub mod process;
mod resolver;

use std::collections::BTreeMap;

use chrono::Utc;
pub use error::EngineError;
pub use ports::{AiProvider, AiRequest, AiResponse, RunRepository, WorkflowRepository};
use ptc_domain::{
    InputKind, OutputFormat, RunRecord, RunStatus, StepDefinition, StepRun, WorkflowDefinition,
    WorkflowReference, RUN_SCHEMA_VERSION, WORKFLOW_SCHEMA_VERSION,
};
pub use resolver::{ResolvedInputs, VariableResolver};
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Default)]
pub struct RunOptions {
    pub allow_active: bool,
}

pub struct WorkflowEngine<P, R> {
    provider: P,
    runs: R,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedWorkflow {
    pub workflow_id: String,
    pub workflow_version: String,
    pub inputs: BTreeMap<String, Value>,
    pub steps: Vec<PreparedStep>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedStep {
    pub id: String,
    pub name: String,
    pub provider: Option<String>,
    pub output_format: OutputFormat,
    pub prompt_preview: String,
}

impl<P, R> WorkflowEngine<P, R>
where
    P: AiProvider,
    R: RunRepository,
{
    pub fn new(provider: P, runs: R) -> Self {
        Self { provider, runs }
    }

    pub async fn execute(
        &self,
        workflow: &WorkflowDefinition,
        values: BTreeMap<String, Value>,
        content_hash: String,
    ) -> Result<RunRecord, EngineError> {
        self.execute_with_options(workflow, values, content_hash, RunOptions::default())
            .await
    }

    pub async fn execute_with_options(
        &self,
        workflow: &WorkflowDefinition,
        values: BTreeMap<String, Value>,
        content_hash: String,
        options: RunOptions,
    ) -> Result<RunRecord, EngineError> {
        prepare_workflow(workflow, values.clone())?;
        if workflow.steps.iter().any(StepDefinition::is_active) && !options.allow_active {
            return Err(EngineError::InvalidWorkflow(
                "active collection requires --allow-active or the desktop authorization checkbox"
                    .into(),
            ));
        }
        let inputs = VariableResolver::resolve(&workflow.inputs, values)?;
        for step in &workflow.steps {
            if let StepDefinition::AiPrompt {
                provider: Some(name),
                ..
            } = step
            {
                if name != self.provider.id() {
                    return Err(EngineError::InvalidWorkflow(format!(
                        "step `{}` requires provider `{name}`",
                        step.id()
                    )));
                }
            }
        }
        if workflow.steps.iter().any(StepDefinition::is_active) {
            self.provider.check_ready().await?;
        }
        let started_at = Utc::now();
        let mut run = RunRecord {
            schema_version: RUN_SCHEMA_VERSION.to_owned(),
            id: Uuid::new_v4(),
            workflow: WorkflowReference {
                id: workflow.id.clone(),
                version: workflow.version.clone(),
                content_hash,
            },
            status: RunStatus::Running,
            started_at,
            completed_at: None,
            inputs: inputs.redacted_values(),
            steps: Vec::new(),
            findings: Vec::new(),
        };

        self.runs.save(&run).await?;

        for step in &workflow.steps {
            let step_started = Utc::now();
            let result: Result<AiResponse, EngineError> = async {
                match step {
                    StepDefinition::AiPrompt {
                        prompt,
                        output_format,
                        ..
                    } => {
                        let evidence = serde_json::to_string(&run.steps).map_err(|_| {
                            EngineError::Provider("cannot serialize evidence".into())
                        })?;
                        self.provider
                            .complete(AiRequest {
                                workflow_id: workflow.id.clone(),
                                step_id: step.id().into(),
                                prompt: inputs.with_evidence(evidence).render(prompt)?,
                                output_format: *output_format,
                            })
                            .await
                    }
                    StepDefinition::NmapScan { target, ports, .. } => {
                        let output =
                            collectors::nmap_scan(&inputs.render(target)?, &inputs.render(ports)?)
                                .await?;
                        Ok(AiResponse {
                            provider: "nmap".into(),
                            model: None,
                            content: output.to_string(),
                        })
                    }
                    StepDefinition::HttpHeaders { target, .. } => {
                        let output = collectors::security_headers(&inputs.render(target)?).await?;
                        Ok(AiResponse {
                            provider: "http".into(),
                            model: None,
                            content: output.to_string(),
                        })
                    }
                }
            }
            .await;
            let result = result.and_then(|mut response| {
                let json_output = !matches!(
                    step,
                    StepDefinition::AiPrompt {
                        output_format: OutputFormat::Text,
                        ..
                    }
                );
                response.content = if json_output {
                    let mut value: Value =
                        serde_json::from_str(&response.content).map_err(|_| {
                            EngineError::Provider(
                                "provider returned invalid JSON object output".into(),
                            )
                        })?;
                    if !value.is_object() {
                        return Err(EngineError::Provider(
                            "provider returned invalid JSON object output".into(),
                        ));
                    }
                    inputs.redact_json(&mut value);
                    serde_json::to_string_pretty(&value)
                        .map_err(|_| EngineError::Provider("cannot serialize JSON output".into()))?
                } else {
                    inputs.redact_output(&response.content)
                };
                Ok(response)
            });
            let (provider, model, output, error) = match result {
                Ok(response) => (response.provider, response.model, response.content, None),
                Err(error) => (
                    match step {
                        StepDefinition::NmapScan { .. } => "nmap",
                        StepDefinition::HttpHeaders { .. } => "http",
                        _ => self.provider.id(),
                    }
                    .into(),
                    None,
                    String::new(),
                    Some(inputs.redact_output(&error.to_string())),
                ),
            };
            run.steps.push(StepRun {
                id: step.id().into(),
                status: if error.is_some() {
                    RunStatus::Failed
                } else {
                    RunStatus::Completed
                },
                started_at: step_started,
                completed_at: Some(Utc::now()),
                provider,
                model,
                output,
                error: error.clone(),
            });
            if let Some(error) = error {
                run.status = RunStatus::Failed;
                run.completed_at = Some(Utc::now());
                self.runs.save(&run).await?;
                return Err(EngineError::Provider(format!(
                    "{error}; failed run saved as {}",
                    run.id
                )));
            }
            self.runs.save(&run).await?;
        }

        run.status = RunStatus::Completed;
        run.completed_at = Some(Utc::now());
        self.runs.save(&run).await?;
        Ok(run)
    }
}

pub fn validate_workflow(workflow: &WorkflowDefinition) -> Result<(), EngineError> {
    if workflow.schema_version != WORKFLOW_SCHEMA_VERSION {
        return Err(EngineError::UnsupportedSchema(
            workflow.schema_version.clone(),
        ));
    }
    if !is_slug(&workflow.id) {
        return Err(EngineError::InvalidWorkflow(
            "workflow id must use lowercase letters, numbers, and single hyphens".to_owned(),
        ));
    }
    if workflow.name.trim().is_empty() {
        return Err(EngineError::InvalidWorkflow(
            "workflow name must not be empty".to_owned(),
        ));
    }
    if !is_version(&workflow.version) {
        return Err(EngineError::InvalidWorkflow(
            "workflow version must contain three numeric components, such as 1.0.0".to_owned(),
        ));
    }
    if workflow.steps.is_empty() {
        return Err(EngineError::InvalidWorkflow(
            "workflow must contain at least one step".to_owned(),
        ));
    }

    let mut input_names = std::collections::BTreeSet::new();
    for input in &workflow.inputs {
        if !is_variable_name(&input.name) || input.name == "STEPS" {
            return Err(EngineError::InvalidWorkflow(format!(
                "input name `{}` must use uppercase letters, numbers, and underscores",
                input.name
            )));
        }
        if !input_names.insert(input.name.as_str()) {
            return Err(EngineError::InvalidWorkflow(format!(
                "duplicate input name: {}",
                input.name
            )));
        }
        if input.label.trim().is_empty() {
            return Err(EngineError::InvalidWorkflow(format!(
                "input `{}` must have a label",
                input.name
            )));
        }
        if input.kind == InputKind::Secret && input.default.is_some() {
            return Err(EngineError::InvalidWorkflow(format!(
                "secret input `{}` cannot define a default value",
                input.name
            )));
        }
        if let Some(default) = &input.default {
            resolver::validate_input_value(input, default, false).map_err(|error| {
                EngineError::InvalidWorkflow(format!(
                    "default for input `{}` is invalid: {error}",
                    input.name
                ))
            })?;
        }
    }

    let mut ids = std::collections::BTreeSet::new();
    for step in &workflow.steps {
        if !is_slug(step.id()) {
            return Err(EngineError::InvalidWorkflow(format!(
                "step id `{}` must use lowercase letters, numbers, and single hyphens",
                step.id()
            )));
        }
        if !ids.insert(step.id()) {
            return Err(EngineError::InvalidWorkflow(format!(
                "duplicate step id: {}",
                step.id()
            )));
        }
        if step.name().trim().is_empty() {
            return Err(EngineError::InvalidWorkflow(
                "step name must not be empty".into(),
            ));
        }
        let templates = match step {
            StepDefinition::AiPrompt {
                prompt, provider, ..
            } => {
                if prompt.trim().is_empty()
                    || provider.as_ref().is_some_and(|v| v.trim().is_empty())
                {
                    return Err(EngineError::InvalidWorkflow(
                        "prompt and provider must not be empty".into(),
                    ));
                }
                vec![prompt.as_str()]
            }
            StepDefinition::NmapScan { target, ports, .. } => vec![target.as_str(), ports.as_str()],
            StepDefinition::HttpHeaders { target, .. } => vec![target.as_str()],
        };
        for template in templates {
            for variable in template_variables(template) {
                if variable == "STEPS" && matches!(step, StepDefinition::AiPrompt { .. }) {
                    continue;
                }
                if !input_names.contains(variable.as_str()) {
                    return Err(EngineError::InvalidWorkflow(format!(
                        "step `{}` references undeclared input `{variable}`",
                        step.id()
                    )));
                }
            }
        }
    }
    Ok(())
}

pub fn prepare_workflow(
    workflow: &WorkflowDefinition,
    values: BTreeMap<String, Value>,
) -> Result<PreparedWorkflow, EngineError> {
    validate_workflow(workflow)?;
    let inputs = VariableResolver::resolve(&workflow.inputs, values)?
        .with_evidence("[Collected step evidence will be inserted at runtime]".into());
    let steps = workflow
        .steps
        .iter()
        .map(|step| match step {
            StepDefinition::NmapScan { id, name, target, ports } => {
                let target = inputs.render(target)?; let ports = inputs.render(ports)?;
                collectors::validate_target(&target)?; collectors::validate_ports(&ports)?;
                Ok(PreparedStep { id: id.clone(), name: name.clone(), provider: Some("nmap".into()), output_format: OutputFormat::Json, prompt_preview: inputs.redact_output(&format!("Nmap TCP connect / light service scan: {target}, ports {ports}; timeout 60s")) })
            }
            StepDefinition::HttpHeaders { id, name, target } => {
                let target = inputs.render(target)?; collectors::validate_url(&target)?;
                Ok(PreparedStep { id: id.clone(), name: name.clone(), provider: Some("http".into()), output_format: OutputFormat::Json, prompt_preview: inputs.redact_output(&format!("GET {target}; no redirects; response security headers only")) })
            }
            StepDefinition::AiPrompt {
                id,
                name,
                prompt,
                provider,
                output_format,
            } => Ok(PreparedStep {
                id: id.clone(),
                name: name.clone(),
                provider: provider.clone(),
                output_format: *output_format,
                prompt_preview: inputs.render_redacted(prompt)?,
            }),
        })
        .collect::<Result<Vec<_>, EngineError>>()?;

    Ok(PreparedWorkflow {
        workflow_id: workflow.id.clone(),
        workflow_version: workflow.version.clone(),
        inputs: inputs
            .redacted_values()
            .into_iter()
            .filter(|(name, _)| name != "STEPS")
            .collect(),
        steps,
    })
}

fn is_slug(value: &str) -> bool {
    !value.is_empty()
        && value.split('-').all(|part| {
            !part.is_empty()
                && part
                    .chars()
                    .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit())
        })
}

fn is_variable_name(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_')
        && value
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_uppercase())
}

fn is_version(value: &str) -> bool {
    let parts: Vec<_> = value.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.chars().all(|ch| ch.is_ascii_digit()))
}

fn template_variables(template: &str) -> Vec<String> {
    let mut variables = Vec::new();
    let mut remaining = template;
    while let Some(start) = remaining.find('<') {
        let after_start = &remaining[start + 1..];
        let Some(end) = after_start.find('>') else {
            break;
        };
        let candidate = &after_start[..end];
        if is_variable_name(candidate) {
            variables.push(candidate.to_owned());
        }
        remaining = &after_start[end + 1..];
    }
    variables
}

#[cfg(test)]
mod tests {
    use super::*;
    use ptc_domain::{InputDefinition, InputKind};

    fn workflow() -> WorkflowDefinition {
        WorkflowDefinition {
            schema_version: WORKFLOW_SCHEMA_VERSION.to_owned(),
            id: "endpoint-review".to_owned(),
            version: "1.0.0".to_owned(),
            name: "Endpoint review".to_owned(),
            description: None,
            inputs: vec![InputDefinition {
                name: "TARGET".to_owned(),
                label: "Target".to_owned(),
                kind: InputKind::Url,
                required: true,
                description: None,
                default: None,
            }],
            steps: vec![StepDefinition::AiPrompt {
                id: "review".to_owned(),
                name: "Review".to_owned(),
                prompt: "Review <TARGET>".to_owned(),
                provider: None,
                output_format: OutputFormat::Text,
            }],
        }
    }

    #[test]
    fn validates_declared_template_variables() {
        assert!(validate_workflow(&workflow()).is_ok());
        let mut invalid = workflow();
        let StepDefinition::AiPrompt { prompt, .. } = &mut invalid.steps[0] else {
            unreachable!()
        };
        *prompt = "Review <MISSING>".to_owned();
        assert!(validate_workflow(&invalid).is_err());
    }

    #[test]
    fn prepares_a_redacted_preview() {
        let mut definition = workflow();
        definition.inputs.push(InputDefinition {
            name: "TOKEN".to_owned(),
            label: "Token".to_owned(),
            kind: InputKind::Secret,
            required: true,
            description: None,
            default: None,
        });
        let StepDefinition::AiPrompt { prompt, .. } = &mut definition.steps[0] else {
            unreachable!()
        };
        *prompt = "Review <TARGET> with <TOKEN>".to_owned();
        let prepared = prepare_workflow(
            &definition,
            BTreeMap::from([
                (
                    "TARGET".to_owned(),
                    Value::String("https://example.test".to_owned()),
                ),
                ("TOKEN".to_owned(), Value::String("super-secret".to_owned())),
            ]),
        )
        .unwrap();
        assert!(!prepared.steps[0].prompt_preview.contains("super-secret"));
        assert_eq!(prepared.inputs["TOKEN"], "***REDACTED***");
    }
}
