#!/usr/bin/env python3
"""Resolve and fingerprint a Hugging Face teacher, then run three transfer tracks."""
import argparse, hashlib, json, os, pathlib, subprocess, sys, threading, time
from importlib.util import spec_from_file_location, module_from_spec
from http.server import BaseHTTPRequestHandler, HTTPServer
from huggingface_hub import HfApi, snapshot_download
import torch
from transformers import AutoTokenizer, AutoModelForCausalLM

def main():
    ap=argparse.ArgumentParser()
    ap.add_argument("--out",required=True)
    ap.add_argument("--revision",default="main")
    ap.add_argument("--model",default="Qwen/Qwen2.5-0.5B-Instruct")
    ap.add_argument("--structural",action="store_true")
    args=ap.parse_args()
    out=pathlib.Path(args.out); out.mkdir(parents=True,exist_ok=False)
    info=HfApi().model_info(args.model,revision=args.revision)
    revision=info.sha
    license_id=getattr(info.card_data,"license",None)
    if license_id!="apache-2.0":
        raise RuntimeError("Pilot permits only Apache-2.0 teacher checkpoints")
    snapshot=pathlib.Path(snapshot_download(args.model,revision=revision,allow_patterns=["*.safetensors","*.json","*.txt","*.model","LICENSE","README.md"]))
    weight_files=sorted(snapshot.glob("*.safetensors"))
    if not weight_files: raise RuntimeError("No safe weights")
    identity={"model":args.model,"revision":revision,"license":license_id,"weights":[],"torch":torch.__version__}
    for i,p in enumerate(weight_files):
        with p.open("rb") as f: h=hashlib.file_digest(f,"sha256").hexdigest()
        identity["weights"].append({"file":p.name,"sha256":h,"bytes":p.stat().st_size})
        with (out/f"inventory-{i}.json").open("w") as f: subprocess.run(["target/release/air-cli","amt-inspect",str(p)],stdout=f,check=True)
        with (out/f"stats-{i}.json").open("w") as f: subprocess.run(["target/release/air-cli","amt-stats",str(p)],stdout=f,check=True)
    (out/"identity.json").write_text(json.dumps(identity,indent=2))
    torch.set_num_threads(min(4,os.cpu_count() or 1))
    tokenizer=AutoTokenizer.from_pretrained(snapshot,trust_remote_code=False,local_files_only=True)
    model=AutoModelForCausalLM.from_pretrained(snapshot,trust_remote_code=False,local_files_only=True,use_safetensors=True,torch_dtype=torch.float32).eval()
    if args.structural:
        spec=spec_from_file_location("amt_structural",pathlib.Path(__file__).with_name("amt-structural.py"))
        structural=module_from_spec(spec);spec.loader.exec_module(structural)
        structural.inspect_activations(model,tokenizer,out/"structural-report.json")
    class Handler(BaseHTTPRequestHandler):
        def do_POST(self):
            try:
                n=int(self.headers.get("Content-Length","0"))
                if self.path!="/v1/chat/completions" or n<1 or n>65536: self.send_error(400); return
                body=json.loads(self.rfile.read(n))
                text=tokenizer.apply_chat_template(body["messages"],tokenize=False,add_generation_prompt=True)
                inputs=tokenizer(text,return_tensors="pt")
                with torch.inference_mode(): tokens=model.generate(**inputs,max_new_tokens=64,do_sample=False,pad_token_id=tokenizer.eos_token_id)
                answer=tokenizer.decode(tokens[0,inputs["input_ids"].shape[1]:],skip_special_tokens=True).strip()
                with (out/"raw-responses.jsonl").open("a",encoding="utf-8") as trace:
                    trace.write(json.dumps({"messages":body["messages"],"raw":answer,"requested_format":body.get("response_format"),"format_enforced":False},ensure_ascii=False)+"\n")
                fence=chr(96)*3
                if answer.startswith(fence) and answer.endswith(fence):
                    answer=answer.split("\n",1)[-1][:-3].strip()
                result=json.dumps({"choices":[{"message":{"content":answer}}]}).encode()
                self.send_response(200); self.send_header("Content-Type","application/json"); self.send_header("Content-Length",str(len(result)));self.end_headers();self.wfile.write(result)
            except Exception as e:
                print(type(e).__name__,file=sys.stderr); self.send_error(500)
    server=HTTPServer(("127.0.0.1",0),Handler)
    thread=threading.Thread(target=server.serve_forever,daemon=True); thread.start()
    env=dict(os.environ,AIR_MODEL_PORT=str(server.server_port),AIR_MODEL_NAME="local")
    failures=[]
    try:
        for name,cmd in [("pilot","amt-convert"),("facts","amt-facts"),("classification","amt-classify")]:
            s=json.loads((pathlib.Path("knowledge/examples")/f"amt-{name}.json").read_text())
            s["teacher_id"]=args.model+"@"+revision
            if name=="pilot": s["teacher_license"]=license_id
            spec=out/f"{name}-spec.json";spec.write_text(json.dumps(s,indent=2))
            report=out/f"{name}-report.json"; command=["target/release/air-cli",cmd,str(spec),str(report)]
            if name=="facts":command.append(str(out/"facts.db"))
            started=time.monotonic()
            try:
                p=subprocess.run(command,env=env,timeout=900)
                print(name,"exit",p.returncode,"seconds",time.monotonic()-started,flush=True)
                if p.returncode:failures.append(name)
            except subprocess.TimeoutExpired: failures.append(name)
    finally:
        server.shutdown();server.server_close()
    (out/"execution.json").write_text(json.dumps({"failed_tracks":failures,"note":"CPU float32 teacher; public fixtures, no full-system energy measurement"},indent=2))
    if failures:sys.exit(1)
if __name__=="__main__": main()
