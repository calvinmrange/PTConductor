use std::{
    fs,
    path::{Path, PathBuf},
};

use async_trait::async_trait;
use ptc_domain::{RunRecord, RunStatus, WorkflowDefinition};
use ptc_engine::{validate_workflow, EngineError, RunRepository, WorkflowRepository};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool},
    Row,
};
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

/// The JSON artifact is authoritative; SQLite keeps searchable, redacted metadata.
#[derive(Debug, Clone)]
pub struct IndexedRunRepository {
    artifacts: JsonRunRepository,
    pool: SqlitePool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunSummary {
    pub id: Uuid,
    pub workflow_id: String,
    pub workflow_version: String,
    pub status: RunStatus,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub inputs: std::collections::BTreeMap<String, serde_json::Value>,
}

impl From<&RunRecord> for RunSummary {
    fn from(run: &RunRecord) -> Self {
        let step = run.steps.last();
        Self {
            id: run.id,
            workflow_id: run.workflow.id.clone(),
            workflow_version: run.workflow.version.clone(),
            status: run.status,
            started_at: run.started_at,
            completed_at: run.completed_at,
            provider: step.map(|step| step.provider.clone()),
            model: step.and_then(|step| step.model.clone()),
            inputs: run.inputs.clone(),
        }
    }
}

impl IndexedRunRepository {
    pub async fn open(root: impl Into<PathBuf>) -> Result<Self, EngineError> {
        let root = root.into();
        fs::create_dir_all(&root).map_err(repository_error)?;
        let options = SqliteConnectOptions::new()
            .filename(root.join("index.sqlite"))
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal);
        let pool = SqlitePool::connect_with(options)
            .await
            .map_err(repository_error)?;
        sqlx::query("CREATE TABLE IF NOT EXISTS run_index (id TEXT PRIMARY KEY, workflow_id TEXT NOT NULL, status TEXT NOT NULL, provider TEXT, started_at TEXT NOT NULL, summary TEXT NOT NULL)")
            .execute(&pool).await.map_err(repository_error)?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_run_started ON run_index(started_at DESC)")
            .execute(&pool)
            .await
            .map_err(repository_error)?;
        let repository = Self {
            artifacts: JsonRunRepository::new(&root),
            pool,
        };
        // Import runs created before the index existed, including old Codex runs.
        for entry in fs::read_dir(&root).map_err(repository_error)? {
            let path = entry.map_err(repository_error)?.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            // CLI --output-json exports can share this directory but are not run artifacts.
            if path
                .file_stem()
                .and_then(|value| value.to_str())
                .and_then(|value| Uuid::parse_str(value).ok())
                .is_none()
            {
                continue;
            }
            let contents = fs::read(&path).map_err(repository_error)?;
            let run: RunRecord = serde_json::from_slice(&contents)
                .map_err(|error| EngineError::Repository(format!("{}: {error}", path.display())))?;
            repository.index(&run).await?;
        }
        Ok(repository)
    }

    async fn index(&self, run: &RunRecord) -> Result<(), EngineError> {
        let summary = RunSummary::from(run);
        let serialized = serde_json::to_string(&summary).map_err(repository_error)?;
        let status = serde_json::to_value(run.status).map_err(repository_error)?;
        sqlx::query("INSERT INTO run_index(id, workflow_id, status, provider, started_at, summary) VALUES (?, ?, ?, ?, ?, ?) ON CONFLICT(id) DO UPDATE SET workflow_id=excluded.workflow_id, status=excluded.status, provider=excluded.provider, started_at=excluded.started_at, summary=excluded.summary")
            .bind(run.id.to_string())
            .bind(&run.workflow.id)
            .bind(status.as_str().unwrap_or_default())
            .bind(&summary.provider)
            .bind(run.started_at.to_rfc3339_opts(chrono::SecondsFormat::Nanos, true))
            .bind(serialized)
            .execute(&self.pool).await.map_err(repository_error)?;
        Ok(())
    }

    pub async fn list(&self, limit: u32) -> Result<Vec<RunSummary>, EngineError> {
        let rows =
            sqlx::query("SELECT summary FROM run_index ORDER BY started_at DESC, id DESC LIMIT ?")
                .bind(i64::from(limit.min(500)))
                .fetch_all(&self.pool)
                .await
                .map_err(repository_error)?;
        rows.into_iter()
            .map(|row| {
                let serialized: String = row.try_get("summary").map_err(repository_error)?;
                serde_json::from_str(&serialized).map_err(repository_error)
            })
            .collect()
    }
}

#[async_trait]
impl RunRepository for IndexedRunRepository {
    async fn save(&self, run: &RunRecord) -> Result<(), EngineError> {
        self.artifacts.save(run).await?;
        self.index(run).await
    }

    async fn get(&self, id: Uuid) -> Result<Option<RunRecord>, EngineError> {
        self.artifacts.get(id).await
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
    use ptc_domain::{
        OutputFormat, StepDefinition, StepRun, WorkflowReference, RUN_SCHEMA_VERSION,
        WORKFLOW_SCHEMA_VERSION,
    };

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
                output_format: OutputFormat::Text,
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

    #[tokio::test]
    async fn indexes_existing_and_updated_codex_runs_without_unredacting_inputs() {
        let root = std::env::temp_dir().join(format!("ptc-run-index-{}", Uuid::new_v4()));
        let now = chrono::Utc::now();
        let mut run = RunRecord {
            schema_version: RUN_SCHEMA_VERSION.to_owned(),
            id: Uuid::new_v4(),
            workflow: WorkflowReference {
                id: "tech-analysis".to_owned(),
                version: "1.0.0".to_owned(),
                content_hash: "sha256:test".to_owned(),
            },
            status: RunStatus::Running,
            started_at: now,
            completed_at: None,
            inputs: [("TOKEN".to_owned(), serde_json::json!("[REDACTED]"))].into(),
            steps: Vec::new(),
            findings: Vec::new(),
        };
        JsonRunRepository::new(&root).save(&run).await.unwrap();
        fs::write(
            root.join("technology-result.json"),
            b"{\"technologies\":[]}",
        )
        .unwrap();
        let repository = IndexedRunRepository::open(&root).await.unwrap();
        assert_eq!(
            repository.list(10).await.unwrap()[0].status,
            RunStatus::Running
        );
        run.status = RunStatus::Completed;
        run.completed_at = Some(now);
        run.steps.push(StepRun {
            id: "analyze".to_owned(),
            status: RunStatus::Completed,
            started_at: now,
            completed_at: Some(now),
            provider: "codex".to_owned(),
            model: Some("gpt-6-sol".to_owned()),
            output: "{\"technologies\":[]}".to_owned(),
            error: None,
        });
        repository.save(&run).await.unwrap();
        let summary = repository.list(10).await.unwrap().remove(0);
        assert_eq!(summary.provider.as_deref(), Some("codex"));
        assert_eq!(summary.status, RunStatus::Completed);
        assert_eq!(summary.inputs["TOKEN"], "[REDACTED]");
        assert_eq!(
            repository.get(run.id).await.unwrap().unwrap().steps[0].output,
            "{\"technologies\":[]}"
        );
        drop(repository);
        assert_eq!(
            IndexedRunRepository::open(&root)
                .await
                .unwrap()
                .list(10)
                .await
                .unwrap()
                .len(),
            1
        );
        fs::remove_dir_all(root).unwrap();
    }
}
