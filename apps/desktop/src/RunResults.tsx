import { useState } from "react";

type Data = null | boolean | number | string | Data[] | { [key: string]: Data };
export type ResultStep = { id: string; status: string; provider: string; model?: string; output: string; error?: string };
const isObject = (value: Data): value is Record<string, Data> => value !== null && typeof value === "object" && !Array.isArray(value);
const label = (key: string) => key.replace(/([a-z0-9])([A-Z])/g, "$1 $2").replace(/[_-]+/g, " ").replace(/^./, (letter) => letter.toUpperCase());
const priority = ["target", "url", "summary", "findings", "technologies", "observations", "evidence", "recommendation", "recommendations", "limitations"];
const severityLevels = new Set(["critical", "high", "medium", "low", "informational", "info"]);

function Badge({ value, kind }: { value: string; kind: "severity" | "status" }) {
  const normalized = value.toLowerCase();
  const tone = kind === "severity" ? (severityLevels.has(normalized) ? normalized : "neutral")
    : normalized === "completed" ? "success" : normalized === "failed" ? "high" : "neutral";
  return <span className={`result-badge tone-${tone}`}>{label(value)}</span>;
}

function RawData({ text, title = "View raw JSON" }: { text: string; title?: string }) {
  const [open, setOpen] = useState(false);
  return <details className="raw-data" onToggle={(event) => setOpen(event.currentTarget.open)}>
    <summary>{title}</summary>{open && <pre>{text}</pre>}
  </details>;
}

function DataList({ items, depth, field }: { items: Data[]; depth: number; field: string }) {
  const [visible, setVisible] = useState(50);
  if (!items.length) return <p className="result-empty">{field === "findings" ? "No findings reported." : "No entries reported."}</p>;
  return <>
    <ul className={items.some(isObject) ? "result-card-list" : "result-list"}>
      {items.slice(0, visible).map((item, index) => <li key={index}>
        {isObject(item) ? <ObjectCard value={item} depth={depth} index={index} field={field} /> : <StructuredData value={item} depth={depth} field={field} />}
      </li>)}
    </ul>
    {items.length > visible && <button type="button" className="secondary" onClick={() => setVisible((count) => count + 50)}>Show more ({items.length - visible} remaining)</button>}
  </>;
}

function ObjectCard({ value, depth, index, field }: { value: Record<string, Data>; depth: number; index: number; field: string }) {
  const titleKey = typeof value.title === "string" ? "title" : typeof value.name === "string" ? "name" : null;
  const severity = typeof value.severity === "string" ? value.severity : null;
  const rest = Object.fromEntries(Object.entries(value).filter(([key]) => key !== titleKey && !(key === "severity" && severity !== null)));
  const title = titleKey ? String(value[titleKey]) : `${field === "findings" ? "Finding" : "Item"} ${index + 1}`;
  return <article className="result-item-card">
    <div className="result-item-heading"><h5>{title}</h5>{severity !== null && <Badge value={severity} kind="severity" />}</div>
    <StructuredData value={rest} depth={depth + 1} />
  </article>;
}

/** Provider output is always React text, never HTML or executable Markdown. */
export function StructuredData({ value, depth = 0, field = "" }: { value: Data; depth?: number; field?: string }) {
  if (value === null) return <span className="result-empty">Not provided</span>;
  if (typeof value === "boolean") return <span>{value ? "Yes" : "No"}</span>;
  if (typeof value === "number") return <span>{value}</span>;
  if (typeof value === "string") return <p className="result-text">{value || <span className="result-empty">Not provided</span>}</p>;
  if (depth >= 8) return <RawData text={JSON.stringify(value, null, 2)} title="View nested details" />;
  if (Array.isArray(value)) return <DataList items={value} depth={depth + 1} field={field} />;
  const entries = Object.entries(value).sort(([a], [b]) => {
    const rank = (key: string) => priority.includes(key) ? priority.indexOf(key) : priority.length;
    return rank(a) - rank(b);
  });
  if (!entries.length) return <p className="result-empty">No details reported.</p>;
  return <dl className={`result-properties ${depth === 0 ? "result-properties-root" : ""}`}>
    {entries.map(([key, item]) => <div className={`result-property ${key === "summary" ? "result-summary" : ""}`} key={key}>
      <dt>{label(key)}{Array.isArray(item) && <span className="result-count">{item.length}</span>}</dt>
      <dd>{key === "severity" && typeof item === "string" ? <Badge value={item} kind="severity" /> : <StructuredData value={item} depth={depth + 1} field={key} />}</dd>
    </div>)}
  </dl>;
}

function NmapEvidence({ xml }: { xml: string }) {
  const document = new DOMParser().parseFromString(xml, "application/xml");
  if (document.querySelector("parsererror") || document.documentElement.tagName !== "nmaprun") {
    return <><p className="result-empty">Scan XML could not be formatted. The original evidence is available below.</p><RawData text={xml} title="View scan XML" /></>;
  }
  const rows = Array.from(document.querySelectorAll("host")).flatMap((host) => {
    const address = host.querySelector("address")?.getAttribute("addr") ?? "Unknown host";
    return Array.from(host.querySelectorAll("port")).map((port) => {
      const service = port.querySelector("service");
      return { address, port: `${port.getAttribute("portid") ?? "?"}/${port.getAttribute("protocol") ?? "tcp"}`,
        state: port.querySelector("state")?.getAttribute("state") ?? "Unknown",
        service: service?.getAttribute("name") ?? "Not identified",
        version: [service?.getAttribute("product"), service?.getAttribute("version"), service?.getAttribute("extrainfo")].filter(Boolean).join(" ") || "Not reported" };
    });
  });
  const groups = Array.from(document.querySelectorAll("host > ports > extraports")).map((ports) => `${ports.getAttribute("count") ?? "?"} ports ${ports.getAttribute("state") ?? "unknown"}`);
  return <div className="scan-evidence">
    <div className="result-table-scroll"><table className="result-table"><caption>Observed ports and services</caption>
      <thead><tr><th scope="col">Host</th><th scope="col">Port</th><th scope="col">State</th><th scope="col">Service</th><th scope="col">Version evidence</th></tr></thead>
      <tbody>{rows.slice(0, 1024).map((row, index) => <tr key={index}><td>{row.address}</td><td>{row.port}</td><td>{row.state}</td><td>{row.service}</td><td>{row.version}</td></tr>)}</tbody>
    </table></div>
    {!rows.length && <p className="result-empty">No individual port records were reported.</p>}
    {groups.length > 0 && <p className="result-text">Additional port groups: {groups.join("; ")}.</p>}
    {rows.length > 1024 && <p className="result-empty">Showing the first 1,024 port records. All records are preserved in the scan XML.</p>}
    <RawData text={xml} title="View scan XML" />
  </div>;
}

export function ResultContent({ output }: { output: string }) {
  if (!output.trim()) return <p className="result-empty">No output recorded.</p>;
  let parsed: Data;
  try { parsed = JSON.parse(output) as Data; }
  catch { return <p className="result-text result-plain-text">{output}</p>; }
  const nmap = isObject(parsed) && parsed.collector === "nmap" && typeof parsed.xml === "string";
  const value = nmap ? Object.fromEntries(Object.entries(parsed as Record<string, Data>).filter(([key]) => key !== "xml" && key !== "arguments")) : parsed;
  return <>
    <StructuredData value={value} />
    {nmap && <NmapEvidence xml={(parsed as Record<string, Data>).xml as string} />}
    <RawData text={output} />
  </>;
}

function StepResult({ step, index }: { step: ResultStep; index: number }) {
  return <article className="step-result">
    <header className="result-step-heading"><div><span className="result-kicker">Step {index + 1} · {step.provider}{step.model ? ` / ${step.model}` : ""}</span><h4>{label(step.id)}</h4></div><Badge value={step.status} kind="status" /></header>
    {step.error && <div className="error" role="alert">{step.error}</div>}
    <ResultContent output={step.output} />
  </article>;
}

export function RunResults({ steps }: { steps: ResultStep[] }) {
  const indexed = steps.map((step, index) => ({ step, index }));
  const collectors = indexed.filter(({ step }) => step.provider === "nmap" || step.provider === "http");
  const assessments = indexed.filter(({ step }) => step.provider !== "nmap" && step.provider !== "http");
  return <div className="run-results">
    {assessments.map(({ step, index }) => <StepResult key={step.id} step={step} index={index} />)}
    {collectors.length > 0 && <details className="collected-evidence" open={assessments.length === 0 || collectors.some(({ step }) => step.status === "failed")}>
      <summary>Collected evidence <span className="result-count">{collectors.length}</span><span className="evidence-hint">Inspect the data behind this assessment</span></summary>
      {collectors.map(({ step, index }) => <StepResult key={step.id} step={step} index={index} />)}
    </details>}
    {!steps.length && <p className="result-empty">No steps recorded yet.</p>}
  </div>;
}
