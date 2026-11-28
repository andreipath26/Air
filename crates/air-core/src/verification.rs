//! AIR core boundary for independently verifying generated output.
/// Applications must provide an evidence-, test-, or policy-based verifier.
/// A candidate is trusted only when this verifier returns Ok.
pub trait GeneratedOutputVerifier {
    type Error: std::fmt::Display;
    fn verify(&self, question: &str, candidate: &str) -> Result<(), Self::Error>;
}
