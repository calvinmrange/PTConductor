use std::{collections::BTreeMap, fs::OpenOptions, io::Write, path::PathBuf};

use clap::{Parser, Subcommand, ValueEnum};
use ptc_domain::{
    InputDefinition, InputKind, OutputFormat, StepDefinition, WorkflowDefinition,
    WORKFLOW_SCHEMA_VERSION,
};
use ptc_engine::{prepare_workflow, RunOptions, RunRepository, WorkflowEngine};
use ptc_persistence::{load_workflow_file, FileWorkflowRepository, IndexedRunRepository};
use ptc_providers::{CodexProvider, ConfiguredProvider};
use ptc_reporting::render_report;
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Parser)]
#[command(
    name = "ptconductor",
    version,
    about = "Local-first pentest workflow orchestration"
)]
struct Cli {
    #[arg(long, default_value_os_t = default_directory("PTCONDUCTOR_WORKFLOWS_DIR", "workflows/examples"), global = true)]
    workflows_dir: PathBuf,
    #[arg(long, default_value_os_t = default_directory("PTCONDUCTOR_RUNS_DIR", "runs"), global = true)]
    runs_dir: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Check Codex installation, required flags, and ChatGPT login without calling a model.
    Doctor,
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
        #[arg(long)]
        model: Option<String>,
        #[arg(long)]
        base_url: Option<String>,
        #[arg(
            long,
            help = "Acknowledge transmission of workflow inputs to a remote provider"
        )]
        allow_remote: bool,
        #[arg(
            long,
            help = "Authorize this workflow's Nmap / HTTP collection against the supplied target"
        )]
        allow_active: bool,
        #[arg(long, help = "Write the final JSON step output to a new file")]
        output_json: Option<PathBuf>,
    },
    Prepare {
        workflow: PathBuf,
        #[arg(long = "input", value_name = "NAME=VALUE")]
        inputs: Vec<String>,
    },
    Show {
        run_id: Uuid,
    },
    History {
        #[arg(long, default_value_t = 50)]
        limit: u32,
        #[arg(long)]
        json: bool,
    },
    Report {
        run_id: Uuid,
        #[arg(long, default_value = "markdown")]
        format: String,
        #[arg(long, help = "Save the report to a new file (otherwise print it)")]
        output: Option<PathBuf>,
    },
}

#[derive(Debug, Subcommand)]
enum WorkflowCommand {
    List,
    Validate {
        path: PathBuf,
    },
    Create {
        #[arg(long)]
        id: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        description: Option<String>,
        #[arg(long)]
        prompt: String,
        #[arg(long, default_value = "text")]
        output_format: CliOutputFormat,
        #[arg(long = "field", value_name = "NAME:TYPE:REQUIRED:LABEL")]
        fields: Vec<String>,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CliInputKind {
    Text,
    Textarea,
    Url,
    Number,
    Boolean,
    File,
    Secret,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CliOutputFormat {
    Text,
    Json,
}

impl From<CliOutputFormat> for OutputFormat {
    fn from(value: CliOutputFormat) -> Self {
        match value {
            CliOutputFormat::Text => Self::Text,
            CliOutputFormat::Json => Self::Json,
        }
    }
}

impl From<CliInputKind> for InputKind {
    fn from(value: CliInputKind) -> Self {
        match value {
            CliInputKind::Text => Self::Text,
            CliInputKind::Textarea => Self::Textarea,
            CliInputKind::Url => Self::Url,
            CliInputKind::Number => Self::Number,
            CliInputKind::Boolean => Self::Boolean,
            CliInputKind::File => Self::File,
            CliInputKind::Secret => Self::Secret,
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    match cli.command {
        Command::Doctor => {
            CodexProvider::new("default")?.check_ready().await?;
            println!("Codex CLI ready: required flags supported; ChatGPT authentication detected. Live inference still depends on model access and usage allowance.");
        }
        Command::Workflow { command } => match command {
            WorkflowCommand::List => {
                let repository = FileWorkflowRepository::new(cli.workflows_dir);
                for loaded in repository.discover()? {
                    println!(
                        "{}\t{}\t{}\t{}",
                        loaded.definition.id,
                        loaded.definition.version,
                        loaded.definition.name,
                        loaded.path.display()
                    );
                }
            }
            WorkflowCommand::Validate { path } => {
                let loaded = load_workflow_file(path)?;
                println!(
                    "valid: {} v{} ({})",
                    loaded.definition.id, loaded.definition.version, loaded.content_hash
                );
            }
            WorkflowCommand::Create {
                id,
                name,
                description,
                prompt,
                output_format,
                fields,
            } => {
                let inputs = fields
                    .into_iter()
                    .map(|field| parse_field(&field))
                    .collect::<Result<Vec<_>, _>>()?;
                let workflow = WorkflowDefinition {
                    schema_version: WORKFLOW_SCHEMA_VERSION.to_owned(),
                    id,
                    version: "1.0.0".to_owned(),
                    name,
                    description,
                    inputs,
                    steps: vec![StepDefinition::AiPrompt {
                        id: "analyze".to_owned(),
                        name: "Analyze".to_owned(),
                        prompt,
                        provider: None,
                        output_format: output_format.into(),
                    }],
                };
                let created = FileWorkflowRepository::new(cli.workflows_dir).create(&workflow)?;
                println!("created: {}", created.path.display());
            }
        },
        Command::Run {
            workflow,
            inputs,
            provider,
            model,
            base_url,
            allow_remote,
            allow_active,
            output_json,
        } => {
            if output_json.as_ref().is_some_and(|path| path.exists()) {
                return Err("output file already exists; choose a new --output-json path".into());
            }
            if matches!(provider.as_str(), "openai" | "codex") && !allow_remote {
                return Err(format!("provider `{provider}` sends workflow inputs to OpenAI; pass --allow-remote to confirm").into());
            }
            let loaded = load_workflow_file(workflow)?;
            let values = parse_workflow_inputs(inputs, &loaded.definition.inputs)?;
            let model = model.unwrap_or_else(|| match provider.as_str() {
                "openai" => "gpt-6-luna".to_owned(),
                "codex" => "default".to_owned(),
                "ollama" => "llama3.2".to_owned(),
                _ => "mock".to_owned(),
            });
            let provider = ConfiguredProvider::new(&provider, &model, base_url.as_deref())?;
            let repository = IndexedRunRepository::open(cli.runs_dir).await?;
            let engine = WorkflowEngine::new(provider, repository);
            let run = engine
                .execute_with_options(
                    &loaded.definition,
                    values,
                    loaded.content_hash,
                    RunOptions { allow_active },
                )
                .await?;
            if let Some(path) = output_json {
                let final_step = run.steps.last().ok_or("run had no steps")?;
                let value: Value = serde_json::from_str(&final_step.output)
                    .map_err(|_| "final step output is not JSON; choose a JSON workflow")?;
                let mut file = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&path)?;
                serde_json::to_writer_pretty(&mut file, &value)?;
                file.write_all(b"\n")?;
                eprintln!("saved JSON output: {}", path.display());
            }
            println!("{}", serde_json::to_string_pretty(&run)?);
        }
        Command::Prepare { workflow, inputs } => {
            let loaded = load_workflow_file(workflow)?;
            let prepared = prepare_workflow(
                &loaded.definition,
                parse_workflow_inputs(inputs, &loaded.definition.inputs)?,
            )?;
            println!("{}", serde_json::to_string_pretty(&prepared)?);
        }
        Command::Show { run_id } => {
            let repository = IndexedRunRepository::open(cli.runs_dir).await?;
            let run = repository
                .get(run_id)
                .await?
                .ok_or_else(|| format!("run not found: {run_id}"))?;
            println!("{}", serde_json::to_string_pretty(&run)?);
        }
        Command::History { limit, json } => {
            let repository = IndexedRunRepository::open(cli.runs_dir).await?;
            let runs = repository.list(limit).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&runs)?);
            } else {
                for run in runs {
                    println!(
                        "{}\t{}\t{}\t{}\t{}\t{}",
                        run.id,
                        run.started_at,
                        serde_json::to_value(run.status)?
                            .as_str()
                            .unwrap_or_default(),
                        run.workflow_id,
                        run.provider.as_deref().unwrap_or("—"),
                        run.model.as_deref().unwrap_or("—")
                    );
                }
            }
        }
        Command::Report {
            run_id,
            format,
            output,
        } => {
            let repository = IndexedRunRepository::open(cli.runs_dir).await?;
            let run = repository
                .get(run_id)
                .await?
                .ok_or_else(|| format!("run not found: {run_id}"))?;
            let report = render_report(&run, &format)?;
            if let Some(path) = output {
                let mut file = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&path)?;
                file.write_all(report.as_bytes())?;
                eprintln!("saved report: {}", path.display());
            } else {
                println!("{report}");
            }
        }
    }
    Ok(())
}

fn default_directory(variable: &str, fallback: &str) -> PathBuf {
    std::env::var_os(variable)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(fallback))
}

fn parse_inputs(values: Vec<String>) -> Result<BTreeMap<String, Value>, String> {
    let mut inputs = BTreeMap::new();
    for entry in values {
        let (name, value) = entry
            .split_once('=')
            .ok_or_else(|| "invalid input; expected NAME=VALUE".to_owned())?;
        if inputs.contains_key(name) {
            return Err("duplicate input name".into());
        }
        let value = serde_json::from_str(value).unwrap_or_else(|_| Value::String(value.to_owned()));
        inputs.insert(name.to_owned(), value);
    }
    Ok(inputs)
}

fn parse_workflow_inputs(
    values: Vec<String>,
    definitions: &[InputDefinition],
) -> Result<BTreeMap<String, Value>, String> {
    // Text fields must preserve literal input (including numeric-looking ports and secrets).
    let mut parsed = parse_inputs(values.clone())?;
    for entry in values {
        let (name, raw) = entry.split_once('=').expect("parse_inputs checked syntax");
        if definitions.iter().any(|definition| {
            definition.name == name
                && !matches!(definition.kind, InputKind::Number | InputKind::Boolean)
        }) {
            let value = serde_json::from_str::<String>(raw).unwrap_or_else(|_| raw.to_owned());
            parsed.insert(name.to_owned(), Value::String(value));
        }
    }
    Ok(parsed)
}

fn parse_field(value: &str) -> Result<InputDefinition, String> {
    let parts: Vec<_> = value.splitn(4, ':').collect();
    if parts.len() != 4 {
        return Err(format!(
            "invalid field `{value}`; expected NAME:TYPE:REQUIRED:LABEL"
        ));
    }
    let kind = CliInputKind::from_str(parts[1], true)
        .map_err(|_| format!("unknown input type `{}`", parts[1]))?;
    let required = match parts[2].to_ascii_lowercase().as_str() {
        "required" | "true" | "yes" => true,
        "optional" | "false" | "no" => false,
        _ => {
            return Err(format!(
                "invalid required flag `{}`; use required or optional",
                parts[2]
            ))
        }
    };
    Ok(InputDefinition {
        name: parts[0].to_owned(),
        label: parts[3].to_owned(),
        kind: kind.into(),
        required,
        description: None,
        default: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_typed_field_definitions() {
        let field = parse_field("TARGET:url:required:Target URL").unwrap();
        assert_eq!(field.name, "TARGET");
        assert_eq!(field.kind, InputKind::Url);
        assert!(field.required);
    }

    #[test]
    fn parses_json_scalar_inputs() {
        let values = parse_inputs(vec![
            "COUNT=4".to_owned(),
            "ENABLED=true".to_owned(),
            "LABEL=scan".to_owned(),
        ])
        .unwrap();
        assert_eq!(values["COUNT"], 4);
        assert_eq!(values["ENABLED"], true);
        assert_eq!(values["LABEL"], "scan");
    }
}
