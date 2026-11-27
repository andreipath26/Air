"""Resolve official upstream identities once; publish exact pins before download/build."""
import argparse,json,re,subprocess,urllib.request
from pathlib import Path
MODEL="Qwen/Qwen2.5-1.5B-Instruct-GGUF"
FILE="qwen2.5-1.5b-instruct-q4_k_m.gguf"
def resolve():
    url="https://huggingface.co/api/models/"+MODEL+"?blobs=true"
    with urllib.request.urlopen(url,timeout=30) as r:
        raw=r.read(2*1024*1024+1)
    if len(raw)>2*1024*1024:raise ValueError("metadata limit")
    info=json.loads(raw)
    rev=info["sha"]
    files=[x for x in info["siblings"] if x["rfilename"]==FILE]
    if len(files)!=1:raise ValueError("checkpoint file absent")
    digest=files[0]["lfs"]["sha256"]
    if info.get("cardData",{}).get("license")!="apache-2.0":raise ValueError("checkpoint license not approved")
    llama=subprocess.check_output(["git","ls-remote","https://github.com/ggml-org/llama.cpp.git","HEAD"],text=True,timeout=60).split()[0]
    if not all(re.fullmatch("[0-9a-f]{40}",x) for x in (rev,llama)) or not re.fullmatch("[0-9a-f]{64}",digest):raise ValueError("invalid upstream pins")
    return {"model":MODEL,"file":FILE,"model_revision":rev,"model_sha256":digest,"llama_revision":llama,"license":"apache-2.0","scope":"development resolution, reuse these exact pins for reproduction"}
if __name__=="__main__":
    p=argparse.ArgumentParser();p.add_argument("output");a=p.parse_args()
    with open(a.output,"x") as f:json.dump(resolve(),f,indent=2)
