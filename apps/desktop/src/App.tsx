import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type AppHealth = {
  name: string;
  version: string;
  status: string;
};

export function App() {
  const [health, setHealth] = useState<AppHealth | null>(null);

  useEffect(() => {
    invoke<AppHealth>("health").then(setHealth).catch(() => {
      setHealth({ name: "PTConductor", version: "dev", status: "browser preview" });
    });
  }, []);

  return (
    <main>
      <p className="eyebrow">LOCAL-FIRST SECURITY WORKFLOWS</p>
      <h1>PTConductor</h1>
      <p className="lede">
        Compose authorized testing workflows, provide typed inputs, and retain structured results.
      </p>
      <section aria-label="Application status">
        <span className="status-dot" />
        {health ? `${health.status} · ${health.version}` : "Connecting to engine…"}
      </section>
    </main>
  );
}

