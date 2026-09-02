use thiserror::Error;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("unsupported schema version: {0}")]
    UnsupportedSchema(String),
    #[error("invalid workflow: {0}")]
    InvalidWorkflow(String),
    #[error("invalid input `{name}`: {reason}")]
    InvalidInput { name: String, reason: String },
    #[error("unresolved template variable: {0}")]
    UnresolvedVariable(String),
    #[error("provider error: {0}")]
    Provider(String),
    #[error("repository error: {0}")]
    Repository(String),
}

