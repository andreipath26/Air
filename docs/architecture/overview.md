# Architecture

Teacher -> curator -> representation compiler -> typed store. Runtime: request -> deterministic fast path or SLM semantic router -> Graphmaster -> bounded retrieval -> reasoning/skill execution -> verification -> language realisation. Unknown or uncertain tasks escalate, abstain or request missing context. Categories are many-to-many references to canonical units. Keep provenance, exceptions, time and uncertainty separate from identity. AIR-IR must not be locked to a particular graph or neural backend. Executable records are untrusted until sandboxed and verified. See ROADMAP.md phases 03–20. No subsystem is yet implemented beyond bootstrap contracts.


## Model conversion extension
The proposed [AIR Model Transcompiler](model-transcompiler.md) introduces two acquisition tracks: internal checkpoint/activation analysis and behavioural teacher extraction. Both feed the existing curator and representation compiler; independently validated capabilities are packaged with neural residuals and explicit fallback. See P23 in the roadmap. This is planned, not a completed converter.
