mod error;
mod ports;
mod resolver;

use std::collections::BTreeMap;

use chrono::Utc;
pub use error::EngineError;
pub use ports::{AiProvider, AiRequest, AiResponse, RunRepository, WorkflowRepository};
use ptc_domain::{
    RunRecord, RunStatus, StepDefinition, StepRun, WorkflowDefinition, WorkflowReference,
    RUN_SCHEMA_VERSION, WORKFLOW_SCHEMA_VERSION,
};
pub use resolver::{ResolvedInputs, VariableResolver};
use serde_json::Value;
use uuid::Uuid;

pub struct WorkflowEngine<P, R> {
    provider: P,
    runs: R,
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
        validate_workflow(workflow)?;
        let inputs = VariableResolver::resolve(&workflow.inputs, values)?;
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
            let StepDefinition::AiPrompt {
                id,
                name: _,
                prompt,
                ..
            } = step;
            let step_started = Utc::now();
            let rendered_prompt = inputs.render(prompt)?;
            let response = match self
                .provider
                .complete(AiRequest {
                    workflow_id: workflow.id.clone(),
                    step_id: id.clone(),
                    prompt: rendered_prompt,
                })
                .await
            {
                Ok(response) => response,
                Err(error) => {
                    run.steps.push(StepRun {
                        id: id.clone(),
                        status: RunStatus::Failed,
                        started_at: step_started,
                        completed_at: Some(Utc::now()),
                        provider: self.provider.id().to_owned(),
                        output: String::new(),
                        error: Some(error.to_string()),
                    });
                    run.status = RunStatus::Failed;
                    run.completed_at = Some(Utc::now());
                    self.runs.save(&run).await?;
                    return Err(error);
                }
            };

            run.steps.push(StepRun {
                id: id.clone(),
                status: RunStatus::Completed,
                started_at: step_started,
                completed_at: Some(Utc::now()),
                provider: response.provider,
                output: response.content,
                error: None,
            });
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
    if workflow.id.trim().is_empty() || workflow.name.trim().is_empty() {
        return Err(EngineError::InvalidWorkflow(
            "workflow id and name must not be empty".to_owned(),
        ));
    }
    if workflow.steps.is_empty() {
        return Err(EngineError::InvalidWorkflow(
            "workflow must contain at least one step".to_owned(),
        ));
    }

    let mut ids = std::collections::BTreeSet::new();
    for step in &workflow.steps {
        if !ids.insert(step.id()) {
            return Err(EngineError::InvalidWorkflow(format!(
                "duplicate step id: {}",
                step.id()
            )));
        }
    }
    Ok(())
}
