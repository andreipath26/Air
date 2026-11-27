#!/usr/bin/env python3
"""Measure AIR-first routing with explicit, metered llama.cpp fallbacks."""
import argparse
import hashlib
import importlib.util
import json
import pathlib
import select
import subprocess
import threading
import time
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location("air_resource", ROOT / "scripts/g01-resource.py")
resource = importlib.util.module_from_spec(spec)
spec.loader.exec_module(resource)
spec = importlib.util.spec_from_file_location("answer_contract", ROOT / "scripts/g01-answer.py")
answer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(answer)


def sha(path):
    with open(path, "rb") as f:
        return hashlib.file_digest(f, "sha256").hexdigest()


def stop(proc):
    if proc is None or proc.poll() is not None:
        return
    proc.terminate()
    try:
        proc.wait(timeout=10)
    except subprocess.TimeoutExpired:
        proc.kill()
        proc.wait()


def wait_server(proc, port, meter=None):
    url = f"http://127.0.0.1:{port}"
    deadline = time.monotonic() + 300
    while True:
        if proc.poll() is not None:
            raise RuntimeError("llama-server exited before becoming healthy")
        try:
            with urllib.request.urlopen(url + "/health", timeout=2) as response:
                if response.status == 200:
                    return url
        except OSError:
            pass
        if meter is not None:
            meter.sample()
        if time.monotonic() >= deadline:
            raise TimeoutError("llama-server startup timed out")
        time.sleep(0.25)


def call_model(url, question):
    body = {
        "messages": [
            {"role": "system", "content": "Give only the short factual answer, or unknown if uncertain."},
            {"role": "user", "content": question},
        ],
        "temperature": 0,
        "max_tokens": 64,
        "stream": False,
    }
    req = urllib.request.Request(
        url + "/v1/chat/completions",
        data=json.dumps(body).encode(),
        headers={"Content-Type": "application/json"},
    )
    with urllib.request.urlopen(req, timeout=120) as response:
        raw = json.loads(response.read(65537))
    text = raw["choices"][0]["message"]["content"]
    extracted, status = answer.extract(text)
    if status == "malformed":
        raise ValueError("malformed model answer")
    return extracted, raw, status


def main():
    p = argparse.ArgumentParser()
    p.add_argument("dataset")
    p.add_argument("output")
    p.add_argument("--air-bin", required=True)
    p.add_argument("--db", required=True)
    p.add_argument("--policy", required=True)
    p.add_argument("--router", required=True)
    p.add_argument("--llama-server", required=True)
    p.add_argument("--gguf", required=True)
    p.add_argument("--expected-model-sha", required=True)
    p.add_argument("--expected-server-sha", required=True)
    p.add_argument("--port", type=int, default=18082)
    p.add_argument("--threads", type=int, default=4)
    p.add_argument("--fallback-mode", choices=("resident", "on_demand"), default="resident")
    a = p.parse_args()
    if not 1024 <= a.port <= 65535 or not 1 <= a.threads <= 8:
        raise ValueError("port or thread count outside bounded limits")
    for path in (a.dataset, a.air_bin, a.db, a.policy, a.router, a.llama_server, a.gguf):
        if not pathlib.Path(path).is_file():
            raise FileNotFoundError(path)
    if sha(a.gguf) != a.expected_model_sha or sha(a.llama_server) != a.expected_server_sha:
        raise ValueError("pinned model/server hash mismatch")

    dataset_raw = pathlib.Path(a.dataset).read_bytes()
    if len(dataset_raw) > 8 * 1024 * 1024:
        raise ValueError("dataset exceeds 8 MiB limit")
    cases = json.loads(dataset_raw)
    if not isinstance(cases, list) or not 1 <= len(cases) <= 10000:
        raise ValueError("dataset must be a non-empty JSON array")
    ids = [c.get("id") for c in cases]
    if any(not isinstance(x, str) or not x for x in ids) or len(set(ids)) != len(ids):
        raise ValueError("case IDs must be unique non-empty strings")
    if any(not isinstance(c.get("question"), str) or not 1 <= len(c["question"]) <= 4096 for c in cases):
        raise ValueError("each case needs a non-empty question")

    out = pathlib.Path(a.output)
    out.mkdir(parents=True, exist_ok=False)
    air_cmd = [a.air_bin, "factual-serve", a.db, a.policy, a.router]
    model_cmd = [a.llama_server, "-m", a.gguf, "--host", "127.0.0.1", "--port", str(a.port),
                 "-ngl", "0", "-t", str(a.threads), "-c", "2048", "-np", "1"]
    identity = {
        "binary_sha256": sha(a.air_bin), "database_sha256": sha(a.db),
        "policy_sha256": sha(a.policy), "router_sha256": sha(a.router),
        "model_sha256": a.expected_model_sha, "server_sha256": a.expected_server_sha,
        "binary_size_bytes": pathlib.Path(a.air_bin).stat().st_size,
        "database_size_bytes": pathlib.Path(a.db).stat().st_size,
        "policy_size_bytes": pathlib.Path(a.policy).stat().st_size,
        "router_size_bytes": pathlib.Path(a.router).stat().st_size,
        "model_size_bytes": pathlib.Path(a.gguf).stat().st_size,
        "server_size_bytes": pathlib.Path(a.llama_server).stat().st_size,
        "fallback_mode": a.fallback_mode,
    }
    answers = {}
    errors = {}
    direct = 0
    fallback = 0
    peak_rss = 0
    meter = resource.Meter()
    stop_sampling = threading.Event()
    processes = []
    air_proc = None
    model_proc = None
    started = time.monotonic()

    with (out / "stderr.log").open("w", encoding="utf-8") as log, \
         (out / "responses.jsonl").open("w", encoding="utf-8") as trace:
        try:
            air_proc = subprocess.Popen(air_cmd, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                        stderr=log, text=True, bufsize=1)
            processes.append(air_proc)
            if a.fallback_mode == "resident":
                model_proc = subprocess.Popen(model_cmd, stdout=log, stderr=log)
                processes.append(model_proc)
                model_url = wait_server(model_proc, a.port, meter)
            else:
                model_url = None
            ready = time.monotonic()

            def sample():
                nonlocal peak_rss
                while not stop_sampling.is_set():
                    rss_total = 0
                    for proc in list(processes):
                        if proc.poll() is None:
                            try:
                                for line in pathlib.Path(f"/proc/{proc.pid}/status").read_text().splitlines():
                                    if line.startswith("VmRSS:"):
                                        rss_total += int(line.split()[1]) * 1024
                            except (OSError, ValueError):
                                pass
                    peak_rss = max(peak_rss, rss_total)
                    meter.sample()
                    stop_sampling.wait(0.05)

            monitor = threading.Thread(target=sample, daemon=True)
            monitor.start()
            started_inference = time.monotonic()
            for case in cases:
                case_started = time.monotonic()
                source = "air"
                raw_air = None
                raw_model = None
                fallback_reason = None
                try:
                    if air_proc.poll() is not None:
                        raise RuntimeError("AIR factual server exited")
                    air_proc.stdin.write(json.dumps(case["question"]) + "\n")
                    air_proc.stdin.flush()
                    if not select.select([air_proc.stdout], [], [], 120)[0]:
                        raise TimeoutError("AIR response timed out")
                    raw_air = json.loads(air_proc.stdout.readline(65537))
                    value = raw_air.get("answer")
                    if value is not None:
                        direct += 1
                    else:
                        fallback += 1
                        source = "model_fallback"
                        fallback_reason = raw_air.get("reason", "air_abstained")
                        if a.fallback_mode == "on_demand":
                            model_proc = subprocess.Popen(model_cmd, stdout=log, stderr=log)
                            processes.append(model_proc)
                            model_url = wait_server(model_proc, a.port, meter)
                        value, raw_model, model_status = call_model(model_url, case["question"])
                        if a.fallback_mode == "on_demand":
                            stop(model_proc)
                            model_proc = None
                            model_url = None
                    answers[case["id"]] = value
                    trace.write(json.dumps({
                        "id": case["id"], "answer": value, "source": source,
                        "fallback_reason": fallback_reason, "air_response": raw_air,
                        "model_response": raw_model, "seconds": time.monotonic() - case_started,
                    }, ensure_ascii=False) + "\n")
                except Exception as exc:
                    errors[case["id"]] = type(exc).__name__
                    trace.write(json.dumps({"id": case["id"], "source": source,
                                            "error": type(exc).__name__, "detail": str(exc)[:300],
                                            "air_response": raw_air, "model_response": raw_model},
                                           ensure_ascii=False) + "\n")
                    raise
                finally:
                    trace.flush()
            inference_seconds = time.monotonic() - started_inference
        finally:
            stop_sampling.set()
            if "monitor" in locals():
                monitor.join(timeout=2)
            meter.sample()
            for proc in reversed(processes):
                stop(proc)

    obs = {
        "system": "hybrid", "dataset_sha256": hashlib.sha256(dataset_raw).hexdigest(), "identity": identity,
        "answers": answers, "errors": errors, "direct_air_answers": direct,
        "model_fallback_answers": fallback,
        "fallback_rate": fallback / len(cases),
        "air_direct_answer_rate": direct / len(cases),
        "wall_seconds": inference_seconds,
        "total_wall_seconds_including_startup": time.monotonic() - started,
        "startup_seconds": ready - started,
        "sampled_child_peak_rss_bytes": peak_rss,
        "resource_diagnostics": meter.report(),
        "measurement_scope": "whole-host used memory sampled from /proc/meminfo; CPU package RAPL when exposed; not external wall-power metering",
        "gate_eligible": False,
    }
    (out / "observations.json").write_text(json.dumps(obs, indent=2), encoding="utf-8")
    if errors:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
