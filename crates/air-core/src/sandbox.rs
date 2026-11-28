//! Rootless process isolation for generated-code verification.
//!
//! Linux runs apply rlimits, no_new_privs, a syscall deny filter, and (when
//! supported) Landlock. Filesystem writes are allowed only below scratch_dir.
//! Read-only runtime paths are explicit exceptions needed to load the verifier
//! executable and its libraries. On older kernels, BestEffort reports the
//! unavailable Landlock status while retaining rlimits and seccomp.

use std::ffi::{CString, OsStr};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LandlockMode {
    /// Continue with rlimits and seccomp if the kernel lacks Landlock.
    BestEffort,
    /// Refuse to start the child if Landlock cannot be enforced.
    Required,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Enforcement {
    Enforced,
    Unavailable,
    UnsupportedPlatform,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EnforcementReport {
    pub landlock: Enforcement,
    pub landlock_abi: Option<i32>,
    pub seccomp: Enforcement,
    pub resource_limits: Enforcement,
}

#[derive(Clone, Debug)]
pub struct SandboxConfig {
    pub scratch_dir: PathBuf,
    /// Read-only paths needed by the executable, dynamic loader and runtime.
    /// Every write right remains denied outside scratch_dir.
    pub readonly_paths: Vec<PathBuf>,
    pub landlock_mode: LandlockMode,
    pub cpu_seconds: u64,
    pub address_space_bytes: u64,
    pub file_size_bytes: u64,
    pub wall_timeout: Duration,
    pub max_output_bytes: usize,
}

impl SandboxConfig {
    pub fn new(scratch_dir: impl Into<PathBuf>) -> Self {
        Self {
            scratch_dir: scratch_dir.into(),
            readonly_paths: [
                "/usr", "/bin", "/sbin", "/lib", "/lib64", "/etc", "/proc", "/dev",
            ]
            .into_iter()
            .map(PathBuf::from)
            .collect(),
            landlock_mode: LandlockMode::BestEffort,
            cpu_seconds: 5,
            address_space_bytes: 1024 * 1024 * 1024,
            file_size_bytes: 16 * 1024 * 1024,
            wall_timeout: Duration::from_secs(10),
            max_output_bytes: 1024 * 1024,
        }
    }

    pub fn allow_readonly_path(&mut self, path: impl Into<PathBuf>) {
        self.readonly_paths.push(path.into());
    }

    pub fn require_landlock(&mut self, required: bool) {
        self.landlock_mode = if required {
            LandlockMode::Required
        } else {
            LandlockMode::BestEffort
        };
    }
}

#[derive(Debug)]
pub struct SandboxOutput {
    pub status: ExitStatus,
    pub timed_out: bool,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
    pub enforcement: EnforcementReport,
}

struct PreparedSecurity {
    scratch: CString,
    readonly: Vec<CString>,
    handled_access: u64,
    landlock_enabled: bool,
    rlimits: (u64, u64, u64),
    seccomp: Vec<libc::sock_filter>,
}

fn prepare_scratch(path: &Path) -> io::Result<PathBuf> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "scratch directory cannot be a symlink"));
        }
        Ok(metadata) if !metadata.is_dir() => {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "scratch path is not a directory"));
        }
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => std::fs::create_dir_all(path)?,
        Err(error) => return Err(error),
    }
    let canonical = std::fs::canonicalize(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let metadata = std::fs::metadata(&canonical)?;
        let effective_uid = unsafe { libc::geteuid() };
        if metadata.uid() != effective_uid {
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, "scratch directory must be owned by the current user"));
        }
        std::fs::set_permissions(&canonical, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(canonical)
}

fn canonical_readonly_paths(paths: &[PathBuf]) -> io::Result<Vec<PathBuf>> {
    let mut result = Vec::new();
    for path in paths {
        match std::fs::canonicalize(path) {
            Ok(canonical) if !result.contains(&canonical) => result.push(canonical),
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(result)
}

#[cfg(target_os = "linux")]
fn probe_landlock_abi() -> Option<i32> {
    const LANDLOCK_CREATE_RULESET_VERSION: libc::c_ulong = 1;
    const SYS_LANDLOCK_CREATE_RULESET: libc::c_long = 444;
    let version = unsafe {
        libc::syscall(
            SYS_LANDLOCK_CREATE_RULESET,
            std::ptr::null::<libc::c_void>(),
            0usize,
            LANDLOCK_CREATE_RULESET_VERSION,
        )
    };
    (version >= 1 && version <= i32::MAX as libc::c_long).then_some(version as i32)
}

#[cfg(not(target_os = "linux"))]
fn probe_landlock_abi() -> Option<i32> {
    None
}

#[cfg(target_os = "linux")]
fn landlock_access(abi: i32) -> u64 {
    const EXECUTE: u64 = 1 << 0;
    const WRITE_FILE: u64 = 1 << 1;
    const READ_FILE: u64 = 1 << 2;
    const READ_DIR: u64 = 1 << 3;
    const REMOVE_DIR: u64 = 1 << 4;
    const REMOVE_FILE: u64 = 1 << 5;
    const MAKE_CHAR: u64 = 1 << 6;
    const MAKE_DIR: u64 = 1 << 7;
    const MAKE_REG: u64 = 1 << 8;
    const MAKE_SOCK: u64 = 1 << 9;
    const MAKE_FIFO: u64 = 1 << 10;
    const MAKE_BLOCK: u64 = 1 << 11;
    const MAKE_SYM: u64 = 1 << 12;
    const REFER: u64 = 1 << 13;
    const TRUNCATE: u64 = 1 << 14;
    const IOCTL_DEV: u64 = 1 << 15;
    let mut access = EXECUTE
        | WRITE_FILE
        | READ_FILE
        | READ_DIR
        | REMOVE_DIR
        | REMOVE_FILE
        | MAKE_CHAR
        | MAKE_DIR
        | MAKE_REG
        | MAKE_SOCK
        | MAKE_FIFO
        | MAKE_BLOCK
        | MAKE_SYM;
    if abi >= 2 {
        access |= REFER;
    }
    if abi >= 3 {
        access |= TRUNCATE;
    }
    if abi >= 5 {
        access |= IOCTL_DEV;
    }
    access
}

#[cfg(target_os = "linux")]
fn prepare_security(config: &SandboxConfig) -> io::Result<(PreparedSecurity, EnforcementReport)> {
    let scratch_path = prepare_scratch(&config.scratch_dir)?;
    let readonly_paths = canonical_readonly_paths(&config.readonly_paths)?;
    use std::os::unix::ffi::OsStrExt;
    let scratch = CString::new(scratch_path.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "scratch path contains NUL"))?;
    let readonly = readonly_paths
        .iter()
        .map(|path| CString::new(path.as_os_str().as_bytes())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "read-only path contains NUL")))
        .collect::<io::Result<Vec<_>>>()?;

    let abi = probe_landlock_abi();
    if abi.is_none() && config.landlock_mode == LandlockMode::Required {
        return Err(io::Error::new(io::ErrorKind::Unsupported, "Landlock is unavailable on this kernel"));
    }
    let seccomp = build_seccomp_filter();
    let report = EnforcementReport {
        landlock: if abi.is_some() { Enforcement::Enforced } else { Enforcement::Unavailable },
        landlock_abi: abi,
        seccomp: if seccomp.is_empty() { Enforcement::UnsupportedPlatform } else { Enforcement::Enforced },
        resource_limits: Enforcement::Enforced,
    };
    let prepared = PreparedSecurity {
        scratch,
        readonly,
        handled_access: landlock_access(abi.unwrap_or(1)),
        landlock_enabled: abi.is_some(),
        rlimits: (config.cpu_seconds, config.address_space_bytes, config.file_size_bytes),
        seccomp,
    };
    Ok((prepared, report))
}

#[cfg(not(target_os = "linux"))]
fn prepare_security(config: &SandboxConfig) -> io::Result<(PreparedSecurity, EnforcementReport)> {
    let _ = prepare_scratch(&config.scratch_dir)?;
    if config.landlock_mode == LandlockMode::Required {
        return Err(io::Error::new(io::ErrorKind::Unsupported, "Landlock is Linux-only"));
    }
    Ok((
        PreparedSecurity {
            scratch: CString::new("").expect("empty string has no NUL"),
            readonly: Vec::new(),
            handled_access: 0,
            landlock_enabled: false,
            rlimits: (config.cpu_seconds, config.address_space_bytes, config.file_size_bytes),
            seccomp: Vec::new(),
        },
        EnforcementReport {
            landlock: Enforcement::UnsupportedPlatform,
            landlock_abi: None,
            seccomp: Enforcement::UnsupportedPlatform,
            resource_limits: if cfg!(unix) { Enforcement::Enforced } else { Enforcement::UnsupportedPlatform },
        },
    ))
}

#[cfg(target_os = "linux")]
unsafe fn add_landlock_path_rule(
    ruleset_fd: libc::c_int,
    path: &CString,
    access: u64,
) -> io::Result<()> {
    const SYS_LANDLOCK_ADD_RULE: libc::c_long = 445;
    const LANDLOCK_RULE_PATH_BENEATH: libc::c_uint = 1;
    const O_PATH: libc::c_int = 0o10000000;
    const O_DIRECTORY: libc::c_int = 0o200000;
    const O_CLOEXEC: libc::c_int = 0o2000000;
    #[repr(C)]
    struct PathBeneathAttr {
        allowed_access: u64,
        parent_fd: libc::c_int,
    }
    let path_fd = libc::open(path.as_ptr(), O_PATH | O_DIRECTORY | O_CLOEXEC);
    if path_fd < 0 {
        return Err(io::Error::last_os_error());
    }
    let attr = PathBeneathAttr {
        allowed_access: access,
        parent_fd: path_fd,
    };
    let result = libc::syscall(
        SYS_LANDLOCK_ADD_RULE,
        ruleset_fd,
        LANDLOCK_RULE_PATH_BENEATH,
        &attr as *const PathBeneathAttr,
        0u32,
    );
    let saved_error = if result < 0 { Some(io::Error::last_os_error()) } else { None };
    libc::close(path_fd);
    match saved_error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

#[cfg(target_os = "linux")]
unsafe fn apply_landlock(security: &PreparedSecurity) -> io::Result<()> {
    const SYS_LANDLOCK_CREATE_RULESET: libc::c_long = 444;
    const SYS_LANDLOCK_RESTRICT_SELF: libc::c_long = 446;
    #[repr(C)]
    struct RulesetAttr {
        handled_access_fs: u64,
    }
    const EXECUTE: u64 = 1 << 0;
    const READ_FILE: u64 = 1 << 2;
    const READ_DIR: u64 = 1 << 3;
    let attr = RulesetAttr { handled_access_fs: security.handled_access };
    let ruleset_fd = libc::syscall(
        SYS_LANDLOCK_CREATE_RULESET,
        &attr as *const RulesetAttr,
        std::mem::size_of::<RulesetAttr>(),
        0u32,
    ) as libc::c_int;
    if ruleset_fd < 0 {
        return Err(io::Error::last_os_error());
    }
    let readonly_access = EXECUTE | READ_FILE | READ_DIR;
    for path in &security.readonly {
        if let Err(error) = add_landlock_path_rule(ruleset_fd, path, readonly_access) {
            libc::close(ruleset_fd);
            return Err(error);
        }
    }
    if let Err(error) = add_landlock_path_rule(ruleset_fd, &security.scratch, security.handled_access) {
        libc::close(ruleset_fd);
        return Err(error);
    }
    let result = libc::syscall(SYS_LANDLOCK_RESTRICT_SELF, ruleset_fd, 0u32);
    let saved_error = if result < 0 { Some(io::Error::last_os_error()) } else { None };
    libc::close(ruleset_fd);
    match saved_error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

#[cfg(target_os = "linux")]
fn build_seccomp_filter() -> Vec<libc::sock_filter> {
    const BPF_LD: u16 = 0x00;
    const BPF_W: u16 = 0x00;
    const BPF_ABS: u16 = 0x20;
    const BPF_JMP: u16 = 0x05;
    const BPF_JEQ: u16 = 0x10;
    const BPF_K: u16 = 0x00;
    const BPF_ALU: u16 = 0x04;
    const BPF_AND: u16 = 0x50;
    const BPF_RET: u16 = 0x06;
    const SECCOMP_RET_KILL_PROCESS: u32 = 0x8000_0000;
    const SECCOMP_RET_ERRNO: u32 = 0x0005_0000;
    const SECCOMP_RET_ALLOW: u32 = 0x7fff_0000;
    const AUDIT_ARCH_X86_64: u32 = 0xc000_003e;
    const AUDIT_ARCH_AARCH64: u32 = 0xc000_00b7;
    const OFF_ARCH: u32 = 4;
    const OFF_NR: u32 = 0;
    const OFF_ARG0: u32 = 16;
    const THREAD_FLAGS: u32 = 0x0001_0900; // CLONE_VM | CLONE_SIGHAND | CLONE_THREAD

    fn ins(code: u16, jt: u8, jf: u8, k: u32) -> libc::sock_filter {
        libc::sock_filter { code, jt, jf, k }
    }
    fn jump_eq(value: u32, jt: u8, jf: u8) -> libc::sock_filter {
        ins(BPF_JMP | BPF_JEQ | BPF_K, jt, jf, value)
    }

    #[cfg(target_arch = "x86_64")]
    let audit_arch = AUDIT_ARCH_X86_64;
    #[cfg(target_arch = "aarch64")]
    let audit_arch = AUDIT_ARCH_AARCH64;
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    return Vec::new();

    #[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
    {
        let mut code = vec![
            ins(BPF_LD | BPF_W | BPF_ABS, 0, 0, OFF_ARCH),
            jump_eq(audit_arch, 1, 0),
            ins(BPF_RET | BPF_K, 0, 0, SECCOMP_RET_KILL_PROCESS),
            ins(BPF_LD | BPF_W | BPF_ABS, 0, 0, OFF_NR),
        ];
        let denied: &[libc::c_long] = &[
            libc::SYS_socket,
            libc::SYS_socketpair,
            libc::SYS_connect,
            libc::SYS_bind,
            libc::SYS_listen,
            libc::SYS_accept,
            libc::SYS_accept4,
            libc::SYS_mount,
            libc::SYS_umount2,
            libc::SYS_pivot_root,
            libc::SYS_unshare,
            libc::SYS_setns,
            libc::SYS_ptrace,
            libc::SYS_kexec_load,
            libc::SYS_init_module,
            libc::SYS_delete_module,
            libc::SYS_reboot,
            libc::SYS_bpf,
            libc::SYS_perf_event_open,
            libc::SYS_clone3,
            libc::SYS_fork,
            libc::SYS_vfork,
        ];
        for syscall_nr in denied {
            code.push(jump_eq(*syscall_nr as u32, 0, 1));
            code.push(ins(BPF_RET | BPF_K, 0, 0, SECCOMP_RET_ERRNO | libc::EPERM as u32));
        }
        // Permit clone only for threads. The child cannot create a new process
        // tree; normal Rust thread pools continue to work.
        code.push(jump_eq(libc::SYS_clone as u32, 0, 4));
        code.push(ins(BPF_LD | BPF_W | BPF_ABS, 0, 0, OFF_ARG0));
        code.push(ins(BPF_ALU | BPF_AND | BPF_K, 0, 0, THREAD_FLAGS));
        code.push(jump_eq(THREAD_FLAGS, 1, 0));
        code.push(ins(BPF_RET | BPF_K, 0, 0, SECCOMP_RET_ERRNO | libc::EPERM as u32));
        code.push(ins(BPF_RET | BPF_K, 0, 0, SECCOMP_RET_ALLOW));
        code
    }
}

#[cfg(not(target_os = "linux"))]
fn build_seccomp_filter() -> Vec<libc::sock_filter> {
    Vec::new()
}

#[cfg(target_os = "linux")]
unsafe fn install_seccomp(filter: &[libc::sock_filter]) -> io::Result<()> {
    const PR_SET_SECCOMP: libc::c_int = 22;
    const PR_SET_NO_NEW_PRIVS: libc::c_int = 38;
    const SECCOMP_MODE_FILTER: libc::c_int = 2;
    if libc::prctl(PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0 {
        return Err(io::Error::last_os_error());
    }
    if filter.is_empty() {
        return Ok(());
    }
    let mut program = libc::sock_fprog {
        len: filter.len() as u16,
        filter: filter.as_ptr() as *mut libc::sock_filter,
    };
    if libc::prctl(
        PR_SET_SECCOMP,
        SECCOMP_MODE_FILTER,
        &mut program as *mut libc::sock_fprog as libc::c_ulong,
        0,
        0,
    ) != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn read_capped(mut reader: impl Read, limit: usize) -> io::Result<(Vec<u8>, bool)> {
    let mut output = Vec::with_capacity(limit.min(16 * 1024));
    let mut buffer = [0u8; 8192];
    let mut truncated = false;
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        let room = limit.saturating_sub(output.len());
        let keep = room.min(count);
        output.extend_from_slice(&buffer[..keep]);
        truncated |= keep < count;
    }
    Ok((output, truncated))
}

pub fn run_sandboxed(
    program: impl AsRef<OsStr>,
    args: &[std::ffi::OsString],
    config: &SandboxConfig,
) -> io::Result<SandboxOutput> {
    if config.cpu_seconds == 0 || config.address_space_bytes == 0 || config.file_size_bytes == 0 {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "sandbox limits must be non-zero"));
    }
    let (security, enforcement) = prepare_security(config)?;
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(&config.scratch_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(target_os = "linux")]
    {
        use std::os::unix::process::CommandExt;
        let rlimits = security.rlimits;
        let do_landlock = security.landlock_enabled;
        let prepared = security;
        unsafe {
            command.pre_exec(move || {
                let limits = [
                    (libc::RLIMIT_CPU, rlimits.0, rlimits.0.saturating_add(1)),
                    (libc::RLIMIT_AS, rlimits.1, rlimits.1),
                    (libc::RLIMIT_FSIZE, rlimits.2, rlimits.2),
                    (libc::RLIMIT_CORE, 0, 0),
                ];
                for (resource, soft, hard) in limits {
                    let limit = libc::rlimit {
                        rlim_cur: soft as libc::rlim_t,
                        rlim_max: hard as libc::rlim_t,
                    };
                    if libc::setrlimit(resource, &limit) != 0 {
                        return Err(io::Error::last_os_error());
                    }
                }
                if libc::prctl(38, 1, 0, 0, 0) != 0 { // PR_SET_NO_NEW_PRIVS
                    return Err(io::Error::last_os_error());
                }
                if do_landlock {
                    if let Err(error) = apply_landlock(&prepared) {
                        return Err(error);
                    }
                }
                install_seccomp(&prepared.seccomp)?;
                Ok(())
            });
        }
    }

    #[cfg(all(unix, not(target_os = "linux")))]
    {
        use std::os::unix::process::CommandExt;
        let rlimits = security.rlimits;
        unsafe {
            command.pre_exec(move || {
                for (resource, value) in [
                    (libc::RLIMIT_CPU, rlimits.0),
                    (libc::RLIMIT_AS, rlimits.1),
                    (libc::RLIMIT_FSIZE, rlimits.2),
                ] {
                    let limit = libc::rlimit {
                        rlim_cur: value as libc::rlim_t,
                        rlim_max: value as libc::rlim_t,
                    };
                    if libc::setrlimit(resource, &limit) != 0 {
                        return Err(io::Error::last_os_error());
                    }
                }
                Ok(())
            });
        }
    }

    let mut child = command.spawn()?;
    let stdout = child.stdout.take().expect("stdout is piped");
    let stderr = child.stderr.take().expect("stderr is piped");
    let stdout_limit = config.max_output_bytes;
    let stderr_limit = config.max_output_bytes;
    let stdout_thread = thread::spawn(move || read_capped(stdout, stdout_limit));
    let stderr_thread = thread::spawn(move || read_capped(stderr, stderr_limit));

    let deadline = Instant::now() + config.wall_timeout;
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            timed_out = true;
            let _ = child.kill();
            break child.wait()?;
        }
        thread::sleep(Duration::from_millis(5));
    };
    let (stdout, stdout_truncated) = stdout_thread
        .join()
        .map_err(|_| io::Error::other("stdout reader panicked"))??;
    let (stderr, stderr_truncated) = stderr_thread
        .join()
        .map_err(|_| io::Error::other("stderr reader panicked"))??;

    Ok(SandboxOutput {
        status,
        timed_out,
        stdout,
        stderr,
        stdout_truncated,
        stderr_truncated,
        enforcement,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);

    fn root() -> PathBuf {
        std::env::temp_dir().join(format!(
            "air-rootless-sandbox-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn shell_config(root: &Path) -> SandboxConfig {
        SandboxConfig::new(root.join("scratch"))
    }

    fn args_with_script(script: &str, args: &[OsString]) -> Vec<OsString> {
        let mut result = vec![OsString::from("-c"), OsString::from(script), OsString::from("sandbox-test")];
        result.extend_from_slice(args);
        result
    }

    #[test]
    fn scratch_write_allowed_and_outside_write_denied_when_landlock_exists() {
        let root = root();
        let config = shell_config(&root);
        let scratch_file = config.scratch_dir.join("inside.txt");
        let outside_file = root.join("outside.txt");
        let inside = run_sandboxed(
            "/bin/sh",
            &args_with_script("printf ok > \"$1\"", &[scratch_file.clone().into()]),
            &config,
        ).unwrap();
        assert!(inside.status.success());
        assert_eq!(std::fs::read(&scratch_file).unwrap(), b"ok");
        let outside = run_sandboxed(
            "/bin/sh",
            &args_with_script("printf no > \"$1\"", &[outside_file.clone().into()]),
            &config,
        ).unwrap();
        if outside.enforcement.landlock == Enforcement::Enforced {
            assert!(!outside_file.exists(), "Landlock allowed a write outside scratch");
        } else {
            // Old kernels still apply the remaining restrictions and report fallback.
            assert_eq!(outside.enforcement.landlock, Enforcement::Unavailable);
        }
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn wall_timeout_and_cpu_limit_stop_busy_child() {
        let root = root();
        let mut config = shell_config(&root);
        config.cpu_seconds = 1;
        config.wall_timeout = Duration::from_secs(4);
        let started = Instant::now();
        let output = run_sandboxed(
            "/bin/sh",
            &args_with_script("while :; do :; done", &[]),
            &config,
        ).unwrap();
        assert!(!output.status.success());
        assert!(started.elapsed() < Duration::from_secs(8));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn address_space_limit_rejects_large_python_allocation() {
        if !Path::new("/usr/bin/python3").exists() {
            return;
        }
        let root = root();
        let mut config = shell_config(&root);
        config.address_space_bytes = 128 * 1024 * 1024;
        let args = [
            OsString::from("-S"),
            OsString::from("-c"),
            OsString::from("x = bytearray(536870912); print(len(x))"),
        ];
        let output = run_sandboxed("/usr/bin/python3", &args, &config).unwrap();
        assert!(!output.status.success(), "large allocation escaped RLIMIT_AS");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn seccomp_blocks_network_socket_creation() {
        if !Path::new("/usr/bin/python3").exists() {
            return;
        }
        let root = root();
        let config = shell_config(&root);
        let args = [
            OsString::from("-S"),
            OsString::from("-c"),
            OsString::from("import socket; socket.socket()"),
        ];
        let output = run_sandboxed("/usr/bin/python3", &args, &config).unwrap();
        assert!(!output.status.success(), "seccomp allowed socket creation");
        assert_eq!(output.enforcement.seccomp, Enforcement::Enforced);
        let _ = std::fs::remove_dir_all(root);
    }
}
