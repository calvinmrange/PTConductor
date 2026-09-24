import { FormEvent, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type InputKind = "text" | "textarea" | "url" | "number" | "boolean" | "file" | "secret";
type JsonValue = string | number | boolean | null;

type InputDefinition = {
  name: string;
  label: string;
  type: InputKind;
  required: boolean;
  description?: string;
  default?: JsonValue;
};

type WorkflowDefinition = {
  schemaVersion: string;
  id: string;
  version: string;
  name: string;
  description?: string;
  inputs: InputDefinition[];
  steps: Array<{
    type: "ai_prompt";
    id: string;
    name: string;
    prompt: string;
    provider?: string;
    outputFormat?: "text" | "json";
  }>;
};

type WorkflowSummary = {
  id: string;
  version: string;
  name: string;
  description?: string;
  path: string;
  inputCount: number;
  stepCount: number;
};

type WorkflowDocument = {
  path: string;
  contentHash: string;
  definition: WorkflowDefinition;
};

type PreparedWorkflow = {
  workflowId: string;
  workflowVersion: string;
  inputs: Record<string, JsonValue>;
  steps: Array<{
    id: string;
    name: string;
    provider?: string;
    promptPreview: string;
  }>;
};

type DraftField = Pick<InputDefinition, "name" | "label" | "type" | "required">;

type RunRecord = {
  id: string;
  status: string;
  steps: Array<{ id: string; status: string; provider: string; model?: string; output: string; error?: string }>;
};

const schemaVersion = "ptconductor.dev/workflow/v1alpha1";
const demoWorkflow: WorkflowDocument = {
  path: "browser-preview/web-endpoint-review.json",
  contentHash: "sha256:browser-preview",
  definition: {
    schemaVersion,
    id: "web-endpoint-review",
    version: "1.0.0",
    name: "Web endpoint review",
    description: "Review an authorized web endpoint with optional operator context.",
    inputs: [
      { name: "TARGET", label: "Target URL", type: "url", required: true, description: "Authorized HTTP or HTTPS endpoint." },
      { name: "NOTES", label: "Operator notes", type: "textarea", required: false, description: "Scope, constraints, or observations." },
    ],
    steps: [{ type: "ai_prompt", id: "review", name: "Review endpoint", prompt: "Review <TARGET>. Context: <NOTES>" }],
  },
};

const isTauri = () => "__TAURI_INTERNALS__" in window;

function summaryOf(document: WorkflowDocument): WorkflowSummary {
  const workflow = document.definition;
  return {
    id: workflow.id,
    version: workflow.version,
    name: workflow.name,
    description: workflow.description,
    path: document.path,
    inputCount: workflow.inputs.length,
    stepCount: workflow.steps.length,
  };
}

export function App() {
  const [workflows, setWorkflows] = useState<WorkflowSummary[]>([]);
  const [selected, setSelected] = useState<WorkflowDocument | null>(null);
  const [values, setValues] = useState<Record<string, JsonValue>>({});
  const [prepared, setPrepared] = useState<PreparedWorkflow | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(true);
  const [creating, setCreating] = useState(false);
  const [provider, setProvider] = useState<"openai" | "codex" | "ollama" | "mock">("codex");
  const [model, setModel] = useState("gpt-6-sol");
  const [allowRemote, setAllowRemote] = useState(false);
  const [run, setRun] = useState<RunRecord | null>(null);

  async function refresh(preferredPath?: string) {
    const list = isTauri() ? await invoke<WorkflowSummary[]>("list_workflows") : [summaryOf(demoWorkflow)];
    setWorkflows(list);
    const next = list.find((item) => item.path === preferredPath) ?? list[0];
    if (next) await selectWorkflow(next);
  }

  async function selectWorkflow(summary: WorkflowSummary) {
    setBusy(true);
    setError("");
    setPrepared(null);
    setRun(null);
    setAllowRemote(false);
    try {
      const document = isTauri()
        ? await invoke<WorkflowDocument>("load_workflow", { path: summary.path })
        : demoWorkflow;
      setSelected(document);
      setValues(
        Object.fromEntries(
          document.definition.inputs
            .filter((input) => input.default !== undefined || input.type === "boolean")
            .map((input) => [input.name, input.default ?? false]),
        ),
      );
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(false);
    }
  }

  useEffect(() => {
    refresh().catch((cause) => {
      setError(String(cause));
      setBusy(false);
    });
  }, []);

  async function prepare(event: FormEvent) {
    event.preventDefault();
    if (!selected) return;
    setBusy(true);
    setError("");
    const supplied = suppliedValues();
    try {
      const result = isTauri()
        ? await invoke<PreparedWorkflow>("prepare_workflow", { path: selected.path, values: supplied })
        : prepareInBrowser(selected.definition, supplied);
      setPrepared(result);
    } catch (cause) {
      setPrepared(null);
      setError(String(cause));
    } finally {
      setBusy(false);
    }
  }

  function suppliedValues(): Record<string, JsonValue> {
    if (!selected) return {};
    return Object.fromEntries(
      selected.definition.inputs
        .filter((input) => input.required || (values[input.name] !== "" && values[input.name] !== undefined))
        .map((input) => [input.name, values[input.name] ?? ""]),
    );
  }

  async function execute() {
    if (!selected || !isTauri()) return;
    setBusy(true);
    setRun(null);
    setError("");
    try {
      const result = await invoke<RunRecord>("run_workflow", {
        path: selected.path,
        values: suppliedValues(),
        provider,
        model: provider === "mock" ? "mock" : model,
        allowRemote,
      });
      setRun(result);
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(false);
    }
  }

  return (
    <main className="app-shell">
      <header className="topbar">
        <div className="brand-mark">PT</div>
        <div>
          <strong>PTConductor</strong>
          <span>Workflow studio</span>
        </div>
        <div className="engine-status"><i />{isTauri() ? "Local engine" : "Browser preview"}</div>
      </header>

      <aside className="library">
        <div className="section-heading">
          <div><span>LIBRARY</span><h1>Workflows</h1></div>
          <button className="icon-button" onClick={() => setCreating(true)} aria-label="Create workflow">+</button>
        </div>
        <p className="library-copy">Validated, versioned procedures stored on this device.</p>
        <div className="workflow-list">
          {workflows.map((workflow) => (
            <button
              key={workflow.path}
              className={selected?.path === workflow.path ? "workflow-card active" : "workflow-card"}
              onClick={() => selectWorkflow(workflow)}
            >
              <span className="card-kicker">v{workflow.version} · {workflow.inputCount} inputs</span>
              <strong>{workflow.name}</strong>
              <small>{workflow.description || workflow.id}</small>
            </button>
          ))}
          {!busy && workflows.length === 0 && <p className="empty">No workflows yet. Create the first one.</p>}
        </div>
      </aside>

      <section className="workspace">
        {creating ? (
          <WorkflowCreator
            onCancel={() => setCreating(false)}
            onCreated={async (document) => {
              setCreating(false);
              if (isTauri()) await refresh(document.path);
              else {
                setWorkflows([summaryOf(document)]);
                setSelected(document);
                setValues({});
              }
            }}
          />
        ) : selected ? (
          <>
            <div className="workflow-header">
              <div>
                <span className="eyebrow">{selected.definition.id} / {selected.definition.version}</span>
                <h2>{selected.definition.name}</h2>
                <p>{selected.definition.description}</p>
              </div>
              <span className="validated-badge">✓ Schema valid</span>
            </div>
            <form onSubmit={prepare} className="input-form">
              <div className="form-title"><span>01</span><div><h3>Workflow inputs</h3><p>Fields are generated from the workflow definition.</p></div></div>
              <div className="field-grid">
                {selected.definition.inputs.map((input) => (
                  <WorkflowField
                    key={input.name}
                    definition={input}
                    value={values[input.name]}
                    onChange={(value) => {
                      setValues((current) => ({ ...current, [input.name]: value }));
                      setPrepared(null);
                      setRun(null);
                      setAllowRemote(false);
                    }}
                  />
                ))}
              </div>
              {error && <div className="error" role="alert">{error}</div>}
              <div className="form-actions">
                <span>Secrets are redacted from previews and stored run metadata.</span>
                <button className="primary" disabled={busy}>{busy ? "Validating…" : "Validate & prepare"}<b>→</b></button>
              </div>
            </form>
            <PreparedPanel prepared={prepared} />
            {prepared && <section className="preview-panel">
              <div className="form-title"><span>03</span><div><h3>Run with a provider</h3><p>Prompts are transmitted only when you click Run.</p></div></div>
              <div className="field-grid">
                <label className="field"><span className="field-label">Provider</span>
                  <select value={provider} onChange={(event) => {
                    const next = event.target.value as typeof provider;
                    setProvider(next);
                    setModel(next === "openai" ? "gpt-6-luna" : next === "codex" ? "gpt-6-sol" : next === "ollama" ? "llama3.2" : "mock");
                    setAllowRemote(false);
                    setRun(null);
                  }}><option value="codex">Codex (ChatGPT sign-in)</option><option value="openai">OpenAI-compatible (API key)</option><option value="ollama">Ollama (local)</option><option value="mock">Mock (offline)</option></select>
                </label>
                <label className="field"><span className="field-label">Model</span><input value={model} onChange={(event) => setModel(event.target.value)} disabled={provider === "mock"} /></label>
              </div>
              {provider === "openai" && <p className="remote-notice">The workflow prompt and inputs will be sent to the configured OpenAI-compatible endpoint. Set <code>OPENAI_API_KEY</code> in the terminal before launching the app. Do not paste it into a workflow field.</p>}
              {provider === "codex" && <p className="remote-notice">Codex uses your local CLI sign-in and sends this workflow to your ChatGPT account. Run <code>codex login</code> with ChatGPT first. It starts in an empty temporary directory with a read-only sandbox; an agent may still read accessible files. Remove sensitive content from evidence.</p>}
              {(provider === "openai" || provider === "codex") && <label className="consent"><input type="checkbox" checked={allowRemote} onChange={(event) => setAllowRemote(event.target.checked)} /> I authorize sending these workflow inputs to the selected remote provider.</label>}
              {error && <div className="error" role="alert">{error}</div>}
              <div className="form-actions"><span>Responses are saved locally as run JSON artifacts.</span><button type="button" className="primary" onClick={execute} disabled={busy || !isTauri() || ((provider === "openai" || provider === "codex") && !allowRemote)}>{busy ? "Running…" : "Run workflow"}<b>→</b></button></div>
              {!isTauri() && <p className="remote-notice">Open the Tauri desktop app to run workflows. Browser preview does not call providers.</p>}
            </section>}
            {run && <section className="preview-panel">
              <div className="form-title"><span>04</span><div><h3>Results</h3><p>Run {run.id} · {run.status}</p></div></div>
              {run.steps.map((step) => <div key={step.id}><h4>{step.id} · {step.provider}{step.model ? ` / ${step.model}` : ""}</h4><pre>{step.output || step.error}</pre></div>)}
            </section>}
          </>
        ) : (
          <div className="center-state">{busy ? "Loading workflows…" : "Create a workflow to begin."}</div>
        )}
      </section>
    </main>
  );
}

function WorkflowField({ definition, value, onChange }: { definition: InputDefinition; value: JsonValue | undefined; onChange: (value: JsonValue) => void }) {
  const id = `input-${definition.name}`;
  if (definition.type === "boolean") {
    return (
      <label className="field boolean-field" htmlFor={id}>
        <input id={id} type="checkbox" checked={Boolean(value)} onChange={(event) => onChange(event.target.checked)} />
        <span><strong>{definition.label}</strong><small>{definition.description}</small></span>
      </label>
    );
  }
  const common = {
    id,
    required: definition.required,
    value: value === undefined || value === null ? "" : String(value),
    placeholder: placeholderFor(definition.type),
    onChange: (event: React.ChangeEvent<HTMLInputElement | HTMLTextAreaElement>) => {
      const raw = event.target.value;
      onChange(definition.type === "number" && raw !== "" ? Number(raw) : raw);
    },
  };
  return (
    <label className={definition.type === "textarea" ? "field full" : "field"} htmlFor={id}>
      <span className="field-label">{definition.label}{definition.required && <em>required</em>}</span>
      {definition.type === "textarea" ? <textarea {...common} rows={5} /> : <input {...common} type={definition.type === "secret" ? "password" : definition.type === "number" ? "number" : "text"} />}
      <small>{definition.description || `${definition.type} · ${definition.name}`}</small>
    </label>
  );
}

function PreparedPanel({ prepared }: { prepared: PreparedWorkflow | null }) {
  if (!prepared) return (
    <section className="preview-panel muted-panel">
      <span>02</span><div><h3>Prepared run</h3><p>Validated inputs and rendered prompt previews will appear here before execution.</p></div>
    </section>
  );
  return (
    <section className="preview-panel">
      <div className="form-title"><span>02</span><div><h3>Prepared run</h3><p>{prepared.workflowId} is ready for provider execution.</p></div></div>
      <div className="preview-columns">
        <div><h4>Resolved inputs</h4><pre>{JSON.stringify(prepared.inputs, null, 2)}</pre></div>
        <div><h4>Prompt preview</h4>{prepared.steps.map((step) => <pre key={step.id}>{step.promptPreview}</pre>)}</div>
      </div>
    </section>
  );
}

function WorkflowCreator({ onCancel, onCreated }: { onCancel: () => void; onCreated: (document: WorkflowDocument) => Promise<void> }) {
  const [id, setId] = useState("");
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [prompt, setPrompt] = useState("Analyze the authorized target using these inputs: <TARGET>");
  const [outputFormat, setOutputFormat] = useState<"text" | "json">("text");
  const [fields, setFields] = useState<DraftField[]>([{ name: "TARGET", label: "Target", type: "text", required: true }]);
  const [error, setError] = useState("");
  const canSubmit = useMemo(() => Boolean(id && name && prompt && fields.every((field) => field.name && field.label)), [id, name, prompt, fields]);

  function updateField(index: number, patch: Partial<DraftField>) {
    setFields((current) => current.map((field, fieldIndex) => fieldIndex === index ? { ...field, ...patch } : field));
  }

  async function submit(event: FormEvent) {
    event.preventDefault();
    setError("");
    const definition: WorkflowDefinition = {
      schemaVersion,
      id,
      version: "1.0.0",
      name,
      description: description || undefined,
      inputs: fields,
      steps: [{ type: "ai_prompt", id: "analyze", name: "Analyze", prompt, outputFormat }],
    };
    try {
      const document = isTauri()
        ? await invoke<WorkflowDocument>("create_workflow", { workflow: definition })
        : { path: `browser-preview/${id}.json`, contentHash: "sha256:browser-preview", definition };
      await onCreated(document);
    } catch (cause) {
      setError(String(cause));
    }
  }

  return (
    <form className="creator" onSubmit={submit}>
      <div className="workflow-header"><div><span className="eyebrow">NEW WORKFLOW</span><h2>Create a procedure</h2><p>Define typed fields once; PTConductor generates the runner interface.</p></div></div>
      <div className="field-grid">
        <label className="field"><span className="field-label">Workflow ID <em>required</em></span><input value={id} onChange={(event) => setId(event.target.value)} placeholder="endpoint-review" required /><small>Lowercase letters, numbers, and hyphens.</small></label>
        <label className="field"><span className="field-label">Name <em>required</em></span><input value={name} onChange={(event) => setName(event.target.value)} placeholder="Endpoint review" required /></label>
        <label className="field full"><span className="field-label">Description</span><input value={description} onChange={(event) => setDescription(event.target.value)} placeholder="What this workflow accomplishes" /></label>
      </div>
      <div className="builder-heading"><h3>Input definitions</h3><button type="button" className="secondary" onClick={() => setFields((current) => [...current, { name: "", label: "", type: "text", required: false }])}>+ Add input</button></div>
      <div className="field-builder">
        {fields.map((field, index) => (
          <div className="field-row" key={index}>
            <input value={field.name} onChange={(event) => updateField(index, { name: event.target.value.toUpperCase() })} placeholder="VARIABLE_NAME" aria-label="Variable name" required />
            <input value={field.label} onChange={(event) => updateField(index, { label: event.target.value })} placeholder="Field label" aria-label="Field label" required />
            <select value={field.type} onChange={(event) => updateField(index, { type: event.target.value as InputKind })} aria-label="Field type">
              {(["text", "textarea", "url", "number", "boolean", "file", "secret"] as InputKind[]).map((type) => <option key={type}>{type}</option>)}
            </select>
            <label className="required-toggle"><input type="checkbox" checked={field.required} onChange={(event) => updateField(index, { required: event.target.checked })} /> Required</label>
            <button type="button" className="remove" onClick={() => setFields((current) => current.filter((_, fieldIndex) => fieldIndex !== index))}>×</button>
          </div>
        ))}
      </div>
      <label className="field full prompt-field"><span className="field-label">Prompt template <em>required</em></span><textarea rows={6} value={prompt} onChange={(event) => setPrompt(event.target.value)} required /><small>Reference declared inputs with angle brackets, such as &lt;TARGET&gt;.</small></label>
      <label className="field"><span className="field-label">Response format</span><select value={outputFormat} onChange={(event) => setOutputFormat(event.target.value as "text" | "json")}><option value="text">Text</option><option value="json">JSON object</option></select></label>
      {error && <div className="error">{error}</div>}
      <div className="form-actions"><button type="button" className="secondary" onClick={onCancel}>Cancel</button><button className="primary" disabled={!canSubmit}>Create workflow <b>→</b></button></div>
    </form>
  );
}

function placeholderFor(type: InputKind) {
  switch (type) {
    case "url": return "https://authorized.example";
    case "number": return "0";
    case "file": return "/path/to/evidence.txt";
    case "secret": return "Stored only for this run";
    default: return "Enter a value";
  }
}

function prepareInBrowser(workflow: WorkflowDefinition, supplied: Record<string, JsonValue>): PreparedWorkflow {
  for (const input of workflow.inputs) {
    const value = supplied[input.name] ?? input.default;
    if (input.required && (value === undefined || value === "")) throw new Error(`invalid input \`${input.name}\`: required value is missing`);
    if (input.type === "url" && value && !/^https?:\/\/[^\s/]+/.test(String(value))) throw new Error(`invalid input \`${input.name}\`: expected a valid HTTP or HTTPS URL`);
    if (input.type === "number" && typeof value !== "number") throw new Error(`invalid input \`${input.name}\`: expected a number`);
  }
  const inputs = Object.fromEntries(workflow.inputs.map((input) => [input.name, input.type === "secret" && supplied[input.name] ? "***REDACTED***" : supplied[input.name] ?? input.default ?? null]));
  return {
    workflowId: workflow.id,
    workflowVersion: workflow.version,
    inputs,
    steps: workflow.steps.map((step) => ({
      id: step.id,
      name: step.name,
      provider: step.provider,
      promptPreview: workflow.inputs.reduce((prompt, input) => prompt.replaceAll(`<${input.name}>`, String(inputs[input.name] ?? "")), step.prompt),
    })),
  };
}
