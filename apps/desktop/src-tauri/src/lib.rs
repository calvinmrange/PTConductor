use ptc_providers::CodexProvider;
use ptc_reporting::render_report;
use std::{collections::BTreeMap, env, fs::OpenOptions, io::Write, path::PathBuf};
use tauri::Manager;

use ptc_domain::RunRecord;
use ptc_domain::WorkflowDefinition;
use ptc_engine::{
    prepare_workflow as prepare_definition, PreparedWorkflow, RunOptions, RunRepository,
    WorkflowEngine,
};
use ptc_persistence::{
    load_workflow_file, FileWorkflowRepository, IndexedRunRepository, LoadedWorkflow, RunSummary,
};
use ptc_providers::ConfiguredProvider;
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

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
fn list_workflows(state: tauri::State<AppPaths>) -> Result<Vec<WorkflowSummary>, String> {
    let repository = FileWorkflowRepository::new(&state.workflows);
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
fn load_workflow(path: String, state: tauri::State<AppPaths>) -> Result<WorkflowDocument, String> {
    let path = approved_workflow_path(path, &state.workflows)?;
    load_workflow_file(path)
        .map(Into::into)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn prepare_workflow(
    path: String,
    values: BTreeMap<String, Value>,
    state: tauri::State<AppPaths>,
) -> Result<PreparedWorkflow, String> {
    let path = approved_workflow_path(path, &state.workflows)?;
    let loaded = load_workflow_file(path).map_err(|error| error.to_string())?;
    prepare_definition(&loaded.definition, values).map_err(|error| error.to_string())
}

#[tauri::command]
fn create_workflow(
    workflow: WorkflowDefinition,
    state: tauri::State<AppPaths>,
) -> Result<WorkflowDocument, String> {
    FileWorkflowRepository::new(&state.workflows)
        .create(&workflow)
        .map(Into::into)
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn run_workflow(
    path: String,
    values: BTreeMap<String, Value>,
    provider: String,
    model: String,
    allow_remote: bool,
    allow_active: bool,
    state: tauri::State<'_, AppPaths>,
) -> Result<RunRecord, String> {
    if matches!(provider.as_str(), "openai" | "codex") && !allow_remote {
        return Err("Confirm remote transmission before running this workflow".to_owned());
    }
    let path = approved_workflow_path(path, &state.workflows)?;
    let loaded = load_workflow_file(path).map_err(|error| error.to_string())?;
    let base_url = match provider.as_str() {
        "openai" => env::var("PTCONDUCTOR_OPENAI_BASE_URL").ok(),
        "ollama" => env::var("PTCONDUCTOR_OLLAMA_BASE_URL").ok(),
        _ => None,
    };
    let configured = ConfiguredProvider::new(&provider, &model, base_url.as_deref())
        .map_err(|error| error.to_string())?;
    let repository = IndexedRunRepository::open(&state.runs)
        .await
        .map_err(|error| error.to_string())?;
    let engine = WorkflowEngine::new(configured, repository);
    engine
        .execute_with_options(
            &loaded.definition,
            values,
            loaded.content_hash,
            RunOptions { allow_active },
        )
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn list_runs(state: tauri::State<'_, AppPaths>) -> Result<Vec<RunSummary>, String> {
    let repository = IndexedRunRepository::open(&state.runs)
        .await
        .map_err(|error| error.to_string())?;
    repository
        .list(100)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn get_run(
    run_id: Uuid,
    state: tauri::State<'_, AppPaths>,
) -> Result<Option<RunRecord>, String> {
    let repository = IndexedRunRepository::open(&state.runs)
        .await
        .map_err(|error| error.to_string())?;
    repository
        .get(run_id)
        .await
        .map_err(|error| error.to_string())
}

struct AppPaths {
    workflows: PathBuf,
    runs: PathBuf,
    reports: PathBuf,
}

#[tauri::command]
async fn check_codex() -> Result<String, String> {
    CodexProvider::new("default")
        .map_err(|e| e.to_string())?
        .check_ready()
        .await
        .map_err(|e| e.to_string())?;
    Ok(
        "Codex CLI and ChatGPT login ready. Model access and allowance are checked during a run."
            .into(),
    )
}

#[tauri::command]
async fn export_report(
    run_id: Uuid,
    format: String,
    state: tauri::State<'_, AppPaths>,
) -> Result<String, String> {
    let repository = IndexedRunRepository::open(&state.runs)
        .await
        .map_err(|e| e.to_string())?;
    let run = repository
        .get(run_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("Run not found")?;
    let report = render_report(&run, &format)?;
    std::fs::create_dir_all(&state.reports).map_err(|e| e.to_string())?;
    let extension = if format == "html" { "html" } else { "md" };
    let path = state
        .reports
        .join(format!("{run_id}-{}.{}", Uuid::new_v4(), extension));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|e| e.to_string())?;
    file.write_all(report.as_bytes())
        .map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().into_owned())
}

fn initialize_paths(app: &tauri::App) -> Result<AppPaths, Box<dyn std::error::Error>> {
    let data = env::var_os("PTCONDUCTOR_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or(app.path().app_data_dir()?);
    let workflows = env::var_os("PTCONDUCTOR_WORKFLOWS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| data.join("workflows"));
    let runs = env::var_os("PTCONDUCTOR_RUNS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| data.join("runs"));
    let reports = data.join("reports");
    std::fs::create_dir_all(&workflows)?;
    for (name, content) in [
        (
            "technology-fingerprint.json",
            include_str!("../../../../workflows/examples/technology-fingerprint.json"),
        ),
        (
            "typed-input-demo.json",
            include_str!("../../../../workflows/examples/typed-input-demo.json"),
        ),
        (
            "web-endpoint-review.json",
            include_str!("../../../../workflows/examples/web-endpoint-review.json"),
        ),
        (
            "nmap-scan-summary.json",
            include_str!("../../../../workflows/examples/nmap-scan-summary.json"),
        ),
        (
            "security-headers-summary.json",
            include_str!("../../../../workflows/examples/security-headers-summary.json"),
        ),
    ] {
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(workflows.join(name))
        {
            Ok(mut file) => file.write_all(content.as_bytes())?,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(AppPaths {
        workflows,
        runs,
        reports,
    })
}

fn approved_workflow_path(path: String, root: &std::path::Path) -> Result<PathBuf, String> {
    let root = root
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
        .setup(|app| {
            let paths = initialize_paths(app)?;
            app.manage(paths);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            health,
            list_workflows,
            load_workflow,
            prepare_workflow,
            create_workflow,
            run_workflow,
            list_runs,
            get_run,
            check_codex,
            export_report
        ])
        .run(tauri::generate_context!())
        .expect("failed to run PTConductor");
}
