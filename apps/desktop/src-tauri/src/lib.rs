use std::{collections::BTreeMap, env, path::PathBuf};

use ptc_domain::WorkflowDefinition;
use ptc_engine::{prepare_workflow as prepare_definition, PreparedWorkflow};
use ptc_persistence::{load_workflow_file, FileWorkflowRepository, LoadedWorkflow};
use serde::Serialize;
use serde_json::Value;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AppHealth {
    name: &'static str,
    version: &'static str,
    status: &'static str,
}

#[tauri::command]
fn health() -> AppHealth {
    AppHealth {
        name: "PTConductor",
        version: env!("CARGO_PKG_VERSION"),
        status: "engine ready",
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkflowSummary {
    id: String,
    version: String,
    name: String,
    description: Option<String>,
    path: String,
    input_count: usize,
    step_count: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkflowDocument {
    path: String,
    content_hash: String,
    definition: WorkflowDefinition,
}

impl From<LoadedWorkflow> for WorkflowDocument {
    fn from(loaded: LoadedWorkflow) -> Self {
        Self {
            path: loaded.path.to_string_lossy().into_owned(),
            content_hash: loaded.content_hash,
            definition: loaded.definition,
        }
    }
}

#[tauri::command]
fn list_workflows() -> Result<Vec<WorkflowSummary>, String> {
    let repository = FileWorkflowRepository::new(workflow_root());
    let workflows = repository
        .discover()
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|loaded| {
            let definition = loaded.definition;
            WorkflowSummary {
                id: definition.id,
                version: definition.version,
                name: definition.name,
                description: definition.description,
                path: loaded.path.to_string_lossy().into_owned(),
                input_count: definition.inputs.len(),
                step_count: definition.steps.len(),
            }
        })
        .collect();
    Ok(workflows)
}

#[tauri::command]
fn load_workflow(path: String) -> Result<WorkflowDocument, String> {
    let path = approved_workflow_path(path)?;
    load_workflow_file(path)
        .map(Into::into)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn prepare_workflow(
    path: String,
    values: BTreeMap<String, Value>,
) -> Result<PreparedWorkflow, String> {
    let path = approved_workflow_path(path)?;
    let loaded = load_workflow_file(path).map_err(|error| error.to_string())?;
    prepare_definition(&loaded.definition, values).map_err(|error| error.to_string())
}

#[tauri::command]
fn create_workflow(workflow: WorkflowDefinition) -> Result<WorkflowDocument, String> {
    FileWorkflowRepository::new(workflow_root())
        .create(&workflow)
        .map(Into::into)
        .map_err(|error| error.to_string())
}

fn workflow_root() -> PathBuf {
    env::var_os("PTCONDUCTOR_WORKFLOWS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../workflows/examples")
        })
}

fn approved_workflow_path(path: String) -> Result<PathBuf, String> {
    let root = workflow_root()
        .canonicalize()
        .map_err(|error| format!("cannot access workflow directory: {error}"))?;
    let candidate = PathBuf::from(path)
        .canonicalize()
        .map_err(|error| format!("cannot access workflow: {error}"))?;
    if !candidate.starts_with(&root) {
        return Err("workflow path is outside the configured workflow directory".to_owned());
    }
    Ok(candidate)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            health,
            list_workflows,
            load_workflow,
            prepare_workflow,
            create_workflow
        ])
        .run(tauri::generate_context!())
        .expect("failed to run PTConductor");
}
