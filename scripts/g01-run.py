#!/usr/bin/env python3
"""One-command sequential G01 observation runner; no fabricated gate measurements."""
import argparse,hashlib,json,os,pathlib,subprocess,sys,time,urllib.request
ROOT=pathlib.Path(__file__).resolve().parent.parent
def sha(p):
    with open(p,"rb") as f:return hashlib.file_digest(f,"sha256").hexdigest()
def main():
    p=argparse.ArgumentParser()
    p.add_argument("corpus");p.add_argument("dataset");p.add_argument("router");p.add_argument("output")
    p.add_argument("--binary",default=str(ROOT/"target/release/air-cli"))
    p.add_argument("--llama-server");p.add_argument("--gguf");p.add_argument("--model-sha");p.add_argument("--server-sha");p.add_argument("--teacher-id")
    p.add_argument("--port",type=int,default=18081);p.add_argument("--threads",type=int,default=4)
    p.add_argument("--development-reference-only",action="store_true")
    a=p.parse_args()
    if not 1<=a.threads<=8 or not 1024<=a.port<=65535:raise ValueError("resource bounds")
    out=pathlib.Path(a.output).resolve();out.mkdir(parents=True,exist_ok=False)
    def run(script,*args):
        subprocess.run([sys.executable,str(ROOT/"scripts"/script),*map(str,args)],check=True)
    manifest={"inputs":{k:sha(v) for k,v in [("corpus",a.corpus),("dataset",a.dataset),("router",a.router),("binary",a.binary)]},"development_reference_only":a.development_reference_only,"gate_eligible":False}
    (out/"manifest.json").write_text(json.dumps(manifest,indent=2))
    run("g01-corpus.py",a.corpus,out/"reference-store","--mode","reference","--binary",a.binary)
    if not a.development_reference_only:
        if not all([a.llama_server,a.gguf,a.model_sha,a.server_sha,a.teacher_id]):raise ValueError("pinned teacher configuration required")
        if sha(a.llama_server)!=a.server_sha or sha(a.gguf)!=a.model_sha:raise ValueError("teacher hash mismatch")
        cmd=[a.llama_server,"-m",a.gguf,"--host","127.0.0.1","--port",str(a.port),"-ngl","0","-t",str(a.threads),"-c","2048","-np","1"]
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
                    if time.monotonic()>deadline:raise TimeoutError("teacher startup")
                    time.sleep(.25)
                run("g01-corpus.py",a.corpus,out/"teacher-store","--mode","teacher","--binary",a.binary,"--port",a.port,"--teacher-id",a.teacher_id,"--teacher-model-sha",a.model_sha)
            finally:
                proc.terminate()
                try:proc.wait(timeout=10)
                except subprocess.TimeoutExpired:proc.kill();proc.wait()
    for name,store in [("reference_retrieval","reference-store")]+([] if a.development_reference_only else [("air","teacher-store")]):
        run("g01-benchmark.py",a.dataset,out/name,"--system",name,"--air-bin",a.binary,"--db",out/store/"knowledge.db","--policy",out/store/"policy.db","--router",a.router)
    if not a.development_reference_only:
        run("g01-benchmark.py",a.dataset,out/"quantized_teacher","--system","quantized_teacher","--llama-server",a.llama_server,"--gguf",a.gguf,"--expected-model-sha",a.model_sha,"--expected-server-sha",a.server_sha,"--port",a.port,"--threads",a.threads)
    cases=json.loads(pathlib.Path(a.dataset).read_text());summary={}
    def norm(x):return None if x is None else " ".join(x.casefold().split())
    for name in ["reference_retrieval","air","quantized_teacher"]:
        path=out/name/"observations.json"
        if path.exists():
            obs=json.loads(path.read_text());correct=sum(norm(obs["answers"][c["id"]])==norm(c["answer"]) for c in cases if c["id"] not in obs["errors"])
            summary[name]={"correct":correct,"total":len(cases),"observation_sha256":sha(path),"wall_seconds":obs["wall_seconds"]}
    (out/"summary.json").write_text(json.dumps({"systems":summary,"gate_eligible":False,"note":"Independent review and whole-system energy evidence remain required."},indent=2))
if __name__=="__main__":main()
