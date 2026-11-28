//! Warm, bounded rustc metadata verification for short AIR coding loops.
//!
//! Initializes a private scratch workspace in /dev/shm when available and
//! precompiles workspace libraries offline once. Subsequent candidate checks
//! invoke rustc directly against the cached .rlib artifacts; no Cargo lock is
//! taken in the per-candidate loop. This verifies compilation, not execution.

use crate::compiler_repair::{parse_json_diagnostics, CompilerDiagnostic};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    error::Error,
    ffi::OsStr,
    fmt,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{atomic::{AtomicU64, Ordering}, Mutex, OnceLock},
    time::{Duration, Instant},
};

pub const DEFAULT_EDITION: &str = "2021";
pub const MAX_SOURCE_BYTES: usize = 1_048_576;
pub const DEFAULT_CACHE_CAPACITY: usize = 128;
const REQUIRED_WORKSPACE_RLIBS: &[&str] = &["air_core", "air_weights"];

#[derive(Debug)]
pub enum HotSwapError {
    Io(std::io::Error),
    CargoUnavailable(std::io::Error),
    PreparationFailed(String),
    NoPrecompiledRlibs(PathBuf),
    InvalidEdition,
    SourceTooLarge,
    CachePoisoned,
    RustcUnavailable(std::io::Error),
}

impl fmt::Display for HotSwapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "hot-swap workspace I/O error: {e}"),
            Self::CargoUnavailable(e) => write!(f, "cargo is needed once to prepare rlibs: {e}"),
            Self::PreparationFailed(e) => write!(f, "offline workspace preparation failed: {e}"),
            Self::NoPrecompiledRlibs(path) => write!(f, "no usable precompiled rlibs in {}", path.display()),
            Self::InvalidEdition => f.write_str("unsupported Rust edition"),
            Self::SourceTooLarge => f.write_str("Rust source exceeds the 1 MiB verification limit"),
            Self::CachePoisoned => f.write_str("hot-swap cache lock is poisoned"),
            Self::RustcUnavailable(e) => write!(f, "could not start rustc: {e}"),
        }
    }
}
impl Error for HotSwapError {}
impl From<std::io::Error> for HotSwapError {
    fn from(value: std::io::Error) -> Self { Self::Io(value) }
}

#[derive(Debug, Clone)]
pub struct CompileOutcome {
    pub success: bool,
    pub cache_hit: bool,
    pub elapsed: Duration,
    pub artifact: Option<PathBuf>,
    pub diagnostics: Vec<CompilerDiagnostic>,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, Clone)]
struct CacheEntry {
    success: bool,
    artifact: Option<PathBuf>,
    diagnostics: Vec<CompilerDiagnostic>,
    stdout: String,
    stderr: String,
}

#[derive(Default)]
struct CompileCache {
    entries: HashMap<[u8; 32], CacheEntry>,
    order: VecDeque<[u8; 32]>,
}

pub struct WarmRustVerifier {
    scratch: PathBuf,
    deps_dir: PathBuf,
    externs: Vec<(String, PathBuf)>,
    edition: String,
    cache_capacity: usize,
    sequence: AtomicU64,
    cache: Mutex<CompileCache>,
}

static PREPARED_WORKSPACES: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();

impl WarmRustVerifier {
    /// Prepare workspace rlibs once, then keep direct rustc invocations warm.
    /// Preparation is offline and only happens when the required workspace
    /// libraries are missing from target/debug/deps.
    pub fn initialize(workspace_root: impl AsRef<Path>) -> Result<Self, HotSwapError> {
        Self::initialize_with(workspace_root, DEFAULT_EDITION, DEFAULT_CACHE_CAPACITY)
    }

    pub fn initialize_with(
        workspace_root: impl AsRef<Path>,
        edition: &str,
        cache_capacity: usize,
    ) -> Result<Self, HotSwapError> {
        if !supported_edition(edition) { return Err(HotSwapError::InvalidEdition); }
        let workspace_root = workspace_root.as_ref().canonicalize()?;
        let deps_dir = target_deps_dir(&workspace_root);
        prepare_workspace_once(&workspace_root, &deps_dir)?;
        Self::from_prebuilt_artifacts(
            &workspace_root,
            &deps_dir,
            edition,
            cache_capacity,
        )
    }

    /// Open an already-built artifact directory. Useful for embedded launchers
    /// and tests running under Cargo, where invoking nested Cargo would contend
    /// for the outer build lock.
    pub fn from_prebuilt_artifacts(
        workspace_root: impl AsRef<Path>,
        deps_dir: impl AsRef<Path>,
        edition: &str,
        cache_capacity: usize,
    ) -> Result<Self, HotSwapError> {
        if !supported_edition(edition) { return Err(HotSwapError::InvalidEdition); }
        let workspace_root = workspace_root.as_ref().canonicalize()?;
        let deps_dir = deps_dir.as_ref().canonicalize()?;
        let externs = discover_rlibs(&deps_dir)?;
        let scratch = select_scratch_workspace()?;
        Ok(Self {
            scratch,
            deps_dir,
            externs,
            edition: edition.to_owned(),
            cache_capacity: cache_capacity.max(1),
            sequence: AtomicU64::new(0),
            cache: Mutex::new(CompileCache::default()),
        })
    }

    pub fn scratch_workspace(&self) -> &Path { &self.scratch }
    pub fn dependency_directory(&self) -> &Path { &self.deps_dir }
    pub fn precompiled_extern_count(&self) -> usize { self.externs.len() }

    /// Compile a candidate to metadata with direct rustc invocation. A digest
    /// cache makes exact retries return without starting another compiler.
    pub fn verify_source(&self, source: &str) -> Result<CompileOutcome, HotSwapError> {
        self.verify_source_with_edition(source, &self.edition)
    }

    pub fn verify_source_with_edition(
        &self,
        source: &str,
        edition: &str,
    ) -> Result<CompileOutcome, HotSwapError> {
        if !supported_edition(edition) { return Err(HotSwapError::InvalidEdition); }
        if source.len() > MAX_SOURCE_BYTES { return Err(HotSwapError::SourceTooLarge); }
        let started = Instant::now();
        let key = cache_key(edition, source, &self.externs);
        {
            let mut cache = self.cache.lock().map_err(|_| HotSwapError::CachePoisoned)?;
            if let Some(entry) = cache.entries.get(&key).cloned() {
                touch(&mut cache.order, key);
                return Ok(outcome(entry, true, started.elapsed()));
            }
        }

        let id = self.sequence.fetch_add(1, Ordering::Relaxed);
        let stem = format!("candidate-{}-{id}", std::process::id());
        let source_path = self.scratch.join(format!("{stem}.rs"));
        let artifact_path = self.scratch.join(format!("{stem}.rmeta"));
        fs::write(&source_path, source)?;

        let mut command = Command::new("rustc");
        command
            .arg("--edition").arg(edition)
            .arg("--crate-type=lib")
            .arg("--crate-name").arg(format!("air_verify_{id}"))
            .arg("--error-format=json")
            .arg("--emit=metadata")
            .arg("-L").arg(format!("dependency={}", self.deps_dir.display()));
        for (name, path) in &self.externs {
            command.arg("--extern").arg(format!("{name}={}", path.display()));
        }
        command.arg("-o").arg(&artifact_path).arg(&source_path);
        let execution = command.output().map_err(HotSwapError::RustcUnavailable)?;
        let _ = fs::remove_file(&source_path);
        let stdout = String::from_utf8_lossy(&execution.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&execution.stderr).into_owned();
        let diagnostic_stream = format!("{stdout}\n{stderr}");
        let entry = CacheEntry {
            success: execution.status.success(),
            artifact: execution.status.success().then_some(artifact_path.clone()),
            diagnostics: parse_json_diagnostics(&diagnostic_stream),
            stdout,
            stderr,
        };
        let result = outcome(entry.clone(), false, started.elapsed());
        self.insert_cache(key, entry)?;
        Ok(result)
    }

    fn insert_cache(&self, key: [u8; 32], entry: CacheEntry) -> Result<(), HotSwapError> {
        let mut cache = self.cache.lock().map_err(|_| HotSwapError::CachePoisoned)?;
        if let Some(old) = cache.entries.insert(key, entry) {
            if let Some(path) = old.artifact { let _ = fs::remove_file(path); }
            touch(&mut cache.order, key);
        } else {
            cache.order.push_back(key);
        }
        while cache.entries.len() > self.cache_capacity {
            let Some(evicted) = cache.order.pop_front() else { break };
            if let Some(old) = cache.entries.remove(&evicted) {
                if let Some(path) = old.artifact { let _ = fs::remove_file(path); }
            }
        }
        Ok(())
    }
}

fn outcome(entry: CacheEntry, cache_hit: bool, elapsed: Duration) -> CompileOutcome {
    CompileOutcome {
        success: entry.success,
        cache_hit,
        elapsed,
        artifact: entry.artifact,
        diagnostics: entry.diagnostics,
        stdout: entry.stdout,
        stderr: entry.stderr,
    }
}

fn supported_edition(value: &str) -> bool {
    matches!(value, "2018" | "2021" | "2024")
}

fn cache_key(edition: &str, source: &str, externs: &[(String, PathBuf)]) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(edition.as_bytes());
    hash.update([0]);
    hash.update(source.as_bytes());
    for (name, path) in externs {
        hash.update(name.as_bytes());
        hash.update([0]);
        hash.update(path.as_os_str().to_string_lossy().as_bytes());
        hash.update([0]);
    }
    hash.finalize().into()
}

fn touch(order: &mut VecDeque<[u8; 32]>, key: [u8; 32]) {
    if let Some(index) = order.iter().position(|value| *value == key) {
        order.remove(index);
    }
    order.push_back(key);
}

fn target_deps_dir(workspace_root: &Path) -> PathBuf {
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .map(|path| if path.is_absolute() { path } else { workspace_root.join(path) })
        .unwrap_or_else(|| workspace_root.join("target"));
    target.join("debug").join("deps")
}

fn has_required_workspace_rlibs(deps_dir: &Path) -> bool {
    let Ok(entries) = fs::read_dir(deps_dir) else { return false };
    let mut names = HashSet::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension() != Some(OsStr::new("rlib")) { continue; }
        if let Some(name) = crate_name(&path) { names.insert(name); }
    }
    REQUIRED_WORKSPACE_RLIBS.iter().all(|name| names.contains(*name))
}

fn prepare_workspace_once(workspace_root: &Path, deps_dir: &Path) -> Result<(), HotSwapError> {
    let registry = PREPARED_WORKSPACES.get_or_init(|| Mutex::new(HashSet::new()));
    let mut prepared = registry.lock().map_err(|_| HotSwapError::CachePoisoned)?;
    if prepared.contains(workspace_root) && has_required_workspace_rlibs(deps_dir) { return Ok(()); }
    if !has_required_workspace_rlibs(deps_dir) {
        let output = Command::new("cargo")
            .args(["build", "--offline", "--workspace", "--lib"])
            .current_dir(workspace_root)
            .output()
            .map_err(HotSwapError::CargoUnavailable)?;
        if !output.status.success() {
            return Err(HotSwapError::PreparationFailed(format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            )));
        }
    }
    if !has_required_workspace_rlibs(deps_dir) {
        return Err(HotSwapError::NoPrecompiledRlibs(deps_dir.to_path_buf()));
    }
    prepared.insert(workspace_root.to_path_buf());
    Ok(())
}

fn crate_name(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_str()?.strip_prefix("lib")?;
    let (name, hash) = stem.rsplit_once('-')?;
    if hash.len() < 8 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) { return None; }
    let normalized = name.replace('-', "_");
    (!normalized.is_empty()
        && normalized.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'_'))
        .then_some(normalized)
}

fn discover_rlibs(deps_dir: &Path) -> Result<Vec<(String, PathBuf)>, HotSwapError> {
    let entries = fs::read_dir(deps_dir).map_err(HotSwapError::Io)?;
    let mut paths = entries
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension() == Some(OsStr::new("rlib")))
        .collect::<Vec<_>>();
    paths.sort();
    let mut libraries = BTreeMap::new();
    for path in paths {
        if let Some(name) = crate_name(&path) {
            libraries.entry(name).or_insert(path);
        }
    }
    if libraries.is_empty() { return Err(HotSwapError::NoPrecompiledRlibs(deps_dir.to_path_buf())); }
    Ok(libraries.into_iter().collect())
}

fn select_scratch_workspace() -> Result<PathBuf, HotSwapError> {
    let override_path = std::env::var_os("AIR_VERIFY_ROOT").map(PathBuf::from);
    let candidates = override_path
        .into_iter()
        .chain([
            PathBuf::from("/dev/shm/air-verify"),
            PathBuf::from("/tmp/air-verify"),
            std::env::temp_dir().join("air-verify"),
        ]);
    let mut last_error = None;
    for path in candidates {
        match fs::create_dir_all(&path) {
            Ok(()) => {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if let Err(error) = fs::set_permissions(&path, fs::Permissions::from_mode(0o700)) {
                        last_error = Some(error);
                        continue;
                    }
                }
                return path.canonicalize().map_err(HotSwapError::Io);
            }
            Err(error) => last_error = Some(error),
        }
    }
    Err(HotSwapError::Io(last_error.unwrap_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "no scratch workspace candidate")
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn artifact_dir_for_test() -> PathBuf {
        static TEST_DEPS: OnceLock<PathBuf> = OnceLock::new();
        TEST_DEPS.get_or_init(|| {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let id = NEXT.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!(
                "air-verify-test-rlibs-{}-{id}",
                std::process::id()
            ));
            fs::create_dir_all(&dir).unwrap();
            let source = dir.join("dependency.rs");
            let library = dir.join("libhot_swap_test_dep-0123456789abcdef.rlib");
            fs::write(&source, "pub fn answer() -> u8 { 42 }").unwrap();
            let output = Command::new("rustc")
                .args(["--edition=2021", "--crate-type=rlib", "--crate-name=hot_swap_test_dep"])
                .arg("--emit=link")
                .arg("-o").arg(&library)
                .arg(&source)
                .output().expect("start rustc for an isolated test rlib");
            let _ = fs::remove_file(source);
            assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
            dir
        }).clone()
    }

    fn verifier() -> WarmRustVerifier {
        let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent().unwrap().parent().unwrap();
        WarmRustVerifier::from_prebuilt_artifacts(
            workspace_root,
            artifact_dir_for_test(),
            DEFAULT_EDITION,
            8,
        ).expect("use Cargo's already built dependency artifacts")
    }

    #[test]
    fn direct_rustc_compile_and_exact_retry_cache() {
        let verifier = verifier();
        let source = "pub fn answer() -> u8 { hot_swap_test_dep::answer() }";
        let first = verifier.verify_source(source).unwrap();
        assert!(first.success, "{}", first.stderr);
        assert!(!first.cache_hit);
        assert!(first.artifact.as_ref().is_some_and(|path| path.exists()));
        let second = verifier.verify_source(source).unwrap();
        assert!(second.success, "{}", second.stderr);
        assert!(second.cache_hit);
        assert!(second.elapsed <= first.elapsed || second.elapsed.as_millis() == 0);
        println!(
            "HOT_SWAP_RUSTC success=true first_ms={} cached_ms={} cached=true",
            first.elapsed.as_secs_f64() * 1000.0,
            second.elapsed.as_secs_f64() * 1000.0,
        );
    }

    #[test]
    fn structured_rustc_failure_is_returned() {
        let verifier = verifier();
        let result = verifier.verify_source("pub fn broken( {").unwrap();
        assert!(!result.success);
        assert!(!result.diagnostics.is_empty());
        assert!(result.diagnostics[0].message.contains("expected"));
        assert!(result.stderr.contains("error"));
    }

    #[test]
    fn enforces_source_and_edition_bounds() {
        let verifier = verifier();
        assert!(matches!(
            verifier.verify_source_with_edition("fn x() {}", "1998"),
            Err(HotSwapError::InvalidEdition)
        ));
        assert!(matches!(
            verifier.verify_source(&"x".repeat(MAX_SOURCE_BYTES + 1)),
            Err(HotSwapError::SourceTooLarge)
        ));
    }

    #[test]
    fn scratch_workspace_is_private_and_memory_first() {
        let verifier = verifier();
        assert!(verifier.scratch_workspace().is_absolute());
        assert!(verifier.scratch_workspace().exists());
        assert!(verifier.dependency_directory().exists());
        assert!(verifier.precompiled_extern_count() > 0);
    }
}
