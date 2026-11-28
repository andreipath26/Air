//! Deterministic multi-modal binding for coding-task routing.
//!
//! This uses the existing 10,000-bit binary hypervector representation and
//! XOR binding; it adds no learned parameters.

use crate::hdc::HyperVector;

/// Bind prompt-token, AST-tag, and rustc-diagnostic signatures:
/// H_total = H_tokens XOR permute(H_ast, 1) XOR permute(H_diag, 2).
pub fn encode_multiscale(
    prompt_tokens: &str,
    ast_tags: &[&str],
    diagnostic_codes: &[&str],
) -> HyperVector {
    let ast_text = ast_tags.join(" ");
    let diagnostic_text = diagnostic_codes.join(" ");
    let tokens = HyperVector::encode_text(prompt_tokens);
    let ast = HyperVector::encode_text(&ast_text).permute(1);
    let diagnostics = HyperVector::encode_text(&diagnostic_text).permute(2);
    tokens.bind(&ast).bind(&diagnostics)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hdc::{HYPERVECTOR_BITS, HYPERVECTOR_WORDS};

    #[test]
    fn tri_modal_encoding_is_the_exact_xor_permutation_formula() {
        let prompt = "repair a moved Rust value";
        let ast_tags = ["function", "borrow", "argument"];
        let diagnostics = ["E0382", "moved-value"];
        let expected = HyperVector::encode_text(prompt)
            .bind(&HyperVector::encode_text(&ast_tags.join(" ")).permute(1))
            .bind(&HyperVector::encode_text(&diagnostics.join(" ")).permute(2));

        let actual = encode_multiscale(prompt, &ast_tags, &diagnostics);
        assert_eq!(actual, expected);
        assert_eq!(HYPERVECTOR_BITS, 10_000);
        assert_eq!(HYPERVECTOR_WORDS, 157);
        assert_eq!(actual.0[156] >> 16, 0);
    }

    #[test]
    fn ast_and_diagnostic_modalities_change_the_signature_independently() {
        let tokens_only = encode_multiscale("repair borrow", &[], &[]);
        let with_ast = encode_multiscale("repair borrow", &["function", "borrow"], &[]);
        let with_diag = encode_multiscale("repair borrow", &[], &["E0502"]);
        assert_ne!(tokens_only, with_ast);
        assert_ne!(tokens_only, with_diag);
        assert_ne!(with_ast, with_diag);
    }

    #[test]
    fn encoding_is_deterministic_for_same_modalities() {
        assert_eq!(
            encode_multiscale("impl Trait", &["trait", "impl"], &["E0277"]),
            encode_multiscale("impl Trait", &["trait", "impl"], &["E0277"])
        );
    }
}
