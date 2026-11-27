#!/usr/bin/env python3
"""One-command sequential G01 observation runner; no fabricated gate measurements."""
import argparse,hashlib,json,os,pathlib,subprocess,sys,time,urllib.request
import importlib.util
from pathlib import Path
_answer_spec=importlib.util.spec_from_file_location('g01_answer',Path(__file__).with_name('g01-answer.py'))
answer_contract=importlib.util.module_from_spec(_answer_spec);_answer_spec.loader.exec_module(answer_contract)
_resource_spec=importlib.util.spec_from_file_location('g01_resource',Path(__file__).with_name('g01-resource.py'))
resource_meter=importlib.util.module_from_spec(_resource_spec);_resource_spec.loader.exec_module(resource_meter)
ROOT=pathlib.Path(__file__).resolve().parent.parent
def sha(p):
    with open(p,"rb") as f:return hashlib.file_digest(f,"sha256").hexdigest()
def main():
    p=argparse.ArgumentParser()
    p.add_argument("corpus");p.add_argument("dataset");p.add_argument("router");p.add_argument("output")
    p.add_argument("--binary",default=str(ROOT/"target/release/air-cli"))
    p.add_argument("--llama-server");p.add_argument("--gguf");p.add_argument("--model-sha");p.add_argument("--server-sha");p.add_argument("--teacher-id")
    p.add_argument("--port",type=int,default=18081);p.add_argument("--threads",type=int,default=4)
    p.add_argument("--fallback-mode",choices=("resident","on_demand"),default="resident")
    p.add_argument("--g02-review",help="hash-bound review record produced before the run")
    p.add_argument("--hardware",help="JSON hardware identity record for G02 evaluation")
    p.add_argument("--development-reference-only",action="store_true")
    a=p.parse_args()
    if not 1<=a.threads<=8 or not 1024<=a.port<=65535:raise ValueError("resource bounds")
    if not a.development_reference_only and a.port>65534:raise ValueError("port must leave room for hybrid server port")
    if bool(a.g02_review)!=bool(a.hardware):raise ValueError("--g02-review and --hardware must be supplied together")
    out=pathlib.Path(a.output).resolve();out.mkdir(parents=True,exist_ok=False)
    def run(script,*args):
        subprocess.run([sys.executable,str(ROOT/"scripts"/script),*map(str,args)],check=True)
    manifest={"inputs":{k:sha(v) for k,v in [("corpus",a.corpus),("dataset",a.dataset),("router",a.router),("binary",a.binary)]},"development_reference_only":a.development_reference_only,"fallback_mode":a.fallback_mode,"gate_eligible":False}
    (out/"manifest.json").write_text(json.dumps(manifest,indent=2))
    run("g01-corpus.py",a.corpus,out/"reference-store","--mode","reference","--binary",a.binary)
    if not a.development_reference_only:
        if not all([a.llama_server,a.gguf,a.model_sha,a.server_sha,a.teacher_id]):raise ValueError("pinned teacher configuration required")
        if sha(a.llama_server)!=a.server_sha or sha(a.gguf)!=a.model_sha:raise ValueError("teacher hash mismatch")
        cmd=[a.llama_server,"-m",a.gguf,"--host","127.0.0.1","--port",str(a.port),"-ngl","0","-t",str(a.threads),"-c","2048","-np","1"]
        conversion_meter=resource_meter.Meter();conversion_started=time.monotonic()
        with (out/"conversion-server.log").open("w") as log:
            proc=subprocess.Popen(cmd,stdout=log,stderr=log)
            try:
                deadline=time.monotonic()+300
                while True:
                    if proc.poll() is not None:raise RuntimeError("teacher exited")
                    try:
                        with urllib.request.urlopen(f"http://127.0.0.1:{a.port}/health",timeout=2) as r:
                            if r.status==200:break
                    except OSError:pass
                    conversion_meter.sample()
                    if time.monotonic()>deadline:raise TimeoutError("teacher startup")
                    time.sleep(.25)
                run("g01-corpus.py",a.corpus,out/"teacher-store","--mode","teacher","--binary",a.binary,"--port",a.port,"--teacher-id",a.teacher_id,"--teacher-model-sha",a.model_sha)
                conversion_meter.sample()
                conversion_summary_path=out/"teacher-store"/"conversion-summary.json"
                conversion_summary=json.loads(conversion_summary_path.read_text())
                conversion_summary["seconds"]=time.monotonic()-conversion_started
                conversion_summary["energy_joules"]=conversion_meter.report()["cpu_package_energy_joules"]
                conversion_summary["energy_boundary"]=conversion_meter.report()["energy_boundary"]
                conversion_summary_path.write_text(json.dumps(conversion_summary,indent=2))
            finally:
                proc.terminate()
                try:proc.wait(timeout=10)
                except subprocess.TimeoutExpired:proc.kill();proc.wait()
    for name,store in [("reference_retrieval","reference-store")]+([] if a.development_reference_only else [("air","teacher-store")]):
        run("g01-benchmark.py",a.dataset,out/name,"--system",name,"--air-bin",a.binary,"--db",out/store/"knowledge.db","--policy",out/store/"policy.db","--router",a.router)
    if not a.development_reference_only:
        run("g01-benchmark.py",a.dataset,out/"quantized_teacher","--system","quantized_teacher","--llama-server",a.llama_server,"--gguf",a.gguf,"--expected-model-sha",a.model_sha,"--expected-server-sha",a.server_sha,"--port",a.port,"--threads",a.threads)
        run("g02-hybrid.py",a.dataset,out/"hybrid","--air-bin",a.binary,"--db",out/"teacher-store/knowledge.db","--policy",out/"teacher-store/policy.db","--router",a.router,"--llama-server",a.llama_server,"--gguf",a.gguf,"--expected-model-sha",a.model_sha,"--expected-server-sha",a.server_sha,"--port",a.port+1,"--threads",a.threads,"--fallback-mode",a.fallback_mode)
    cases=json.loads(pathlib.Path(a.dataset).read_text());summary={}
    def norm(x):return None if x is None else " ".join(x.casefold().split())
    for name in ["reference_retrieval","air","quantized_teacher","hybrid"]:
        path=out/name/"observations.json"
        if path.exists():
            obs=json.loads(path.read_text());correct=sum(answer_contract.matches(obs["answers"][c["id"]],c["answer"],c["group"]) for c in cases if c["id"] not in obs["errors"])
            diagnostics=obs.get("resource_diagnostics") or {}
            energy=diagnostics.get("cpu_package_energy_joules")
            row={"correct":correct,"total":len(cases),"accuracy_percent":100*correct/len(cases),"correct_answers_per_second":correct/obs["wall_seconds"] if obs["wall_seconds"] else None,"cpu_package_energy_joules":energy,"cpu_package_joules_per_correct":energy/correct if energy is not None and correct else None,"whole_system_used_memory_peak_bytes":diagnostics.get("system_used_memory_peak_bytes"),"sampled_child_peak_rss_bytes":obs.get("sampled_child_peak_rss_bytes"),"observation_sha256":sha(path),"wall_seconds":obs["wall_seconds"],"startup_seconds":obs["startup_seconds"],"resource_diagnostics":diagnostics}
            if name=="hybrid":
                row.update({k:obs.get(k) for k in ("direct_air_answers","model_fallback_answers","fallback_rate","air_direct_answer_rate")})
            summary[name]=row
    conversion={}
    conversion_summary_path=out/"teacher-store"/"conversion-summary.json"
    if conversion_summary_path.exists():
        cs=json.loads(conversion_summary_path.read_text())
        conversion={k:cs.get(k) for k in ("seconds","energy_joules","storage_bytes","energy_boundary")}
    (out/"summary.json").write_text(json.dumps({"systems":summary,"conversion":conversion,"gate_eligible":False,"note":"Independent review and hardware evidence are still required; CPU package energy is not wall-power energy."},indent=2))
    if a.g02_review and not a.development_reference_only:
        run("g02-evaluate.py",a.dataset,out,a.g02_review,a.hardware,conversion_summary_path,out/"g02-evaluation.json")
if __name__=="__main__":main()
