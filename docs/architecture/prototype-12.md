# Prototype 12 — discovery, predictive warming and residual inference

The reference-driven discovery engine records raw answers, explicit extraction status, wrong answers, abstentions, per-domain coverage and timing. Its public 18-probe fixture spans geography, mathematics, automotive, science, language and unanswerable questions. Exact matching is conservative and can reject valid synonyms. This is not automated discovery of all latent model capabilities.

The real-model structural experiment additionally compares masking 16 high-magnitude hidden coordinates at the last token against a fixed, disjoint size-matched control at the middle layer. This is exploratory coordinate ablation, not sparse-autoencoder feature extraction or semantic circuit discovery. It does not establish domain-specific causal mechanisms.

Optional AIR_PREDICTIVE=1 enables resident explicit-category transition recording and bounded category page warming via SQLite. Only existing categories and observed transitions with at least two occurrences are considered; at most two categories are warmed and payload accounting is capped at 256 KiB. The retrieval itself still follows Indexed query bounds; the payload cap is not an allocation cap for the temporary query. Synchronous warming may worsen latency and is not yet proven beneficial. No speculative answer is trusted.

Optional AIR_RESIDUAL=1 enables local neural answer generation after unresolved routing, labelled unverified and incomplete. This is not a compressed neural residual. Default operation remains without generative fallback. A request may call the model twice (routing and generation); current model_called telemetry is a Boolean per request, not the exact invocation count.

All features are experimental; full model conversion, independent capability preservation, predictive performance benefit, compressed residuals, learned feature extraction and whole-system energy validation remain open.
