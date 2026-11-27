#!/usr/bin/env python3
"""Score AIR-only and AIR-first hybrid runs against the pinned model baseline."""
import argparse
import hashlib
import importlib.util
import json
import math
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location("answer_contract", ROOT / "scripts/g01-answer.py")
answer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(answer)


def read(path):
    raw = Path(path).read_bytes()
    if len(raw) > 16 * 1024 * 1024:
        raise ValueError("input exceeds size limit")
    return raw, json.loads(raw)


def positive(value):
    return type(value) in (int, float) and math.isfinite(value) and value > 0


def evaluate(dataset_raw, cases, results_dir, review, hardware, conversion, policy):
    dataset_sha = hashlib.sha256(dataset_raw).hexdigest()
    if not isinstance(cases, list) or not cases:
        raise ValueError("empty or invalid workload")
    ids = [case["id"] for case in cases]
    if len(set(ids)) != len(ids):
        raise ValueError("duplicate workload IDs")
    for case in cases:
        if case.get("group") == "unknown":
            if case.get("answer") is not None:
                raise ValueError("unknown cases must have null references")
        elif not isinstance(case.get("answer"), str) or not case["answer"].strip():
            raise ValueError("known cases need reference answers")

    result_root = Path(results_dir)
    _, manifest = read(result_root / "manifest.json")
    expected_hashes = manifest.get("inputs", {})
    if expected_hashes.get("dataset") != dataset_sha:
        raise ValueError("run manifest does not bind the scored workload")
    observations = {}
    for system in ("air", "quantized_teacher", "hybrid"):
        _, obs = read(result_root / system / "observations.json")
        if obs.get("system") != system or obs.get("dataset_sha256") != dataset_sha:
            raise ValueError(f"{system}: dataset or system identity mismatch")
        if set(obs.get("answers", {})) != set(ids) or not isinstance(obs.get("errors", {}), dict):
            raise ValueError(f"{system}: incomplete observations")
        observations[system] = obs

    checks = {}
    metrics = {}
    workload_policy = policy["workload"]
    groups = sorted({case["group"] for case in cases})
    per_group = {group: [case for case in cases if case["group"] == group] for group in groups}
    checks["workload_minimum_size"] = len(cases) >= workload_policy["minimum_cases"]
    checks["workload_domain_counts"] = all(
        len(per_group.get(group, [])) >= workload_policy["minimum_cases_per_domain"]
        for group in workload_policy["groups"])
    checks["workload_unknown_count"] = len(per_group.get("unknown", [])) >= workload_policy["minimum_unknown_cases"]
    for name, obs in observations.items():
        scores = {case["id"]: answer.matches(obs["answers"][case["id"]], case["answer"], case["group"])
                  for case in cases}
        correct = sum(scores.values())
        diagnostics = obs.get("resource_diagnostics") or {}
        energy = diagnostics.get("cpu_package_energy_joules")
        ram = diagnostics.get("system_used_memory_peak_bytes")
        elapsed = obs.get("wall_seconds")
        metrics[name] = {
            "correct": correct,
            "total": len(cases),
            "recorded_errors": len(obs.get("errors", {})),
            "accuracy_percent": 100 * correct / len(cases),
            "correct_answers_per_second": correct / elapsed if positive(elapsed) else None,
            "peak_whole_system_used_memory_bytes": ram,
            "cpu_package_energy_joules": energy,
            "cpu_package_joules_per_correct": energy / correct if positive(energy) and correct else None,
            "group_accuracy_percent": {
                group: 100 * sum(scores[c["id"]] for c in members) / len(members)
                for group, members in per_group.items()
            },
        }
        identity = obs.get("identity", {})
        if name == "air":
            metrics[name]["runtime_and_store_bytes"] = sum(
                identity.get(key, 0) for key in ("binary_size_bytes", "database_size_bytes", "policy_size_bytes", "router_size_bytes"))
        elif name == "quantized_teacher":
            metrics[name]["model_and_server_bytes"] = sum(
                identity.get(key, 0) for key in ("model_size_bytes", "server_size_bytes"))
        else:
            metrics[name]["runtime_store_and_model_bytes"] = sum(
                identity.get(key, 0) for key in ("binary_size_bytes", "database_size_bytes", "policy_size_bytes",
                                                   "router_size_bytes", "model_size_bytes", "server_size_bytes"))

    air = metrics["air"]
    baseline = metrics["quantized_teacher"]
    hybrid = metrics["hybrid"]
    thresholds = policy["thresholds"]
    conversion_energy = conversion.get("energy_joules")
    for name in ("air", "hybrid"):
        measured = metrics[name]["cpu_package_energy_joules"]
        correct = metrics[name]["correct"]
        if positive(measured) and positive(conversion_energy) and correct:
            life_energy = measured + conversion_energy
            metrics[name]["lifecycle_cpu_package_joules_per_correct"] = life_energy / correct
        else:
            metrics[name]["lifecycle_cpu_package_joules_per_correct"] = None
    checks["air_model_absent"] = not any(k in observations["air"].get("identity", {})
                                          for k in ("model_sha256", "teacher_id", "fallback_model"))
    checks["same_pinned_model_for_baseline_and_fallback"] = all(
        observations["quantized_teacher"].get("identity", {}).get(k) ==
        observations["hybrid"].get("identity", {}).get(k)
        for k in ("model_sha256", "server_sha256"))
    checks["same_air_store_and_router"] = all(
        observations["air"].get("identity", {}).get(k) == observations["hybrid"].get("identity", {}).get(k)
        for k in ("binary_sha256", "database_sha256", "policy_sha256", "router_sha256"))
    checks["air_absolute_accuracy"] = air["accuracy_percent"] >= thresholds["air_only_absolute_accuracy_percent"]
    checks["air_relative_accuracy"] = baseline["correct"] > 0 and \
        air["correct"] * 100 >= baseline["correct"] * thresholds["air_only_relative_to_pinned_model_percent"]
    checks["air_domain_accuracy"] = all(
        air["group_accuracy_percent"].get(group, 0) >= thresholds["minimum_per_domain_accuracy_percent"]
        for group in policy["workload"]["groups"])
    checks["air_unknown_abstention"] = air["group_accuracy_percent"].get("unknown", 0) >= \
        thresholds["unknown_abstention_percent"]
    checks["air_throughput"] = positive(baseline["correct_answers_per_second"]) and \
        positive(air["correct_answers_per_second"]) and \
        air["correct_answers_per_second"] >= baseline["correct_answers_per_second"] * thresholds["correct_answer_throughput_ratio"]
    checks["air_ram_reduction"] = positive(air["peak_whole_system_used_memory_bytes"]) and \
        positive(baseline["peak_whole_system_used_memory_bytes"]) and \
        air["peak_whole_system_used_memory_bytes"] <= baseline["peak_whole_system_used_memory_bytes"] * \
        (1 - thresholds["whole_system_peak_ram_reduction_percent"] / 100)
    checks["air_energy_reduction"] = positive(air["lifecycle_cpu_package_joules_per_correct"]) and \
        positive(baseline["cpu_package_joules_per_correct"]) and \
        air["lifecycle_cpu_package_joules_per_correct"] <= baseline["cpu_package_joules_per_correct"] * \
        (1 - thresholds["energy_per_correct_reduction_percent"] / 100)

    hybrid_obs = observations["hybrid"]
    rows = [json.loads(line) for line in (result_root / "hybrid" / "responses.jsonl").read_text(encoding="utf-8").splitlines()]
    row_ids = [row.get("id") for row in rows]
    if set(row_ids) != set(ids) or len(row_ids) != len(ids):
        raise ValueError("hybrid trace does not account for every test case exactly once")
    direct = sum(row.get("source") == "air" for row in rows)
    fallback = sum(row.get("source") == "model_fallback" for row in rows)
    if direct + fallback != len(cases):
        raise ValueError("hybrid trace contains hidden or unclassified requests")
    metrics["hybrid"]["air_direct_answers"] = direct
    metrics["hybrid"]["model_fallback_answers"] = fallback
    metrics["hybrid"]["air_direct_answer_rate"] = direct / len(cases)
    metrics["hybrid"]["fallback_rate"] = fallback / len(cases)
    checks["hybrid_fallbacks_fully_accounted"] = hybrid_obs.get("model_fallback_answers") == fallback
    checks["hybrid_quality_within_tolerance"] = baseline["correct"] > 0 and \
        hybrid["accuracy_percent"] >= baseline["accuracy_percent"] - thresholds["hybrid_quality_tolerance_percentage_points"]
    checks["hybrid_energy_below_baseline"] = positive(hybrid["lifecycle_cpu_package_joules_per_correct"]) and \
        positive(baseline["cpu_package_joules_per_correct"]) and \
        hybrid["lifecycle_cpu_package_joules_per_correct"] < baseline["cpu_package_joules_per_correct"]
    checks["hybrid_ram_not_above_baseline"] = positive(hybrid["peak_whole_system_used_memory_bytes"]) and \
        positive(baseline["peak_whole_system_used_memory_bytes"]) and \
        hybrid["peak_whole_system_used_memory_bytes"] <= baseline["peak_whole_system_used_memory_bytes"]

    review_hashes = review.get("input_hashes", {})
    checks["review_binds_all_run_inputs"] = all(
        review_hashes.get(key) == expected_hashes.get(key) for key in ("corpus", "dataset", "router")) and \
        review.get("status") == "review_assertions_recorded"
    target = policy["hardware"]
    checks["declared_target_hardware"] = all(hardware.get(key) == target.get(key)
        for key in ("model", "cpu", "ram_gib", "gpu"))
    checks["conversion_cost_complete"] = all(positive(conversion.get(key)) for key in ("seconds", "energy_joules")) and \
        type(conversion.get("storage_bytes")) is int and conversion["storage_bytes"] >= 0

    query_energy_saving = None
    if positive(air.get("cpu_package_joules_per_correct")) and positive(baseline.get("cpu_package_joules_per_correct")):
        # Using measured accuracy and joules/query, solve for when one-time conversion energy is paid back.
        air_joules_per_query = air.get("cpu_package_energy_joules", 0) / len(cases)
        net_saving_per_query = baseline["cpu_package_joules_per_correct"] * air["accuracy_percent"] / 100 - air_joules_per_query
        if net_saving_per_query > 0 and positive(conversion.get("energy_joules")):
            query_energy_saving = conversion["energy_joules"] / net_saving_per_query

    air_pass = all(checks[k] for k in ("air_model_absent", "air_absolute_accuracy", "air_relative_accuracy",
        "air_domain_accuracy", "air_unknown_abstention", "air_throughput", "air_ram_reduction", "air_energy_reduction"))
    hybrid_pass = all(checks[k] for k in ("hybrid_fallbacks_fully_accounted", "hybrid_quality_within_tolerance",
        "hybrid_energy_below_baseline", "hybrid_ram_not_above_baseline"))
    return {
        "dataset_sha256": dataset_sha,
        "numeric_checks_passed": all(checks.values()),
        "air_replacement_result": "meets_numeric_thresholds" if air_pass else "not_demonstrated",
        "hybrid_result": "meets_numeric_thresholds" if hybrid_pass else "not_demonstrated",
        "gate_status": "requires_independent_human_review" if all(checks.values()) else "not_passed",
        "checks": checks,
        "metrics": metrics,
        "conversion_energy_break_even_queries": query_energy_saving,
        "limitations": [
            "Human review assertions and hardware declarations are not authenticated by this program.",
            "CPU package energy is not whole-system wall-power energy; record the reported meter boundary.",
            "One factual capability passing does not establish replacement of general-purpose model weights.",
        ],
    }


def main():
    p = argparse.ArgumentParser()
    p.add_argument("dataset")
    p.add_argument("results_dir")
    p.add_argument("review")
    p.add_argument("hardware")
    p.add_argument("conversion")
    p.add_argument("output")
    p.add_argument("--policy", default="research/evidence-gate-02.json")
    a = p.parse_args()
    raw, cases = read(a.dataset)
    _, review = read(a.review)
    _, hardware = read(a.hardware)
    _, conversion = read(a.conversion)
    _, policy = read(a.policy)
    result = evaluate(raw, cases, a.results_dir, review, hardware, conversion, policy)
    with open(a.output, "x", encoding="utf-8") as f:
        json.dump(result, f, indent=2, allow_nan=False)
    print(json.dumps(result, indent=2, allow_nan=False))


if __name__ == "__main__":
    main()
