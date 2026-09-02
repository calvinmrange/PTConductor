use std::{fs, path::PathBuf};

use async_trait::async_trait;
use ptc_domain::{RunRecord, WorkflowDefinition};
use ptc_engine::{EngineError, RunRepository, WorkflowRepository};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct FileWorkflowRepository {
    root: PathBuf,
}

impl FileWorkflowRepository {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
}

#[async_trait]
impl WorkflowRepository for FileWorkflowRepository {
    async fn list(&self) -> Result<Vec<WorkflowDefinition>, EngineError> {
        if !self.root.exists() {
            return Ok(Vec::new());
        }

        let mut workflows = Vec::new();
        for entry in fs::read_dir(&self.root).map_err(repository_error)? {
            let path = entry.map_err(repository_error)?.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let contents = fs::read_to_string(path).map_err(repository_error)?;
            workflows.push(serde_json::from_str(&contents).map_err(repository_error)?);
        }
        workflows.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(workflows)
    }

    async fn get(&self, id: &str) -> Result<Option<WorkflowDefinition>, EngineError> {
        Ok(self.list().await?.into_iter().find(|flow| flow.id == id))
    }
}

#[derive(Debug, Clone)]
pub struct JsonRunRepository {
    root: PathBuf,
}

impl JsonRunRepository {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn path_for(&self, id: Uuid) -> PathBuf {
        self.root.join(format!("{id}.json"))
    }
}

#[async_trait]
impl RunRepository for JsonRunRepository {
    async fn save(&self, run: &RunRecord) -> Result<(), EngineError> {
        fs::create_dir_all(&self.root).map_err(repository_error)?;
        let destination = self.path_for(run.id);
        let temporary = destination.with_extension("json.tmp");
        let contents = serde_json::to_vec_pretty(run).map_err(repository_error)?;
        fs::write(&temporary, contents).map_err(repository_error)?;
        if destination.exists() {
            fs::remove_file(&destination).map_err(repository_error)?;
        }
        fs::rename(temporary, destination).map_err(repository_error)
    }

    async fn get(&self, id: Uuid) -> Result<Option<RunRecord>, EngineError> {
        let path = self.path_for(id);
        if !path.exists() {
            return Ok(None);
        }
        let contents = fs::read_to_string(path).map_err(repository_error)?;
        serde_json::from_str(&contents)
            .map(Some)
            .map_err(repository_error)
    }
}

fn repository_error(error: impl std::fmt::Display) -> EngineError {
    EngineError::Repository(error.to_string())
}
