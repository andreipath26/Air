#!/usr/bin/env python3
"""Check G02 scoring, fallback accounting, and frozen-input bindings."""
import hashlib
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location("g02_evaluate", ROOT / "scripts/g02-evaluate.py")
gate = importlib.util.module_from_spec(spec)
spec.loader.exec_module(gate)


class EvaluateTests(unittest.TestCase):
    def test_complete_evidence_scores_air_and_hybrid_separately(self):
        cases = []
        for group in ("geography", "science", "engineering", "general_reference"):
            for i in range(50):
                cases.append({"id":f"{group}-{i}","question":f"{group} question {i}",
                              "answer":f"answer-{i}","group":group,"source":"reviewed source"})
        for i in range(40):
            cases.append({"id":f"unknown-{i}","question":f"unknown question {i}",
                          "answer":None,"group":"unknown","source":"reviewed source"})
        raw = json.dumps(cases).encode()
        digest = hashlib.sha256(raw).hexdigest()
        hashes = {"corpus":"c"*64,"dataset":digest,"router":"r"*64}
        observations = {}
        with tempfile.TemporaryDirectory(prefix="air-g02-evaluate-") as tmp:
            root = Path(tmp)
            (root / "manifest.json").write_text(json.dumps({"inputs":hashes}),encoding="utf-8")
            for name, energy, ram, seconds in (("air",20,500,5),("quantized_teacher",200,2000,20),("hybrid",100,1000,10)):
                answers = {c["id"]: (None if c["group"]=="unknown" else c["answer"]) for c in cases}
                identity = {"binary_sha256":"b","database_sha256":"d","policy_sha256":"p","router_sha256":"r"}
                if name != "air":
                    identity.update({"model_sha256":"m","server_sha256":"s"})
                obs = {"system":name,"dataset_sha256":digest,"answers":answers,"errors":{},"identity":identity,
                       "wall_seconds":seconds,"resource_diagnostics":{"cpu_package_energy_joules":energy,
                       "system_used_memory_peak_bytes":ram}}
                if name == "hybrid":
                    obs.update({"direct_air_answers":200,"model_fallback_answers":40})
                    rows = [{"id":c["id"],"source":"model_fallback" if c["group"]=="unknown" else "air"} for c in cases]
                    (root / "hybrid").mkdir()
                    (root / "hybrid/responses.jsonl").write_text("\n".join(json.dumps(x) for x in rows),encoding="utf-8")
                (root / name).mkdir(exist_ok=True)
                (root / name / "observations.json").write_text(json.dumps(obs),encoding="utf-8")
                observations[name]=obs
            policy = json.loads((ROOT / "research/evidence-gate-02.json").read_text())
            review={"input_hashes":hashes,"status":"review_assertions_recorded"}
            hardware=policy["hardware"]
            conversion={"seconds":1,"energy_joules":1,"storage_bytes":100}
            result=gate.evaluate(raw,cases,root,review,hardware,conversion,policy)
            self.assertEqual(result["air_replacement_result"],"meets_numeric_thresholds")
            self.assertEqual(result["hybrid_result"],"meets_numeric_thresholds")
            self.assertEqual(result["metrics"]["hybrid"]["model_fallback_answers"],40)
            self.assertEqual(result["gate_status"],"requires_independent_human_review")
            rows[0]["source"] = "unclassified"
            (root / "hybrid/responses.jsonl").write_text("\n".join(json.dumps(x) for x in rows),encoding="utf-8")
            with self.assertRaises(ValueError):
                gate.evaluate(raw,cases,root,review,hardware,conversion,policy)


if __name__ == "__main__":
    unittest.main()
