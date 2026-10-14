"""Offline CLI contract tests: real HTTP/SQLite, fake Codex/Nmap; optional real Nmap.
Run after cargo build -p ptconductor-cli. No account, API key or remote target needed.
"""
import http.server
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import threading
import unittest
import uuid

ROOT = Path(__file__).resolve().parents[1]
CLI = Path(os.environ.get("PTC_TEST_BINARY", ROOT / "target/debug/ptconductor")).resolve()

CODEX = r'''#!/usr/bin/env python3
import json, os, pathlib, sys
args = sys.argv[1:]
if args == ["exec", "--help"]:
    print("--output-last-message --ephemeral --ignore-user-config")
    sys.exit(0)
if args == ["login", "status"]:
    print("Logged in using " + os.environ.get("TEST_LOGIN", "ChatGPT"), file=sys.stderr)
    sys.exit(0)
assert "OPENAI_API_KEY" not in os.environ and "CODEX_API_KEY" not in os.environ
assert args[:3] == ["--ask-for-approval", "never", "exec"]
assert args[args.index("--sandbox") + 1] == "read-only"
assert all(flag in args for flag in ["--ephemeral", "--ignore-user-config", "--skip-git-repo-check"])
assert args[-1] == "-"
assert "--model" not in args
prompt = sys.stdin.read()
pathlib.Path(os.environ["TEST_PROMPT"]).write_text(prompt)
pathlib.Path(os.environ["TEST_ARGS"]).write_text(json.dumps(args))
mode = os.environ.get("TEST_MODE", "success")
if mode == "quota":
    print("429 quota exhausted private-stderr-value", file=sys.stderr)
    sys.exit(1)
if mode == "missing": sys.exit(0)
answer = pathlib.Path(args[args.index("--output-last-message") + 1])
if mode == "invalid": answer.write_text("not JSON")
elif mode == "oversized": answer.write_text("x" * (1024 * 1024 + 1))
else: answer.write_text(json.dumps({"summary": "<script>alert(1)</script> fixture-secret", "findings": [], "limitations": ["Test fixture; no AI inference"]}))
print("progress noise that is NOT the final result")
'''
NMAP = r'''#!/usr/bin/env python3
import json, os, pathlib, sys
pathlib.Path(os.environ["TEST_NMAP_ARGS"]).write_text(json.dumps(sys.argv[1:]))
if os.environ.get("TEST_MODE") == "nmap_error": sys.exit(2)
print('<nmaprun><host><ports><port protocol="tcp" portid="80"><state state="open"/><service name="http"/></port></ports></host><runstats><finished exit="success"/></runstats></nmaprun>')
'''


class Handler(http.server.BaseHTTPRequestHandler):
    hits = []

    def do_GET(self):
        self.hits.append(self.path)
        self.send_response(302 if self.path == "/redirect" else 200)
        self.send_header("Content-Security-Policy", "default-src 'self'")
        self.send_header("X-Content-Type-Options", "nosniff")
        self.send_header("Set-Cookie", "session=must-not-be-collected")
        self.send_header("Location", "/never-follow")
        self.end_headers()

    def log_message(self, *args):
        pass


class WorkflowTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        threading.Thread(target=cls.server.serve_forever, daemon=True).start()
        cls.url = f"http://127.0.0.1:{cls.server.server_port}"

    @classmethod
    def tearDownClass(cls):
        cls.server.shutdown()
        cls.server.server_close()

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="ptc integration ")
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.bin = self.root / "bin with spaces"
        self.bin.mkdir()
        for name, content in [("codex", CODEX), ("nmap", NMAP)]:
            path = self.bin / name
            path.write_text(content)
            path.chmod(0o700)
        self.env = dict(os.environ, PATH=str(self.bin) + os.pathsep + os.environ["PATH"],
                        PTCONDUCTOR_CODEX_BIN=str(self.bin / "codex"),
                        PTCONDUCTOR_RUNS_DIR=str(self.root / "runs"),
                        TEST_PROMPT=str(self.root / "prompt.txt"), TEST_ARGS=str(self.root / "args.json"),
                        TEST_NMAP_ARGS=str(self.root / "nmap.json"), OPENAI_API_KEY="not-a-real-key",
                        CODEX_API_KEY="not-a-real-key")

    def cli(self, *args, ok=True):
        result = subprocess.run([str(CLI), *map(str, args)], cwd=ROOT, env=self.env,
                                text=True, capture_output=True, timeout=100)
        if ok:
            self.assertEqual(result.returncode, 0, result.stderr)
        else:
            self.assertNotEqual(result.returncode, 0, result.stdout)
        return result

    def run_flow(self, filename="security-headers-summary.json", *extra, ok=True):
        return self.cli("run", ROOT / "workflows/examples" / filename, "--provider", "codex",
                        "--allow-remote", "--allow-active", *extra, ok=ok)

    def history(self):
        return json.loads(self.cli("history", "--json").stdout)

    def test_doctor_and_account_preflight_before_collection(self):
        self.cli("doctor")
        self.env["TEST_LOGIN"] = "API key"
        error = self.run_flow("nmap-scan-summary.json", ok=False)
        self.assertIn("signed in with ChatGPT", error.stderr)
        self.assertFalse((self.root / "nmap.json").exists())

    def test_http_evidence_codex_final_file_history_and_reports(self):
        export = self.root / "result.json"
        result = self.run_flow("security-headers-summary.json", "--input", f"TARGET={self.url}",
                               "--output-json", export)
        run = json.loads(result.stdout)
        self.assertEqual(run["status"], "completed")
        self.assertEqual(len(run["steps"]), 2)
        evidence = json.loads(run["steps"][0]["output"])
        self.assertEqual(evidence["headers"]["x-content-type-options"], ["nosniff"])
        self.assertNotIn("set-cookie", evidence["headers"])
        self.assertIn("nosniff", (self.root / "prompt.txt").read_text())
        self.assertNotIn("must-not-be-collected", (self.root / "prompt.txt").read_text())
        self.assertEqual(json.loads(export.read_text()), json.loads(run["steps"][1]["output"]))
        self.assertEqual(self.history()[0]["id"], run["id"])
        self.assertEqual(json.loads(self.cli("show", run["id"]).stdout), run)
        for fmt, extension in [("html", "html"), ("markdown", "md")]:
            report = self.root / ("report." + extension)
            self.cli("report", run["id"], "--format", fmt, "--output", report)
            self.assertIn("nosniff", report.read_text())
            self.cli("report", run["id"], "--format", fmt, "--output", report, ok=False)
        html = (self.root / "report.html").read_text()
        self.assertNotIn("<script>", html)
        self.assertIn("&lt;script&gt;", html)
        answer_path = Path(json.loads((self.root / "args.json").read_text())[-2])
        self.assertFalse(answer_path.parent.exists(), "temporary Codex workspace leaked")

    def test_nmap_fixed_arguments_and_collected_xml_reaches_codex(self):
        run = json.loads(self.run_flow("nmap-scan-summary.json").stdout)
        args = json.loads((self.root / "nmap.json").read_text())
        self.assertEqual(args, ["-sT", "-sV", "--version-light", "-Pn", "-T3", "--host-timeout",
                                "60s", "-p", "22,80,443", "-oX", "-", "127.0.0.1"])
        self.assertIn("nmaprun", (self.root / "prompt.txt").read_text())
        self.assertEqual(run["steps"][0]["provider"], "nmap")

    def test_consent_and_malformed_targets_block_before_execution(self):
        flow = ROOT / "workflows/examples/nmap-scan-summary.json"
        self.cli("run", flow, "--provider", "codex", "--allow-remote", ok=False)
        self.cli("run", flow, "--provider", "codex", "--allow-active", ok=False)
        for target in ["-iL", "127.0.0.1;id", "10.0.0.0/8"]:
            self.run_flow("nmap-scan-summary.json", "--input", "TARGET=" + target, ok=False)
        self.assertFalse((self.root / "nmap.json").exists())
        self.assertFalse((self.root / "prompt.txt").exists())

    def test_provider_errors_preserve_evidence_and_failed_history(self):
        for mode in ["quota", "missing", "invalid", "oversized"]:
            with self.subTest(mode=mode):
                self.env["TEST_MODE"] = mode
                error = self.run_flow("security-headers-summary.json", "--input", f"TARGET={self.url}", ok=False)
                self.assertNotIn("private-stderr-value", error.stderr)
                latest = self.history()[0]
                self.assertEqual(latest["status"], "failed")
                run = json.loads(self.cli("show", latest["id"]).stdout)
                self.assertEqual(run["steps"][0]["status"], "completed")
                self.assertEqual(run["steps"][1]["status"], "failed")
                self.assertIn(latest["id"], error.stderr)

    def test_collector_failure_does_not_invoke_ai(self):
        self.env["TEST_MODE"] = "nmap_error"
        self.run_flow("nmap-scan-summary.json", ok=False)
        self.assertFalse((self.root / "prompt.txt").exists())
        self.assertEqual(self.history()[0]["status"], "failed")

    def test_http_redirect_is_not_followed(self):
        Handler.hits.clear()
        run = json.loads(self.run_flow("security-headers-summary.json", "--input",
                                      f"TARGET={self.url}/redirect").stdout)
        self.assertEqual(Handler.hits, ["/redirect"])
        self.assertEqual(json.loads(run["steps"][0]["output"])["status"], 302)

    def test_secret_values_are_redacted_everywhere_persisted(self):
        definition = json.loads((ROOT / "workflows/examples/security-headers-summary.json").read_text())
        definition["inputs"] += [{"name": "TOKEN", "label": "Token", "type": "secret", "required": True},
                                  {"name": "COPY", "label": "Copy", "type": "text", "required": True}]
        definition["steps"] = definition["steps"][1:]
        definition["steps"][0]["prompt"] = "Return JSON: <TOKEN> <COPY>"
        path = self.root / "secret.json"
        path.write_text(json.dumps(definition))
        args = ["--input", "TOKEN=fixture-secret", "--input", "COPY=fixture-secret"]
        preview = self.cli("prepare", path, *args).stdout
        self.assertNotIn("fixture-secret", preview)
        run = json.loads(self.cli("run", path, "--provider", "codex", "--allow-remote", *args).stdout)
        self.assertNotIn("fixture-secret", json.dumps(run))
        self.assertNotIn("fixture-secret", json.dumps(self.history()))
        self.assertNotIn("fixture-secret", self.cli("report", run["id"], "--format", "html").stdout)
        self.assertIn("fixture-secret", (self.root / "prompt.txt").read_text())
        self.assertEqual((self.root / "runs" / (run["id"] + ".json")).stat().st_mode & 0o777, 0o600)

    def test_index_rebuild_and_corrupt_artifact_isolation(self):
        run = json.loads(self.run_flow("nmap-scan-summary.json").stdout)
        for path in (self.root / "runs").glob("index.sqlite*"):
            path.unlink()
        corrupt = self.root / "runs" / (str(uuid.uuid4()) + ".json")
        corrupt.write_text("not JSON")
        self.assertEqual(self.history()[0]["id"], run["id"])
        self.assertTrue(corrupt.exists())

    @unittest.skipUnless(os.environ.get("PTC_TEST_REAL_NMAP") == "1", "enable real loopback Nmap explicitly")
    def test_real_nmap_loopback(self):
        real = shutil.which("nmap", path=os.environ["PATH"])
        self.assertIsNotNone(real, "install nmap for this test")
        (self.bin / "nmap").unlink()
        run = json.loads(self.run_flow("nmap-scan-summary.json", "--input", f"PORTS={self.server.server_port}").stdout)
        xml = json.loads(run["steps"][0]["output"])["xml"]
        self.assertIn('state="open"', xml)
        self.assertIn(f'portid="{self.server.server_port}"', xml)


if __name__ == "__main__":
    unittest.main(verbosity=2)
