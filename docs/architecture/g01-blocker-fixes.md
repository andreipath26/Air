# G01 blocker fixes

Contextual extraction now accepts anchored subject/relation statements and parenthesized answers without looking at the expected answer. The original reference comparison remains mandatory. Multi-teacher reconciliation combines complete distinct traces and records all candidates, admitting only independently reference-matched answers. Teacher identity declarations are not authenticated. Reconciled corpus can be imported with g01-corpus.py --mode reference; this is a reference-validated union of teacher results, not unconstrained knowledge discovery.

Indexed SQLite schema advances to version 2: notation relations (chemical symbol, SI unit symbol, case sensitive identifier) preserve case; other relations retain legacy normalization. Version 1 databases are rejected rather than silently reinterpreted; reimport from original source and repeat reviews. Old policy reviews remain stored but are not used by v2 assessment. Other fact storage backends and object aliases still need typed identity work. No lossless migration of already-collapsed values is claimed.

Remaining blockers: actual multi-teacher experiment, independently authored and reviewed dataset, pinned quantized run, target hardware and whole-system power. No gate passed.
