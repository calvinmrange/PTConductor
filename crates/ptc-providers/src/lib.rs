use async_trait::async_trait;
use ptc_engine::{AiProvider, AiRequest, AiResponse, EngineError};

/// Deterministic provider used for local development and engine tests.
#[derive(Debug, Clone, Default)]
pub struct MockProvider;

#[async_trait]
impl AiProvider for MockProvider {
    fn id(&self) -> &str {
        "mock"
    }

    fn is_remote(&self) -> bool {
        false
    }

    async fn complete(&self, request: AiRequest) -> Result<AiResponse, EngineError> {
        Ok(AiResponse {
            provider: self.id().to_owned(),
            model: Some("deterministic-mock".to_owned()),
            content: format!(
                "Mock analysis for workflow `{}` step `{}`:\n\n{}",
                request.workflow_id, request.step_id, request.prompt
            ),
        })
    }
}
