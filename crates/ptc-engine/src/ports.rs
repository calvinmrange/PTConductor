use async_trait::async_trait;
use ptc_domain::{OutputFormat, RunRecord, WorkflowDefinition};
use uuid::Uuid;

use crate::EngineError;

#[derive(Debug, Clone)]
pub struct AiRequest {
    pub workflow_id: String,
    pub step_id: String,
    pub prompt: String,
    pub output_format: OutputFormat,
}

#[derive(Debug, Clone)]
pub struct AiResponse {
    pub provider: String,
    pub model: Option<String>,
    pub content: String,
}

#[async_trait]
pub trait AiProvider: Send + Sync {
    fn id(&self) -> &str;
    fn is_remote(&self) -> bool;
    async fn complete(&self, request: AiRequest) -> Result<AiResponse, EngineError>;
}

#[async_trait]
pub trait WorkflowRepository: Send + Sync {
    async fn list(&self) -> Result<Vec<WorkflowDefinition>, EngineError>;
    async fn get(&self, id: &str) -> Result<Option<WorkflowDefinition>, EngineError>;
}

#[async_trait]
pub trait RunRepository: Send + Sync {
    async fn save(&self, run: &RunRecord) -> Result<(), EngineError>;
    async fn get(&self, id: Uuid) -> Result<Option<RunRecord>, EngineError>;
}
