"""Reconcile complete teacher artifacts from the same matrix run and benchmark their union."""
import argparse,json,hashlib,subprocess,sys
from pathlib import Path
def main():
    p=argparse.ArgumentParser();p.add_argument("artifacts");p.add_argument("output");a=p.parse_args()
    root=Path(a.artifacts);out=Path(a.output);out.mkdir(parents=True,exist_ok=False)
    corpus=Path("knowledge/development/g01-corpus.json");dataset=Path("knowledge/development/g01-dataset.json");router=Path("knowledge/development/g01-router.json")
    digest=hashlib.sha256(corpus.read_bytes()).hexdigest()
    args=[];identities=[];individual={}
    for identity in sorted(root.glob("*/identity.json")):
        ident=json.loads(identity.read_text())
        name=ident["model"]+"@"+ident["revision"]
        if name in identities:raise ValueError("duplicate teacher")
        store=identity.parent/"g01-development"/"teacher-store"
        summary=json.loads((store/"conversion-summary.json").read_text())
        if summary["corpus_sha256"]!=digest or summary["teacher_id"]!=name:raise ValueError("teacher or corpus mismatch")
        weights_hash=hashlib.sha256(json.dumps(ident["weights"],sort_keys=True).encode()).hexdigest()
        if summary["declared_teacher_model_sha256"]!=weights_hash:raise ValueError("weight identity mismatch")
        identities.append(name);individual[name]=summary["accepted"];args.extend(["--trace",name,str(store/"conversion.jsonl")])
    if len(identities)!=2:raise ValueError("expected two complete matrix artifacts")
    def run(script,*args):subprocess.run([sys.executable,"scripts/"+script,*map(str,args)],check=True)
    run("g01-reconcile.py",corpus,out/"reconciliation",*args)
    run("g01-corpus.py",out/"reconciliation/matched-corpus.json",out/"store","--mode","reference")
    run("g01-benchmark.py",dataset,out/"benchmark","--system","air","--db",out/"store/knowledge.db","--policy",out/"store/policy.db","--router",router)
    rec=json.loads((out/"reconciliation/reconciliation.json").read_text())
    (out/"summary.json").write_text(json.dumps({"individual_accepted":individual,"union_accepted":rec["matched"],"total":rec["total"],"gate_eligible":False,"note":"Reference-validated union of two draft teacher traces, not independent acceptance"},indent=2))
    print((out/"summary.json").read_text())
if __name__=="__main__":main()
