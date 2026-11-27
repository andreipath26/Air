#!/usr/bin/env python3
"""Run one closed-world system against a frozen workload; output raw observations.
Process RSS and CPU package RAPL are diagnostics, not whole-system gate measurements.
"""
import argparse, hashlib, json, pathlib, subprocess, time, urllib.request, os, threading, importlib.util, select
spec=importlib.util.spec_from_file_location('g01_resource',pathlib.Path(__file__).with_name('g01-resource.py'))
resource=importlib.util.module_from_spec(spec);spec.loader.exec_module(resource)
import importlib.util
from pathlib import Path
_answer_spec=importlib.util.spec_from_file_location('g01_answer',Path(__file__).with_name('g01-answer.py'))
answer_contract=importlib.util.module_from_spec(_answer_spec);_answer_spec.loader.exec_module(answer_contract)
def read(path, cap=8*1024*1024):
    with open(path,"rb") as f: raw=f.read(cap+1)
    if len(raw)>cap: raise ValueError("input limit")
    return raw,json.loads(raw)
def sha(path):
    with open(path,"rb") as f:return hashlib.file_digest(f,"sha256").hexdigest()
def main():
    p=argparse.ArgumentParser()
    p.add_argument("dataset");p.add_argument("output")
    p.add_argument("--system",choices=["air","reference_retrieval","quantized_teacher"],required=True)
    p.add_argument("--air-bin",default="target/release/air-cli")
    p.add_argument("--db");p.add_argument("--policy");p.add_argument("--router")
    p.add_argument("--llama-server");p.add_argument("--gguf");p.add_argument("--port",type=int,default=18081)
    p.add_argument("--threads",type=int,default=4);p.add_argument("--expected-model-sha")
    p.add_argument("--expected-server-sha")
    args=p.parse_args()
    if not 1<=args.threads<=8 or not 1024<=args.port<=65535:raise ValueError("resource limits")
    raw,cases=read(args.dataset)
    if not isinstance(cases,list) or not 1<=len(cases)<=10000:raise ValueError("workload limit")
    ids=set()
    for c in cases:
        if not isinstance(c["id"],str) or c["id"] in ids or not isinstance(c["question"],str) or not 1<=len(c["question"])<=4096:raise ValueError("invalid case")
        ids.add(c["id"])
    out=pathlib.Path(args.output);out.mkdir(parents=True,exist_ok=False)
    if args.system=="quantized_teacher":
        if not all([args.llama_server,args.gguf,args.expected_model_sha,args.expected_server_sha]):raise ValueError("pinned model/server hashes required")
        if sha(args.gguf)!=args.expected_model_sha or sha(args.llama_server)!=args.expected_server_sha:raise ValueError("baseline hash mismatch")
        cmd=[args.llama_server,"-m",args.gguf,"--host","127.0.0.1","--port",str(args.port),"-ngl","0","-t",str(args.threads),"-c","2048","-np","1"]
        identity={"model_sha256":args.expected_model_sha,"server_sha256":args.expected_server_sha}
    else:
        if not all([args.db,args.policy,args.router]):raise ValueError("database, policy, router required")
        cmd=[args.air_bin,"factual-serve",args.db,args.policy,args.router]
        identity={k:sha(v) for k,v in [("binary_sha256",args.air_bin),("database_sha256",args.db),("policy_sha256",args.policy),("router_sha256",args.router)]}
    answers={};errors={};peak=0;stop=threading.Event()
    meter=resource.Meter()
    started=time.monotonic()
    with (out/"stderr.log").open("w") as log,(out/"responses.jsonl").open("w") as trace:
        proc=subprocess.Popen(cmd,stdin=subprocess.PIPE,stdout=subprocess.PIPE if args.system!="quantized_teacher" else log,stderr=log,text=True,bufsize=1)
        def sample():
            nonlocal peak
            while not stop.is_set():
                try:
                    for line in pathlib.Path(f"/proc/{proc.pid}/status").read_text().splitlines():
                        if line.startswith("VmRSS:"):peak=max(peak,int(line.split()[1])*1024)
                except OSError:pass
                meter.sample()
                stop.wait(.02)
        monitor=threading.Thread(target=sample,daemon=True);monitor.start()
        try:
            if args.system=="quantized_teacher":
                url=f"http://127.0.0.1:{args.port}"
                deadline=time.monotonic()+300
                while True:
                    if proc.poll() is not None:raise RuntimeError("server exited")
                    try:
                        with urllib.request.urlopen(url+"/health",timeout=2) as r:
                            if r.status==200:break
                    except OSError:pass
                    if time.monotonic()>deadline:raise TimeoutError("server startup")
                    time.sleep(.25)
            ready=time.monotonic()
            for c in cases:
                now=time.monotonic();answer=None;response=None
                try:
                    if args.system=="quantized_teacher":
                        body={"messages":[{"role":"system","content":"Give only the short factual answer, or unknown if uncertain."},{"role":"user","content":c["question"]}],"temperature":0,"max_tokens":64,"stream":False}
                        req=urllib.request.Request(url+"/v1/chat/completions",data=json.dumps(body).encode(),headers={"Content-Type":"application/json"})
                        with urllib.request.urlopen(req,timeout=120) as r:response=json.loads(r.read(65537))
                        text=response["choices"][0]["message"]["content"].strip()
                        if not text:raise ValueError("empty teacher output")
                        answer,status=answer_contract.extract(text)
                        if status=="malformed":raise ValueError("malformed teacher answer")
                    else:
                        proc.stdin.write(json.dumps(c["question"])+"\n");proc.stdin.flush()
                        if not select.select([proc.stdout],[],[],120)[0]:raise TimeoutError('AIR response timeout')
                        response=json.loads(proc.stdout.readline(65537))
                        answer=response["answer"]
                except Exception as e:
                    errors[c["id"]]=type(e).__name__
                    if args.system!="quantized_teacher":raise RuntimeError("resident protocol failed; stop to avoid response misalignment") from e
                answers[c["id"]]=answer
                trace.write(json.dumps({"id":c["id"],"answer":answer,"raw":response,"error":errors.get(c["id"]),"seconds":time.monotonic()-now})+"\n");trace.flush()
            elapsed=time.monotonic()-ready
        finally:
            stop.set();monitor.join();meter.sample();proc.terminate()
            try:proc.wait(timeout=10)
            except subprocess.TimeoutExpired:proc.kill();proc.wait()
    result={"system":args.system,"dataset_sha256":hashlib.sha256(raw).hexdigest(),"identity":identity,"command":cmd,"answers":answers,"errors":errors,"wall_seconds":elapsed,"startup_seconds":ready-started,"sampled_process_peak_rss_bytes":peak,"measurement_scope":"sampled child RSS and system memory; optional CPU package energy; not certified whole-system electrical energy","gate_eligible":False,"resource_diagnostics":meter.report()}
    (out/"observations.json").write_text(json.dumps(result,indent=2))
    if errors:raise SystemExit(1)
if __name__=="__main__":main()
