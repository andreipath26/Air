# G01 — factual capability transfer

Evidence source: GitHub Actions run 45974698319, artifacts 15186138697 (0.5B) and 15186458003 (1.5B). The raw responses and discovery reports now exist. Both execution reports have no failed tracks. This is workflow success, not scientific acceptance.

| Metric | 0.5B | 1.5B |
|---|---:|---:|
| Extracted/retrieved facts | 3/6 | 6/6 |
| Teacher paraphrase accuracy | 5/6 | 6/6 |
| Teacher classification | 2/6 | 6/6 |
| Weightless classification | 3/6 | 5/6 |
| Teacher-fed lexical baseline | 3/6 | 6/6 |
| Reference-fed lexical baseline | 6/6 | 6/6 |
| Arithmetic teacher test | 4/12 | 3/12 |
| Compiled arithmetic | no candidate | no candidate |
| Discovery exact/accepted results | 9/18 | 16/18 |
| Coordinate intervention top-token change | 5/6 | 4/6 |
| Fixed disjoint control top-token change | 0/6 | 0/6 |

Factual retrieval was canonical-key lookup, not a held-out end-to-end paraphrase benchmark. The reference-fed lexical baseline outperformed weightless classification. Structural controls are not magnitude-matched and cannot establish semantic feature identity. Reported teacher timings include generation and transport; no quality-adjusted whole-system resource comparison is available.

G01 is a pre-registered proposed gate, not a passed result. Its machine-readable policy defines minimum workload sizes, absolute/relative quality, group safeguards, abstention and resource targets. The evaluator checks submitted evidence shape and thresholds; it cannot authenticate hardware, independence, measurements or reviewer assertions. Independent review remains required even when numerical checks pass. No automatic deployment approval.

The immediate next tasks are independently authoring and freezing the workload, pinning the quantized llama.cpp baseline, implementing end-to-end paraphrase routing and collecting measurements on the mandatory laptop. Direct reference-fed retrieval must be included to test the incremental value of pretrained-model transfer rather than merely the advantage of a database.
