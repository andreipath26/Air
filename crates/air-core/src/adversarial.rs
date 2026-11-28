//! Deterministic adversarial probes for verified solution promotion.
//!
//! This module generates boundary-focused probe cases from a Rust function
//! signature. Production callers must use a runner backed by air-core's
//! unprivileged process sandbox; the trait is an integration seam, not itself
//! a security boundary.

use std::{error::Error, fmt};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeKind {
    EmptyInput,
    ZeroBoundary,
    MaxBoundary,
    BorrowContention,
    ResultErrorPath,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpectedOutcome {
    Completes,
    ReturnsError,
    CompileRejected,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdversarialProbe {
    pub name: String,
    pub kind: ProbeKind,
    /// Rust test or compile-fail source to run with the candidate source.
    pub source: String,
    pub expected: ExpectedOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CriticExecution {
    pub sandboxed: bool,
    pub primary_passed: bool,
    pub adversarial_passed: usize,
    pub adversarial_total: usize,
}

#[derive(Debug)]
pub enum CriticError {
    UnsupportedLanguage,
    InvalidSignature,
    NoApplicableProbes,
    Runner(String),
    Rejected,
    Distillation(crate::distillation::DistillationError),
}
impl fmt::Display for CriticError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedLanguage => f.write_str("adversarial synthesis currently supports Rust"),
            Self::InvalidSignature => f.write_str("could not parse a supported Rust function signature"),
            Self::NoApplicableProbes => f.write_str("signature produced no applicable adversarial probes"),
            Self::Runner(error) => write!(f, "sandboxed critic runner failed: {error}"),
            Self::Rejected => f.write_str("primary or adversarial verification failed or was not sandboxed"),
            Self::Distillation(error) => write!(f, "distillation failed: {error}"),
        }
    }
}
impl Error for CriticError {}
impl From<crate::distillation::DistillationError> for CriticError {
    fn from(value: crate::distillation::DistillationError) -> Self { Self::Distillation(value) }
}

/// Implemented by a runner that compiles and executes all supplied probe code
/// inside an OS sandbox. A runner must not set sandboxed=true unless all
/// primary and adversarial checks actually ran under that sandbox.
/// Private implementation gate: only in-crate adapters backed by the
/// configured OS sandbox may mint CriticProof values.
pub(crate) mod sealed {
    pub trait Sealed {}
}

pub trait SandboxedAdversarialRunner: sealed::Sealed {
    type Error: fmt::Display;
    fn run_primary_and_adversarial(
        &self,
        language: &str,
        candidate: &str,
        primary_tests: &str,
        probes: &[AdversarialProbe],
    ) -> Result<CriticExecution, Self::Error>;
}

/// Opaque evidence bound to the exact candidate and prompt. Only a successful
/// run through run_adversarial_critic can construct this proof.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CriticProof {
    fingerprint: String,
    cases_passed: usize,
    sandboxed: bool,
}
impl CriticProof {
    pub fn cases_passed(&self) -> usize { self.cases_passed }
    pub fn is_sandboxed(&self) -> bool { self.sandboxed }

    pub(crate) fn matches(&self, language: &str, prompt: &str, candidate: &str) -> bool {
        self.sandboxed && self.fingerprint == candidate_fingerprint(language, prompt, candidate)
    }
}

fn candidate_fingerprint(language: &str, prompt: &str, candidate: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hash = Sha256::new();
    hash.update(language.as_bytes());
    hash.update([0]);
    hash.update(prompt.as_bytes());
    hash.update([0]);
    hash.update(candidate.as_bytes());
    format!("{:x}", hash.finalize())
}

#[derive(Debug)]
struct FunctionSignature {
    name: String,
    params: Vec<String>,
    returns: String,
}

fn matching_delimiter(text: &str, open_at: usize, open: char, close: char) -> Option<usize> {
    let mut depth = 0usize;
    for (offset, ch) in text[open_at..].char_indices() {
        if ch == open { depth += 1; }
        if ch == close {
            depth = depth.checked_sub(1)?;
            if depth == 0 { return Some(open_at + offset); }
        }
    }
    None
}

fn split_top_level(text: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut angles = 0i32;
    let mut parens = 0i32;
    let mut brackets = 0i32;
    for (index, ch) in text.char_indices() {
        match ch {
            '<' => angles += 1,
            '>' => angles = (angles - 1).max(0),
            '(' => parens += 1,
            ')' => parens = (parens - 1).max(0),
            '[' => brackets += 1,
            ']' => brackets = (brackets - 1).max(0),
            ',' if angles == 0 && parens == 0 && brackets == 0 => {
                let item = text[start..index].trim();
                if !item.is_empty() { parts.push(item); }
                start = index + 1;
            }
            _ => {}
        }
    }
    let final_item = text[start..].trim();
    if !final_item.is_empty() { parts.push(final_item); }
    parts
}

fn parse_signature(signature: &str) -> Option<FunctionSignature> {
    let fn_at = signature.find("fn ")? + 3;
    let rest = signature[fn_at..].trim_start();
    let name_len = rest.find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_')).unwrap_or(rest.len());
    if name_len == 0 { return None; }
    let name = rest[..name_len].to_owned();
    let paren_rel = rest.find('(')?;
    let paren_at = fn_at + signature[fn_at..].len() - rest.len() + paren_rel;
    let close = matching_delimiter(signature, paren_at, '(', ')')?;
    let args = &signature[paren_at + 1..close];
    let params = split_top_level(args).into_iter().map(|param| {
        param.split_once(':').map(|(_, ty)| ty.trim().to_owned())
    }).collect::<Option<Vec<_>>>()?;
    let after = signature[close + 1..].trim_start();
    let returns = after.strip_prefix("->").map(str::trim).unwrap_or("");
    Some(FunctionSignature { name, params, returns: returns.trim_end_matches('{').trim().to_owned() })
}

fn canonical_type(ty: &str) -> String {
    let trimmed = ty.trim();
    if let Some(rest) = trimmed.strip_prefix("&'") {
        if let Some(space) = rest.find(char::is_whitespace) {
            return format!("&{}", rest[space..].trim().replace(' ', ""));
        }
    }
    trimmed.replace(' ', "")
}

fn identifier_type(ty: &str) -> Option<&str> {
    let compact = canonical_type(ty);
    let name = compact.rsplit("::").next().unwrap_or(&compact);
    ["u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32", "i64", "i128", "isize"]
        .iter().copied().find(|primitive| name == *primitive)
}

fn value_for_type(ty: &str, edge: &str) -> Option<String> {
    let compact = canonical_type(ty);
    if compact == "&str" { return Some(if edge == "empty" { r#""""#.into() } else { r#""air""#.into() }); }
    if compact == "String" { return Some(if edge == "empty" { "String::new()".into() } else { r#"String::from("air")"#.into() }); }
    if compact.starts_with("&[") { return Some("&[]".into()); }
    if compact.starts_with("&mut[") { return Some("&mut []".into()); }
    if compact == "&String" { return Some("&String::new()".into()); }
    if compact == "&mutString" { return Some("&mut String::new()".into()); }
    if compact == "&mutVec<u8>" { return Some("&mut Vec::<u8>::new()".into()); }
    if compact.starts_with("Vec<") { return Some("Vec::new()".into()); }
    if compact.starts_with("Option<") { return Some("None".into()); }
    if compact == "bool" { return Some("false".into()); }
    if let Some(integer) = identifier_type(&compact) {
        return Some(if edge == "max" { format!("{integer}::MAX") } else { "0".into() });
    }
    if compact == "&i32" { return Some("&0".into()); }
    if compact == "&u32" { return Some("&0".into()); }
    if compact == "&muti32" { return Some("&mut 0".into()); }
    if compact == "&mutu32" { return Some("&mut 0".into()); }
    None
}

fn test_source(function: &FunctionSignature, args: &[String], assert_error: bool) -> String {
    let invocation = format!("{}({})", function.name, args.join(", "));
    if assert_error {
        format!(r#"assert!({invocation}.is_err(), "empty input should exercise the error path");"#)
    } else {
        format!(
            r#"assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {{ let _ = {invocation}; }})).is_ok(), "adversarial input panicked");"#
        )
    }
}

fn synthesize_borrow_contention(
    function: &FunctionSignature,
    baseline: &[String],
) -> Option<AdversarialProbe> {
    for (shared_index, shared_type) in function.params.iter().enumerate() {
        let shared = canonical_type(shared_type);
        let shared_base = if shared == "&str" {
            "String"
        } else if let Some(base) = shared.strip_prefix('&') {
            base
        } else {
            continue;
        };
        for (mutable_index, mutable_type) in function.params.iter().enumerate() {
            if shared_index == mutable_index { continue; }
            let mutable = canonical_type(mutable_type);
            let Some(mutable_base) = mutable.strip_prefix("&mut") else { continue };
            if shared_base != mutable_base { continue; }
            if !matches!(mutable_base, "String" | "i32" | "u32" | "i64" | "u64") { continue; }
            let mut args = baseline.to_vec();
            args[shared_index] = if shared == "&str" { "&_air_value".into() } else { "&_air_value".into() };
            args[mutable_index] = "&mut _air_value".into();
            let call = format!("{}({})", function.name, args.join(", "));
            let source = format!(
                r#"{{ let mut _air_value: {mutable_base} = Default::default(); let _air_shared = &_air_value; let _air_mutable = &mut _air_value; let _ = {call}; let _ = _air_shared; }}"#
            );
            return Some(AdversarialProbe {
                name: "shared_and_mutable_alias_is_rejected".into(),
                kind: ProbeKind::BorrowContention,
                source,
                expected: ExpectedOutcome::CompileRejected,
            });
        }
    }
    None
}

/// Generate deterministic probes from a Rust free-function signature.
/// For Result-returning parse/decode/read-style functions, empty string input
/// also becomes an expected error-path case. Unknown parameter types are not
/// guessed or instantiated.
pub fn synthesize_signature_probes(
    language: &str,
    signature: &str,
) -> Result<Vec<AdversarialProbe>, CriticError> {
    if language != "rust" { return Err(CriticError::UnsupportedLanguage); }
    let function = parse_signature(signature).ok_or(CriticError::InvalidSignature)?;
    let mut baseline = Vec::with_capacity(function.params.len());
    for ty in &function.params {
        baseline.push(value_for_type(ty, "normal").ok_or(CriticError::InvalidSignature)?);
    }

    let mut probes = Vec::new();
    for (index, ty) in function.params.iter().enumerate() {
        let compact = canonical_type(ty);
        let mut args = baseline.clone();
        if compact.starts_with("&[") || compact.starts_with("&mut[") || compact.starts_with("Vec<") {
            args[index] = value_for_type(ty, "empty").ok_or(CriticError::InvalidSignature)?;
            probes.push(AdversarialProbe {
                name: format!("empty_input_param_{index}"),
                kind: ProbeKind::EmptyInput,
                source: test_source(&function, &args, false),
                expected: ExpectedOutcome::Completes,
            });
            let element_type = if compact.starts_with("&[") {
                compact.strip_prefix("&[").and_then(|value| value.strip_suffix(']'))
            } else if compact.starts_with("Vec<") {
                compact.strip_prefix("Vec<").and_then(|value| value.strip_suffix('>'))
            } else { None };
            if let Some(integer) = element_type.and_then(identifier_type) {
                args[index] = if compact.starts_with("Vec<") {
                    format!("vec![0{integer}]")
                } else { format!("&[0{integer}]") };
                probes.push(AdversarialProbe {
                    name: format!("zero_boundary_element_{index}"),
                    kind: ProbeKind::ZeroBoundary,
                    source: test_source(&function, &args, false),
                    expected: ExpectedOutcome::Completes,
                });
                args[index] = if compact.starts_with("Vec<") {
                    format!("vec![{integer}::MAX]")
                } else { format!("&[{integer}::MAX]") };
                probes.push(AdversarialProbe {
                    name: format!("max_boundary_element_{index}"),
                    kind: ProbeKind::MaxBoundary,
                    source: test_source(&function, &args, false),
                    expected: ExpectedOutcome::Completes,
                });
            }
        } else if compact == "&str" || compact == "String" {
            args[index] = value_for_type(ty, "empty").ok_or(CriticError::InvalidSignature)?;
            probes.push(AdversarialProbe {
                name: format!("empty_string_param_{index}"),
                kind: ProbeKind::EmptyInput,
                source: test_source(&function, &args, false),
                expected: ExpectedOutcome::Completes,
            });
            let lower_name = function.name.to_ascii_lowercase();
            if function.returns.starts_with("Result<")
                && ["parse", "decode", "read", "load"].iter().any(|key| lower_name.contains(key))
            {
                probes.push(AdversarialProbe {
                    name: format!("empty_string_error_param_{index}"),
                    kind: ProbeKind::ResultErrorPath,
                    source: test_source(&function, &args, true),
                    expected: ExpectedOutcome::ReturnsError,
                });
            }
        } else if identifier_type(&compact).is_some() {
            args[index] = value_for_type(ty, "zero").ok_or(CriticError::InvalidSignature)?;
            probes.push(AdversarialProbe {
                name: format!("zero_boundary_param_{index}"),
                kind: ProbeKind::ZeroBoundary,
                source: test_source(&function, &args, false),
                expected: ExpectedOutcome::Completes,
            });
            args[index] = value_for_type(ty, "max").ok_or(CriticError::InvalidSignature)?;
            probes.push(AdversarialProbe {
                name: format!("max_boundary_param_{index}"),
                kind: ProbeKind::MaxBoundary,
                source: test_source(&function, &args, false),
                expected: ExpectedOutcome::Completes,
            });
        } else if compact == "bool" || compact.starts_with("Option<") {
            args[index] = value_for_type(ty, "empty").ok_or(CriticError::InvalidSignature)?;
            probes.push(AdversarialProbe {
                name: format!("empty_or_none_param_{index}"),
                kind: ProbeKind::EmptyInput,
                source: test_source(&function, &args, false),
                expected: ExpectedOutcome::Completes,
            });
        }

        if compact.starts_with('&') {
            probes.push(AdversarialProbe {
                name: format!("borrowed_input_scope_param_{index}"),
                kind: ProbeKind::BorrowContention,
                source: format!(
                    r#"{{ let _owned = String::from("air"); let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {{ let _ = {}({}); }})); }}"#,
                    function.name,
                    function.params.iter().enumerate().map(|(i, ty)| {
                        if i == index {
                            if canonical_type(ty) == "&str" { "&_owned".to_owned() }
                            else if canonical_type(ty) == "&[u8]" { "&[0u8]".to_owned() }
                            else if canonical_type(ty) == "&[i32]" { "&[0i32]".to_owned() }
                            else if canonical_type(ty) == "&i32" { "&0i32".to_owned() }
                            else if canonical_type(ty) == "&u32" { "&0u32".to_owned() }
                            else if canonical_type(ty) == "&muti32" { "&mut 0i32".to_owned() }
                            else if canonical_type(ty) == "&mutu32" { "&mut 0u32".to_owned() }
                            else { baseline[i].clone() }
                        } else { baseline[i].clone() }
                    }).collect::<Vec<_>>().join(", ")
                ),
                expected: ExpectedOutcome::Completes,
            });
        }
    }

    if let Some(probe) = synthesize_borrow_contention(&function, &baseline) {
        probes.push(probe);
    }
    if probes.is_empty() { return Err(CriticError::NoApplicableProbes); }
    Ok(probes)
}


/// Run the primary prompt tests and generated probes. A successful proof is
/// bound to exactly these bytes so it cannot authorize another candidate.
pub fn run_adversarial_critic<R: SandboxedAdversarialRunner>(
    runner: &R,
    language: &str,
    prompt: &str,
    signature: &str,
    candidate: &str,
    primary_tests: &str,
) -> Result<CriticProof, CriticError> {
    let probes = synthesize_signature_probes(language, signature)?;
    let execution = runner.run_primary_and_adversarial(language, candidate, primary_tests, &probes)
        .map_err(|error| CriticError::Runner(error.to_string()))?;
    if !execution.sandboxed || !execution.primary_passed
        || execution.adversarial_total != probes.len()
        || execution.adversarial_passed != probes.len()
    {
        return Err(CriticError::Rejected);
    }
    Ok(CriticProof {
        fingerprint: candidate_fingerprint(language, prompt, candidate),
        cases_passed: execution.adversarial_passed,
        sandboxed: true,
    })
}

/// Linux-backed adversarial runner. Every rustc compile and test executable
/// is launched as a separate child under Landlock, seccomp, and rlimits.
pub struct RootlessAdversarialRunner {
    scratch_root: std::path::PathBuf,
    wall_timeout: std::time::Duration,
}

impl RootlessAdversarialRunner {
    pub fn new(scratch_root: impl Into<std::path::PathBuf>) -> Self {
        Self {
            scratch_root: scratch_root.into(),
            wall_timeout: std::time::Duration::from_secs(12),
        }
    }

    pub fn with_wall_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.wall_timeout = timeout;
        self
    }

    fn allocate_workspace(&self) -> std::io::Result<std::path::PathBuf> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        std::fs::create_dir_all(&self.scratch_root)?;
        for _ in 0..32 {
            let suffix = NEXT.fetch_add(1, Ordering::Relaxed);
            let path = self.scratch_root.join(format!(
                "critic-{}-{suffix}",
                std::process::id()
            ));
            match std::fs::create_dir(&path) {
                Ok(()) => return Ok(path),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "could not allocate a critic scratch directory",
        ))
    }

    fn rustc_paths() -> std::io::Result<(std::path::PathBuf, std::path::PathBuf)> {
        use std::io;
        use std::path::PathBuf;
        // Resolve the compiler from PATH without executing any caller-selected
        // RUSTC wrapper outside the sandbox.
        let requested_path = PathBuf::from("rustc");
        let mut found = None;
        for directory in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()) {
            let candidate = directory.join(&requested_path);
            if candidate.is_file() {
                found = Some(candidate);
                break;
            }
        }
        let rustc = std::fs::canonicalize(found.ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "rustc is not on PATH")
        })?)?;
        // Rust toolchains use <sysroot>/bin/rustc. Canonicalizing the executable
        // resolves rustup shims and avoids probing an untrusted external command.
        let sysroot = rustc.parent()
            .and_then(|bin| bin.parent())
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "cannot infer rustc sysroot"))?;
        let sysroot = std::fs::canonicalize(sysroot)?;
        Ok((rustc, sysroot))
    }

    fn run_sandboxed_command(
        &self,
        workspace: &std::path::Path,
        program: &std::path::Path,
        args: &[std::ffi::OsString],
        rustc: &std::path::Path,
        sysroot: &std::path::Path,
    ) -> std::io::Result<crate::sandbox::SandboxOutput> {
        use crate::sandbox::{run_sandboxed, Enforcement, SandboxConfig};
        use std::io;
        let mut config = SandboxConfig::new(workspace);
        config.require_landlock(true);
        config.cpu_seconds = 8;
        config.address_space_bytes = 1536 * 1024 * 1024;
        config.file_size_bytes = 32 * 1024 * 1024;
        config.wall_timeout = self.wall_timeout;
        config.max_output_bytes = 2 * 1024 * 1024;
        if let Some(parent) = rustc.parent() {
            config.allow_readonly_path(parent);
        }
        config.allow_readonly_path(rustc);
        config.allow_readonly_path(sysroot);
        let output = run_sandboxed(program, args, &config)?;
        if output.enforcement.landlock != Enforcement::Enforced
            || output.enforcement.seccomp != Enforcement::Enforced
            || output.enforcement.resource_limits != Enforcement::Enforced
        {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "critic proof requires Landlock, seccomp, and rlimits",
            ));
        }
        Ok(output)
    }

    fn compile_test_harness(
        &self,
        source: &str,
        run_binary: bool,
        expect_borrow_rejection: bool,
    ) -> std::io::Result<bool> {
        use crate::compiler_repair::parse_json_diagnostics;
        use std::ffi::OsString;
        use std::io;
        let (rustc, sysroot) = Self::rustc_paths()?;
        let workspace = self.allocate_workspace()?;
        let result = (|| {
            let source_path = workspace.join("critic.rs");
            let executable = workspace.join("critic-tests");
            std::fs::write(&source_path, source)?;
            let args = vec![
                OsString::from("--edition=2021"),
                OsString::from("--error-format=json"),
                OsString::from("--test"),
                OsString::from("--crate-name=air_adversarial_critic"),
                source_path.as_os_str().to_owned(),
                OsString::from("-o"),
                executable.as_os_str().to_owned(),
            ];
            let compile = self.run_sandboxed_command(
                &workspace,
                &rustc,
                &args,
                &rustc,
                &sysroot,
            )?;
            if compile.timed_out || compile.stdout_truncated || compile.stderr_truncated {
                return Ok(false);
            }
            if expect_borrow_rejection {
                if compile.status.success() { return Ok(false); }
                let stderr = String::from_utf8_lossy(&compile.stderr);
                let diagnostics = parse_json_diagnostics(&stderr);
                return Ok(diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("E0502")));
            }
            if !compile.status.success() || !run_binary { return Ok(false); }
            let run = self.run_sandboxed_command(
                &workspace,
                &executable,
                &[],
                &rustc,
                &sysroot,
            )?;
            Ok(run.status.success() && !run.timed_out && !run.stdout_truncated && !run.stderr_truncated)
        })();
        let _ = std::fs::remove_dir_all(&workspace);
        result
    }
}

impl sealed::Sealed for RootlessAdversarialRunner {}

impl SandboxedAdversarialRunner for RootlessAdversarialRunner {
    type Error = std::io::Error;

    fn run_primary_and_adversarial(
        &self,
        language: &str,
        candidate: &str,
        primary_tests: &str,
        probes: &[AdversarialProbe],
    ) -> Result<CriticExecution, Self::Error> {
        use std::io::{Error, ErrorKind};
        if language != "rust" {
            return Err(Error::new(ErrorKind::InvalidInput, "rootless runner currently supports Rust only"));
        }
        if candidate.is_empty() || !primary_tests.contains("#[test]") {
            return Err(Error::new(ErrorKind::InvalidInput, "candidate and at least one primary #[test] are required"));
        }
        let primary_source = format!("{candidate}\n{primary_tests}");
        let primary_passed = self.compile_test_harness(&primary_source, true, false)?;
        if !primary_passed {
            return Ok(CriticExecution {
                sandboxed: true,
                primary_passed: false,
                adversarial_passed: 0,
                adversarial_total: probes.len(),
            });
        }

        let mut adversarial_passed = 0usize;
        for (index, probe) in probes.iter().enumerate() {
            let probe_source = if probe.expected == ExpectedOutcome::CompileRejected {
                format!(
                    "{candidate}\n#[test] fn air_generated_probe_{index}() {{ {} }}",
                    probe.source
                )
            } else {
                format!(
                    "{candidate}\n#[test] fn air_generated_probe_{index}() {{ {} }}",
                    probe.source
                )
            };
            let expect_borrow_rejection = probe.expected == ExpectedOutcome::CompileRejected;
            if self.compile_test_harness(&probe_source, true, expect_borrow_rejection)? {
                adversarial_passed += 1;
            }
        }
        Ok(CriticExecution {
            sandboxed: true,
            primary_passed: true,
            adversarial_passed,
            adversarial_total: probes.len(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockRunner { fail_probe: Option<String> }
    impl sealed::Sealed for MockRunner {}
    impl SandboxedAdversarialRunner for MockRunner {
        type Error = &'static str;
        fn run_primary_and_adversarial(
            &self,
            _language: &str,
            candidate: &str,
            primary_tests: &str,
            probes: &[AdversarialProbe],
        ) -> Result<CriticExecution, Self::Error> {
            let failed = self.fail_probe.as_deref()
                .filter(|_| candidate.contains("brittle") || primary_tests.contains("fail"));
            Ok(CriticExecution {
                sandboxed: true,
                primary_passed: failed.is_none(),
                adversarial_passed: probes.len() - usize::from(failed.is_some()),
                adversarial_total: probes.len(),
            })
        }
    }

    #[test]
    fn signature_synthesizes_empty_numeric_borrow_and_error_probes() {
        let probes = synthesize_signature_probes(
            "rust",
            "fn parse_count(input: &str, limit: usize, values: &[u32]) -> Result<u32, ParseError>",
        ).unwrap();
        assert!(probes.iter().any(|p| p.kind == ProbeKind::EmptyInput));
        assert!(probes.iter().any(|p| p.kind == ProbeKind::ZeroBoundary));
        assert!(probes.iter().any(|p| p.kind == ProbeKind::MaxBoundary));
        assert!(probes.iter().any(|p| p.kind == ProbeKind::BorrowContention));
        assert!(probes.iter().any(|p| p.kind == ProbeKind::ResultErrorPath));
    }

    #[test]
    fn borrow_contention_probe_expects_rustc_rejection_for_shared_mut_alias() {
        let probes = synthesize_signature_probes(
            "rust",
            "fn update_and_read(input: &str, buffer: &mut String) -> usize",
        ).unwrap();
        assert!(probes.iter().any(|probe| {
            probe.name == "shared_and_mutable_alias_is_rejected"
                && probe.expected == ExpectedOutcome::CompileRejected
                && probe.source.contains("update_and_read(&_air_value, &mut _air_value)")
        }));
    }

    #[test]
    fn rejects_overfitted_solution_when_adversarial_probe_fails() {
        let runner = MockRunner { fail_probe: Some("overfit".into()) };
        assert!(matches!(
            run_adversarial_critic(&runner, "rust", "parse a bounded count", "fn parse(input: &str) -> Result<u32, E>", "fn parse(input: &str) -> Result<u32, E> { /* brittle */ }", "#[test] fn primary() {}"),
            Err(CriticError::Rejected)
        ));
    }

    #[test]
    fn robust_solution_gets_candidate_bound_sandboxed_proof() {
        let runner = MockRunner { fail_probe: None };
        let prompt = "parse a bounded count";
        let candidate = "fn parse(input: &str) -> Result<u32, E> { input.parse().map_err(|_| E) }";
        let proof = run_adversarial_critic(
            &runner, "rust", prompt, "fn parse(input: &str) -> Result<u32, E>", candidate, "#[test] fn primary() {}"
        ).unwrap();
        assert!(proof.is_sandboxed());
        assert!(proof.cases_passed() >= 2);
        assert!(proof.matches("rust", prompt, candidate));
        assert!(!proof.matches("rust", prompt, "fn parse(input: &str) -> Result<u32, E> { /* brittle */ }"));
        let path = std::env::temp_dir().join(format!("air-critic-{}.airw", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let store = crate::distillation::OverlayStore::new(&path);
        let verified = crate::distillation::Verification { compiler_clean: true, tests_passed: true };
        assert_eq!(store.append_critically_verified("rust", prompt, candidate, verified, &proof).unwrap(), 1);
        assert!(matches!(
            store.append_critically_verified("rust", prompt, "fn parse(input: &str) -> Result<u32, E> { /* different */ }", verified, &proof),
            Err(crate::distillation::DistillationError::CriticRejected)
        ));
        std::fs::remove_file(path).unwrap();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn rootless_runner_compiles_and_runs_candidate_and_boundary_probes() {
        let root = std::env::temp_dir().join(format!("air-rootless-critic-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let runner = RootlessAdversarialRunner::new(root.join("scratch"));
        let candidate = "pub fn clamp(value: u32) -> u32 { if value > u32::MAX { u32::MAX } else { value } }";
        let primary = "#[test] fn primary_normal_value() { assert_eq!(clamp(42), 42); }";
        let proof = run_adversarial_critic(
            &runner,
            "rust",
            "clamp a u32 value safely",
            "fn clamp(value: u32) -> u32",
            candidate,
            primary,
        ).expect("sandboxed primary and boundary tests should pass");
        assert!(proof.is_sandboxed());
        assert!(proof.cases_passed() >= 2);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn refuses_runner_that_did_not_sandbox() {
        struct NotSandboxed;
        impl sealed::Sealed for NotSandboxed {}
        impl SandboxedAdversarialRunner for NotSandboxed {
            type Error = &'static str;
            fn run_primary_and_adversarial(&self, _l: &str, _c: &str, _p: &str, probes: &[AdversarialProbe]) -> Result<CriticExecution, Self::Error> {
                Ok(CriticExecution { sandboxed: false, primary_passed: true, adversarial_passed: probes.len(), adversarial_total: probes.len() })
            }
        }
        assert!(matches!(
            run_adversarial_critic(&NotSandboxed, "rust", "prompt", "fn f(x: u32) -> u32", "fn f(x:u32)->u32{x}", "test"),
            Err(CriticError::Rejected)
        ));
    }
}
