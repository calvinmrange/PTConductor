use std::{
    fs,
    path::{Path, PathBuf},
};

use async_trait::async_trait;
use ptc_domain::{RunRecord, WorkflowDefinition};
use ptc_engine::{validate_workflow, EngineError, RunRepository, WorkflowRepository};
use sha2::{Digest, Sha256};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct FileWorkflowRepository {
    root: PathBuf,
}

impl FileWorkflowRepository {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn discover(&self) -> Result<Vec<LoadedWorkflow>, EngineError> {
        if !self.root.exists() {
            return Ok(Vec::new());
        }
        let mut paths = Vec::new();
        collect_json_files(&self.root, &mut paths)?;
        let mut workflows = paths
            .iter()
            .map(load_workflow_file)
            .collect::<Result<Vec<_>, _>>()?;
        workflows.sort_by(|left, right| left.definition.name.cmp(&right.definition.name));
        Ok(workflows)
    }

    pub fn create(&self, workflow: &WorkflowDefinition) -> Result<LoadedWorkflow, EngineError> {
        save_workflow_file(&self.root, workflow)
    }
}

#[derive(Debug, Clone)]
pub struct LoadedWorkflow {
    pub path: PathBuf,
    pub definition: WorkflowDefinition,
    pub content_hash: String,
}

pub fn load_workflow_file(path: impl AsRef<Path>) -> Result<LoadedWorkflow, EngineError> {
    let path = path.as_ref();
    let contents = fs::read(path).map_err(repository_error)?;
    let definition: WorkflowDefinition = serde_json::from_slice(&contents)
        .map_err(|error| EngineError::InvalidWorkflow(format!("{}: {error}", path.display())))?;
    validate_workflow(&definition)?;
    Ok(LoadedWorkflow {
        path: path.to_path_buf(),
        definition,
        content_hash: format!("sha256:{:x}", Sha256::digest(&contents)),
    })
}

pub fn save_workflow_file(
    root: impl AsRef<Path>,
    workflow: &WorkflowDefinition,
) -> Result<LoadedWorkflow, EngineError> {
    validate_workflow(workflow)?;
    let root = root.as_ref();
    fs::create_dir_all(root).map_err(repository_error)?;
    let path = root.join(format!("{}.json", workflow.id));
    if path.exists() {
        return Err(EngineError::Repository(format!(
            "workflow already exists: {}",
            path.display()
        )));
    }
    let contents = serde_json::to_vec_pretty(workflow).map_err(repository_error)?;
    fs::write(&path, &contents).map_err(repository_error)?;
    Ok(LoadedWorkflow {
        path,
        definition: workflow.clone(),
        content_hash: format!("sha256:{:x}", Sha256::digest(&contents)),
    })
}

#[async_trait]
impl WorkflowRepository for FileWorkflowRepository {
    async fn list(&self) -> Result<Vec<WorkflowDefinition>, EngineError> {
        Ok(self
            .discover()?
            .into_iter()
            .map(|workflow| workflow.definition)
            .collect())
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

fn collect_json_files(root: &Path, paths: &mut Vec<PathBuf>) -> Result<(), EngineError> {
    for entry in fs::read_dir(root).map_err(repository_error)? {
        let path = entry.map_err(repository_error)?.path();
        if path.is_dir() {
            collect_json_files(&path, paths)?;
        } else if path.extension().and_then(|value| value.to_str()) == Some("json") {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ptc_domain::{StepDefinition, WORKFLOW_SCHEMA_VERSION};

    fn workflow() -> WorkflowDefinition {
        WorkflowDefinition {
            schema_version: WORKFLOW_SCHEMA_VERSION.to_owned(),
            id: "created-workflow".to_owned(),
            version: "1.0.0".to_owned(),
            name: "Created workflow".to_owned(),
            description: None,
            inputs: Vec::new(),
            steps: vec![StepDefinition::AiPrompt {
                id: "review".to_owned(),
                name: "Review".to_owned(),
                prompt: "Review the supplied context".to_owned(),
                provider: None,
            }],
        }
    }

    #[test]
    fn creates_loads_and_hashes_workflows() {
        let root = std::env::temp_dir().join(format!("ptc-persistence-{}", Uuid::new_v4()));
        let created = save_workflow_file(&root, &workflow()).unwrap();
        let loaded = load_workflow_file(&created.path).unwrap();
        assert_eq!(loaded.definition.id, "created-workflow");
        assert!(loaded.content_hash.starts_with("sha256:"));
        assert!(save_workflow_file(&root, &workflow()).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
