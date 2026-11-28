# AIR candidate invariant gate

## Status and scope

`air_core::invariants` adds a dependency-light, bounded Rust source scan and a candidate promotion proof type. Its checks are **static heuristics, not formal verification**. They catch common panic paths and suspicious constructs; they cannot prove semantic correctness, memory safety, absence of all panics, or safe concurrency.

The scan uses the existing tree-sitter Rust grammar and is capped at 256 KiB per candidate. It reports:

- `unwrap()`, `expect()`, panic/abort-style macros, and malformed syntax trees;
- raw arithmetic and compound arithmetic, as potential overflow, divide-by-zero, or shift-boundary risks;
- `RefCell` borrow/borrow_mut calls and unsafe blocks for review;
- a conservative textual warning when `MutexGuard` and `.await` both occur in the candidate.

Raw arithmetic is blocked by default because this scanner has no type analysis or proof that a guard condition makes an operation safe. Prefer checked/saturating operations or have a human review the candidate. Findings can be conservative false positives.

### Known heuristic error modes

| Check | Likely false positive | Likely false negative |
|---|---|---|
| `unwrap` / `expect` methods | A custom type may define a method with one of these names that cannot panic. | A helper or dependency may panic internally; macro expansion is not inspected. |
| Panic-style macros | `assert!` and related macros may be intentional in test-only code. | A panic hidden behind a helper function or an unexpanded procedural macro is not visible in the syntax tree. |
| Raw arithmetic | String concatenation, overloaded operators, or arithmetic guarded by a proven range condition is still flagged. | Overflow implemented inside a called helper, cast, or dependency is not visible at this call site. |
| `RefCell` borrow methods | A custom `borrow` / `borrow_mut` method may be harmless; type resolution is not performed. | Aliasing through helper calls or separate objects may cause a dynamic borrow panic without an obvious local method call. |
| `MutexGuard` plus `.await` | The heuristic scans the whole candidate, so unrelated occurrences can be reported together. | A lock guard may be hidden behind a type alias or helper and still live across suspension. |
| Unsafe blocks | Correctly justified unsafe code is still flagged for review. | Unsafe operations in an unsafe function or external helper may not be reported by the current block-only check. |

Treat a clear report as “none of these patterns were found,” not as a safety proof. Treat each finding as a review prompt; the compiler and tests remain necessary and do not establish that these static heuristics are complete.

## Gate contract

`gate_candidate` returns an opaque `PromotionProof` only when all three caller-supplied verification facts are true:

1. the compiler accepted the candidate;
2. unit tests passed;
3. those checks ran in an enforced sandbox.

It also requires the static scan to report no findings and binds the proof to a SHA-256 digest of the candidate bytes. The boolean sandbox field is a caller attestation; the static scanner does not itself create or verify a sandbox. Applications should bind this gate to their sandbox runner and test harness.

## Distillation boundary

This change deliberately adds a separate `InvariantGatedPromotion` trait rather than editing `OverlayStore::append_verified`. The open AIRW distillation/adversarial-verification work in PR #35 changes that same promotion path. Until the gate is explicitly integrated with that path, this module is an independently testable promotion boundary and does not make the existing public overlay API invariant-gated. Do not claim delta archive promotion is globally protected by this scanner yet.

## Tests

Unit fixtures cover panic-prone paths, unchecked arithmetic, unsafe blocks, dynamic borrow markers, guard/await co-occurrence, malformed syntax, input bounds, clean checked arithmetic, and the requirement for compiler + tests + sandbox attestation.
