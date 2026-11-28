//! Deterministic CPU-only Rust coding-pipeline smoke suite.
//!
//! The tests route 12 static Rust snippets through AIR core's typed reducer
//! and caller oracle, then invoke rustc to check the combined snippets.
//! Ordinary tests use neuro-symbolic mode. Setting
//! AIR_PIPELINE_SMOKE_MODE=dual opts into extra routing checks with
//! DeterministicMock; this is not SLM inference or a model benchmark.

use air_core::coding::{
    CodingReducer, CodingRequest, ConceptKind, OracleDecision, RankedCandidate,
    ReductionRule, Resolution, TypedConcept, VerificationOracle,
};
use air_slm::routing::{route_fuzzy, route_prompt, EscalationReason, FuzzyRoute, RouteDecision};
use air_slm::{run_verified, DeterministicMock, GenerationOptions, ThinkingMode};
use air_core::verification::GeneratedOutputVerifier;
use air_weights::{ArchiveBuilder, ArchiveReader};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::Instant;

#[derive(Clone, Copy)]
struct Challenge {
    id: &'static str,
    section: &'static str,
    prompt: &'static str,
    route: FuzzyRoute,
    source: &'static str,
}

const CHALLENGES: [Challenge; 12] = [
    Challenge {
        id: "ownership",
        section: "rust.case.ownership",
        prompt: "borrow a Rust String without moving it",
        route: FuzzyRoute::RustOwnership,
        source: "pub fn move_then_len(s: String) -> usize { let owned = s; owned.len() }",
    },
    Challenge {
        id: "borrowing",
        section: "rust.case.borrowing",
        prompt: "borrow a string slice immutably and return its trimmed length",
        route: FuzzyRoute::RustOwnership,
        source: "pub fn trimmed_len(s: &str) -> usize { s.trim().len() }",
    },
    Challenge {
        id: "lifetime",
        section: "rust.case.lifetime",
        prompt: "add a lifetime to return a borrowed value",
        route: FuzzyRoute::RustLifetime,
        source: "pub fn pick_left<'a>(left: &'a str, _right: &str) -> &'a str { left }",
    },
    Challenge {
        id: "trait",
        section: "rust.case.trait",
        prompt: "implement a trait for a concrete Rust type",
        route: FuzzyRoute::TraitGeneric,
        source: "pub trait Area { fn area(&self) -> u32; } pub struct Tile(pub u32); impl Area for Tile { fn area(&self) -> u32 { self.0 * self.0 } }",
    },
    Challenge {
        id: "generic",
        section: "rust.case.generic",
        prompt: "write a generic function with two type parameters",
        route: FuzzyRoute::TraitGeneric,
        source: "pub fn pair_with<T, U>(left: T, right: U) -> (T, U) { (left, right) }",
    },
    Challenge {
        id: "display",
        section: "rust.case.display",
        prompt: "use a generic Display bound to format a value",
        route: FuzzyRoute::TraitGeneric,
        source: "pub fn show<T: std::fmt::Display>(value: T) -> String { value.to_string() }",
    },
    Challenge {
        id: "async",
        section: "rust.case.async",
        prompt: "write an async function that returns a value",
        route: FuzzyRoute::AsyncConcurrency,
        source: "pub async fn answer_async() -> u8 { 42 }",
    },
    Challenge {
        id: "concurrency",
        section: "rust.case.concurrency",
        prompt: "spawn concurrent scoped threads and join their results",
        route: FuzzyRoute::AsyncConcurrency,
        source: "pub fn threaded_sum(a: u32, b: u32) -> u32 { std::thread::scope(|scope| { let left = scope.spawn(move || a); let right = scope.spawn(move || b); left.join().unwrap() + right.join().unwrap() }) }",
    },
    Challenge {
        id: "result",
        section: "rust.case.result",
        prompt: "propagate a parse error with Result and the question mark operator",
        route: FuzzyRoute::ErrorHandling,
        source: "pub fn parse_number(value: &str) -> Result<u32, std::num::ParseIntError> { let parsed = value.parse::<u32>()?; Ok(parsed) }",
    },
    Challenge {
        id: "option",
        section: "rust.case.option",
        prompt: "handle a missing value with Option and map",
        route: FuzzyRoute::ErrorHandling,
        source: "pub fn optional_length(value: Option<&str>) -> Option<usize> { value.map(str::len) }",
    },
    Challenge {
        id: "macro",
        section: "rust.case.macro",
        prompt: "write a variadic macro that counts expressions including a trailing comma",
        route: FuzzyRoute::MacroIterator,
        source: "macro_rules! count_items { ($($item:expr),* $(,)?) => { <[()]>::len(&[$({ let _ = &$item; () }),*]) }; } pub fn macro_count() -> usize { count_items!(1, \"x\", true,) }",
    },
    Challenge {
        id: "iterator",
        section: "rust.case.iterator",
        prompt: "filter even values and deduplicate an iterator edge case",
        route: FuzzyRoute::MacroIterator,
        source: "pub fn unique_even(values: &[i32]) -> Vec<i32> { values.iter().copied().filter(|value| value % 2 == 0).collect::<std::collections::BTreeSet<_>>().into_iter().collect() }",
    },
];

fn mode() -> &'static str {
    match std::env::var("AIR_PIPELINE_SMOKE_MODE").as_deref() {
        Ok("neuro") | Err(_) => "neuro",
        Ok("dual") => "dual",
        Ok(value) => panic!("unsupported AIR_PIPELINE_SMOKE_MODE={value}; use neuro or dual"),
    }
}
fn archive() -> ArchiveReader {
    let mut builder = ArchiveBuilder::new("rust-pipeline_smoke");
    builder.add_language("rust").expect("add Rust language");
    for case in &CHALLENGES {
        builder
            .add_section(case.section, "rust-source", case.source.as_bytes())
            .expect("add static Rust snippet");
    }
    let bytes = builder.finish().expect("build deterministic AIRW smoke archive");
    let path = temp_path("pipeline_smoke", "airw");
    fs::write(&path, bytes).expect("write generated AIRW archive");
    let reader = ArchiveReader::open(&path).expect("mmap generated AIRW archive");
    assert!(reader.is_memory_mapped());
    fs::remove_file(path).expect("remove generated AIRW archive");
    reader
}

fn temp_path(stem: &str, extension: &str) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir().join(format!(
        "air-{stem}-{}-{}.{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed),
        extension
    ))
}

struct ExactArtifact(&'static str);
impl VerificationOracle for ExactArtifact {
    fn verify(
        &self,
        request: &CodingRequest,
        candidate: &RankedCandidate,
        artifact: &[u8],
    ) -> OracleDecision {
        let expected_id_matches = request.concepts().next()
            .is_some_and(|concept| concept.value == format!("case.{}", self.0));
        if expected_id_matches && candidate.id == self.0 {
            let expected = CHALLENGES.iter().find(|case| case.id == self.0).unwrap();
            if artifact == expected.source.as_bytes() {
                return OracleDecision::Accept;
            }
        }
        OracleDecision::Reject
    }
}


struct ExactGeneratedOutput(&'static str);
impl GeneratedOutputVerifier for ExactGeneratedOutput {
    type Error = String;
    fn verify(&self, _question: &str, candidate: &str) -> Result<(), Self::Error> {
        if candidate == self.0 {
            Ok(())
        } else {
            Err("generated output differed from expected Rust artifact".into())
        }
    }
}

static COMPILE_RESULT: OnceLock<Result<(), String>> = OnceLock::new();

fn compile_all_candidates() -> Result<(), String> {
    let source = CHALLENGES
        .iter()
        .map(|case| case.source)
        .collect::<Vec<_>>()
        .join("\n");
    let input = temp_path("rust-pipeline_smoke", "rs");
    let output = temp_path("rust-pipeline_smoke", "rmeta");
    fs::write(&input, source).map_err(|e| e.to_string())?;
    let result = Command::new("rustc")
        .args(["--edition=2021", "--crate-type=lib", "--emit=metadata"])
        .arg("--crate-name")
        .arg("air_rust_pipeline_smoke")
        .arg(&input)
        .arg("-o")
        .arg(&output)
        .output()
        .map_err(|e| format!("start rustc: {e}"))?;
    let _ = fs::remove_file(input);
    let _ = fs::remove_file(output);
    if result.status.success() {
        Ok(())
    } else {
        Err(format!(
            "rustc rejected generated candidates:\n{}",
            String::from_utf8_lossy(&result.stderr)
        ))
    }
}

fn peak_rss_mb() -> u64 {
    fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status.lines().find_map(|line| {
                let value = line.strip_prefix("VmHWM:")?.split_whitespace().next()?;
                value.parse::<u64>().ok()
            })
        })
        .map(|kb| (kb + 1023) / 1024)
        .unwrap_or(0)
}

fn run_case(id: &str) {
    let started = Instant::now();
    let current_mode = mode();
    let case = CHALLENGES.iter().find(|case| case.id == id).unwrap();
    let reader = archive();
    let concept_value = format!("case.{}", case.id);
    let concept = TypedConcept::new(ConceptKind::Intent, concept_value).unwrap();
    let request = CodingRequest::new("rust", vec![concept]).unwrap();
    let rule = ReductionRule::new(
        case.id,
        "rust",
        vec![TypedConcept::new(ConceptKind::Intent, format!("case.{}", case.id)).unwrap()],
        100,
        case.section,
    )
    .unwrap();
    let reducer = CodingReducer::new(vec![rule]).unwrap();

    if current_mode == "dual" {
        assert_eq!(route_fuzzy(case.prompt), case.route, "fuzzy route for {id}");
        let (triggered_prompt, thinking) = if CHALLENGES.iter().position(|item| item.id == id).unwrap() % 2 == 0 {
            (format!("deep {}", case.prompt), ThinkingMode::Budgeted { reasoning_tokens: 64 })
        } else {
            (format!("! {}", case.prompt), ThinkingMode::Direct)
        };
        match route_prompt(&triggered_prompt, 0.99, 0.5) {
            RouteDecision::EscalateToSlm { reason: EscalationReason::ExplicitTrigger } => {}
            other => panic!("explicit {id} prompt did not escalate: {other:?}"),
        }
        let backend = DeterministicMock::new(case.source);
        let verifier = ExactGeneratedOutput(case.source);
        let options = GenerationOptions { thinking, max_new_tokens: 512 };
        let verified = run_verified(&backend, &verifier, &triggered_prompt, &options)
            .unwrap_or_else(|error| panic!("air-core rejected generated {id} output: {error}"));
        assert_eq!(verified.completion.text, case.source);
        assert_eq!(verified.completion.elapsed.as_millis(), 0);
    }

    match reducer.resolve(&request, &reader, Some(&ExactArtifact(case.id)), 4) {
        Resolution::Verified { candidate, artifact, considered } => {
            assert_eq!(candidate.id, case.id);
            assert_eq!(artifact, case.source.as_bytes());
            assert_eq!(considered, 1);
        }
        other => panic!("AIR core did not verify {id}: {other:?}"),
    }
    let compile = COMPILE_RESULT.get_or_init(compile_all_candidates);
    assert!(compile.is_ok(), "{}", compile.as_ref().err().unwrap());
    let latency_ms = started.elapsed().as_secs_f64() * 1000.0;
    println!(
        "pipeline_smoke_CASE mode={current_mode} id={id} result=pass latency_ms={latency_ms:.3} peak_rss_mb={}",
        peak_rss_mb()
    );
}

macro_rules! challenge_test {
    ($name:ident, $id:literal) => {
        #[test]
        fn $name() {
            run_case($id);
        }
    };
}

challenge_test!(rust_case_ownership, "ownership");
challenge_test!(rust_case_borrowing, "borrowing");
challenge_test!(rust_case_lifetime, "lifetime");
challenge_test!(rust_case_trait, "trait");
challenge_test!(rust_case_generic, "generic");
challenge_test!(rust_case_display, "display");
challenge_test!(rust_case_async, "async");
challenge_test!(rust_case_concurrency, "concurrency");
challenge_test!(rust_case_result, "result");
challenge_test!(rust_case_option, "option");
challenge_test!(rust_case_macro, "macro");
challenge_test!(rust_case_iterator, "iterator");

#[test]
fn fuzzy_routing_six_of_six() {
    let examples = [
        ("borrow a Rust String without moving it", FuzzyRoute::RustOwnership, "ownership"),
        ("add a lifetime to return a borrowed value", FuzzyRoute::RustLifetime, "lifetime"),
        ("implement a generic trait for a type", FuzzyRoute::TraitGeneric, "trait_generic"),
        ("write an async function and spawn concurrent tasks", FuzzyRoute::AsyncConcurrency, "async_concurrency"),
        ("propagate a parse error with Result and ?", FuzzyRoute::ErrorHandling, "error_handling"),
        ("write a macro and handle iterator edge cases", FuzzyRoute::MacroIterator, "macro_iterator"),
    ];
    for (prompt, expected, case) in examples {
        assert_eq!(route_fuzzy(prompt), expected, "fuzzy route {case}");
        println!("FUZZY_ROUTE result=pass case={case}");
    }
}

#[test]
fn deep_and_bang_triggers_are_explicit_in_dual_mode() {
    match mode() {
        "neuro" => {
            assert!(matches!(
                route_prompt("ordinary Rust question", 0.99, 0.5),
                RouteDecision::NeuroSymbolicOnly
            ));
        }
        "dual" => {
            assert!(matches!(
                route_prompt("deep explain ownership", 0.99, 0.5),
                RouteDecision::EscalateToSlm { reason: EscalationReason::ExplicitTrigger }
            ));
            assert!(matches!(
                route_prompt("! explain ownership", 0.99, 0.5),
                RouteDecision::EscalateToSlm { reason: EscalationReason::ExplicitTrigger }
            ));
        }
        _ => unreachable!(),
    }
}
