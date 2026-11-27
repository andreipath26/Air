#!/usr/bin/env python3
"""Offline integration check for explicit hybrid answer/fallback accounting."""
import hashlib
import json
import os
import pathlib
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parent.parent

AIR_FIXTURE = '''#!/usr/bin/env python3
import json,sys
for line in sys.stdin:
 q=json.loads(line)
 if q.startswith("known"):
  print(json.dumps({"answer":"yes","source":"fixture"}),flush=True)
 else:
  print(json.dumps({"answer":None,"reason":"missing_or_conflicting_fact"}),flush=True)
'''

MODEL_FIXTURE = '''#!/usr/bin/env python3
import json,sys
from http.server import BaseHTTPRequestHandler,HTTPServer
args=sys.argv
port=int(args[args.index("--port")+1])
class Handler(BaseHTTPRequestHandler):
 def do_GET(self):
  self.send_response(200);self.end_headers();self.wfile.write(b"ok")
 def do_POST(self):
  n=int(self.headers.get("Content-Length","0"));self.rfile.read(n)
  body=json.dumps({"choices":[{"message":{"content":"yes"}}]}).encode()
  self.send_response(200);self.send_header("Content-Type","application/json")
  self.send_header("Content-Length",str(len(body)));self.end_headers();self.wfile.write(body)
 def log_message(self,*args):pass
HTTPServer(("127.0.0.1",port),Handler).serve_forever()
'''


def executable(path, source):
    path.write_text(source, encoding="utf-8")
    path.chmod(0o755)


def run_mode(root, mode, port):
    air = root / "air-fixture"
    model = root / "model-server-fixture"
    gguf = root / "model.gguf"
    dataset = root / "dataset.json"
    out = root / ("result-" + mode)
    executable(air, AIR_FIXTURE)
    executable(model, MODEL_FIXTURE)
    gguf.write_bytes(b"fixture weights")
    dataset.write_text(json.dumps([
        {"id":"known","question":"known fixture question?","answer":"yes","group":"test"},
        {"id":"fallback","question":"unsupported fixture question?","answer":"yes","group":"test"},
    ]), encoding="utf-8")
    args = [sys.executable, str(ROOT / "scripts/g02-hybrid.py"), str(dataset), str(out),
            "--air-bin", str(air), "--db", str(gguf), "--policy", str(gguf),
            "--router", str(dataset), "--llama-server", str(model), "--gguf", str(gguf),
            "--expected-model-sha", hashlib.sha256(gguf.read_bytes()).hexdigest(),
            "--expected-server-sha", hashlib.sha256(model.read_bytes()).hexdigest(),
            "--port", str(port), "--fallback-mode", mode]
    subprocess.run(args, check=True, timeout=30, env={**os.environ, "PYTHONDONTWRITEBYTECODE":"1"})
    obs = json.loads((out / "observations.json").read_text(encoding="utf-8"))
    assert obs["direct_air_answers"] == 1, obs
    assert obs["model_fallback_answers"] == 1, obs
    assert obs["air_direct_answer_rate"] == 0.5, obs
    assert obs["fallback_rate"] == 0.5, obs
    rows = [json.loads(line) for line in (out / "responses.jsonl").read_text(encoding="utf-8").splitlines()]
    assert [row["source"] for row in rows] == ["air", "model_fallback"], rows
    assert rows[1]["fallback_reason"] == "missing_or_conflicting_fact", rows
    assert obs["gate_eligible"] is False


def main():
    with tempfile.TemporaryDirectory(prefix="air-g02-hybrid-") as tmp:
        root = pathlib.Path(tmp)
        run_mode(root, "resident", 18741)
        run_mode(root, "on_demand", 18742)
    print("G02 hybrid resident/on-demand accounting passed")


if __name__ == "__main__":
    main()
