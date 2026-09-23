use std::{collections::BTreeMap, fs::OpenOptions, io::Write, path::PathBuf};

use clap::{Parser, Subcommand, ValueEnum};
use ptc_domain::{
    InputDefinition, InputKind, OutputFormat, StepDefinition, WorkflowDefinition,
    WORKFLOW_SCHEMA_VERSION,
};
use ptc_engine::{prepare_workflow, RunRepository, WorkflowEngine};
use ptc_persistence::{load_workflow_file, FileWorkflowRepository, JsonRunRepository};
use ptc_providers::ConfiguredProvider;
use ptc_reporting::{MarkdownRenderer, ReportRenderer};
use serde_json::Value;
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
        #[arg(long)]
        model: Option<String>,
        #[arg(long)]
        base_url: Option<String>,
        #[arg(
            long,
            help = "Acknowledge transmission of workflow inputs to a remote provider"
        )]
        allow_remote: bool,
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
    Report {
        run_id: Uuid,
        #[arg(long, default_value = "markdown")]
        format: String,
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
            output_json,
        } => {
            if provider == "openai" && !allow_remote {
                return Err("OpenAI-compatible provider transmits workflow inputs externally; pass --allow-remote to confirm".into());
            }
            let loaded = load_workflow_file(workflow)?;
            let values = parse_inputs(inputs)?;
            let model = model.unwrap_or_else(|| match provider.as_str() {
                "openai" => "gpt-4o-mini".to_owned(),
                "ollama" => "llama3.2".to_owned(),
                _ => "mock".to_owned(),
            });
            let provider = ConfiguredProvider::new(&provider, &model, base_url.as_deref())?;
            let engine = WorkflowEngine::new(provider, JsonRunRepository::new(cli.runs_dir));
            let run = engine
                .execute(&loaded.definition, values, loaded.content_hash)
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
            let prepared = prepare_workflow(&loaded.definition, parse_inputs(inputs)?)?;
            println!("{}", serde_json::to_string_pretty(&prepared)?);
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
