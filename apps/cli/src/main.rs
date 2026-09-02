use std::{collections::BTreeMap, fs, path::PathBuf};

use clap::{Parser, Subcommand};
use ptc_domain::WorkflowDefinition;
use ptc_engine::{validate_workflow, RunRepository, WorkflowEngine, WorkflowRepository};
use ptc_persistence::{FileWorkflowRepository, JsonRunRepository};
use ptc_providers::MockProvider;
use ptc_reporting::{MarkdownRenderer, ReportRenderer};
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

#[derive(Debug, Parser)]
#[command(
    name = "ptconductor",
    version,
    about = "Local-first pentest workflow orchestration"
)]
struct Cli {
    #[arg(long, default_value = "workflows/examples", global = true)]
    workflows_dir: PathBuf,
    #[arg(long, default_value = "runs", global = true)]
    runs_dir: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Workflow {
        #[command(subcommand)]
        command: WorkflowCommand,
    },
    Run {
        workflow: PathBuf,
        #[arg(long = "input", value_name = "NAME=VALUE")]
        inputs: Vec<String>,
        #[arg(long, default_value = "mock")]
        provider: String,
    },
    Show {
        run_id: Uuid,
    },
    Report {
        run_id: Uuid,
        #[arg(long, default_value = "markdown")]
        format: String,
    },
}

#[derive(Debug, Subcommand)]
enum WorkflowCommand {
    List,
    Validate { path: PathBuf },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    match cli.command {
        Command::Workflow { command } => match command {
            WorkflowCommand::List => {
                let repository = FileWorkflowRepository::new(cli.workflows_dir);
                for workflow in repository.list().await? {
                    println!("{}\t{}\t{}", workflow.id, workflow.version, workflow.name);
                }
            }
            WorkflowCommand::Validate { path } => {
                let workflow = read_workflow(&path)?;
                validate_workflow(&workflow)?;
                println!("valid: {} v{}", workflow.id, workflow.version);
            }
        },
        Command::Run {
            workflow,
            inputs,
            provider,
        } => {
            if provider != "mock" {
                return Err(format!("provider `{provider}` is not implemented yet").into());
            }
            let raw = fs::read(&workflow)?;
            let definition: WorkflowDefinition = serde_json::from_slice(&raw)?;
            let values = parse_inputs(inputs)?;
            let hash = format!("sha256:{:x}", Sha256::digest(&raw));
            let engine = WorkflowEngine::new(MockProvider, JsonRunRepository::new(cli.runs_dir));
            let run = engine.execute(&definition, values, hash).await?;
            println!("{}", serde_json::to_string_pretty(&run)?);
        }
        Command::Show { run_id } => {
            let repository = JsonRunRepository::new(cli.runs_dir);
            let run = repository
                .get(run_id)
                .await?
                .ok_or_else(|| format!("run not found: {run_id}"))?;
            println!("{}", serde_json::to_string_pretty(&run)?);
        }
        Command::Report { run_id, format } => {
            if format != "markdown" {
                return Err(format!("report format `{format}` is not implemented yet").into());
            }
            let repository = JsonRunRepository::new(cli.runs_dir);
            let run = repository
                .get(run_id)
                .await?
                .ok_or_else(|| format!("run not found: {run_id}"))?;
            println!("{}", MarkdownRenderer.render(&run));
        }
    }
    Ok(())
}

fn read_workflow(path: &PathBuf) -> Result<WorkflowDefinition, Box<dyn std::error::Error>> {
    Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
}

fn parse_inputs(values: Vec<String>) -> Result<BTreeMap<String, Value>, String> {
    values
        .into_iter()
        .map(|entry| {
            let (name, value) = entry
                .split_once('=')
                .ok_or_else(|| format!("invalid input `{entry}`; expected NAME=VALUE"))?;
            let value =
                serde_json::from_str(value).unwrap_or_else(|_| Value::String(value.to_owned()));
            Ok((name.to_owned(), value))
        })
        .collect()
}
