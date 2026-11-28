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
    /// Allow the vfork syscall only for trusted compiler/linker invocations.
    /// Generated code must run with the default strict policy.
    pub allow_vfork_for_toolchain: bool,
    /// Permit fork only for trusted compiler/linker invocations.
    pub allow_fork_for_toolchain: bool,
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
            allow_vfork_for_toolchain: false,
            allow_fork_for_toolchain: false,
        }
    }

    pub fn allow_readonly_path(&mut self, path: impl Into<PathBuf>) {
        self.readonly_paths.push(path.into());
    }

    pub fn allow_vfork_for_toolchain(&mut self, allowed: bool) {
        self.allow_vfork_for_toolchain = allowed;
    }

    pub fn allow_fork_for_toolchain(&mut self, allowed: bool) {
        self.allow_fork_for_toolchain = allowed;
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
    let seccomp = build_seccomp_filter(
        config.allow_vfork_for_toolchain,
        config.allow_fork_for_toolchain,
    );
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
    // Verification needs ordinary files/directories/symlinks in scratch, but never device nodes or Unix sockets.
    const MAKE_CHAR: u64 = 1 << 6;
    const MAKE_SOCK: u64 = 1 << 9;
    const MAKE_BLOCK: u64 = 1 << 11;
    let scratch_access = security.handled_access & !(MAKE_CHAR | MAKE_SOCK | MAKE_BLOCK);
    if let Err(error) = add_landlock_path_rule(ruleset_fd, &security.scratch, scratch_access) {
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
fn build_seccomp_filter(allow_vfork: bool, allow_fork: bool) -> Vec<libc::sock_filter> {
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
    // glibc posix_spawn's vfork-style clone. Match the complete flags word,
    // including SIGCHLD, so this cannot be widened by extra clone capabilities.
    const POSIX_SPAWN_FLAGS: u32 = 0x0000_4111; // CLONE_VM | CLONE_VFORK | SIGCHLD
    const CLONE_FORK_FLAGS: u32 = 0x0000_0011; // SIGCHLD, allowed only in trusted compiler mode
    // glibc may set this to return the child PID without widening process semantics.
    const POSIX_SPAWN_PARENT_TID_FLAGS: u32 = POSIX_SPAWN_FLAGS | 0x0010_0000;
    // Rust/glibc pthread flags. Reject all bits outside this mask, especially
    // namespace, parent, vfork and pidfd flags.
    const THREAD_ALLOWED_FLAGS: u32 = 0x013d_0f00;

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
        // Report clone3 as unavailable so libc can fall back to clone(2), where
        // the exact posix_spawn flag word is checked below.
        code.push(jump_eq(libc::SYS_clone3 as u32, 0, 1));
        code.push(ins(BPF_RET | BPF_K, 0, 0, SECCOMP_RET_ERRNO | libc::ENOSYS as u32));
        // fork/vfork are denied for untrusted programs. Trusted rustc/linker
        // mode may use them for process launching; descendants inherit this filter.
        if !allow_fork {
            code.push(jump_eq(libc::SYS_fork as u32, 0, 1));
            code.push(ins(BPF_RET | BPF_K, 0, 0, SECCOMP_RET_ERRNO | libc::EPERM as u32));
        }
        if !allow_vfork {
            code.push(jump_eq(libc::SYS_vfork as u32, 0, 1));
            code.push(ins(BPF_RET | BPF_K, 0, 0, SECCOMP_RET_ERRNO | libc::EPERM as u32));
        }
        let denied: &[libc::c_long] = &[
            libc::SYS_socket,
            libc::SYS_socketpair,
            libc::SYS_sendto,
            libc::SYS_recvfrom,
            libc::SYS_sendmsg,
            libc::SYS_recvmsg,
            libc::SYS_recvmmsg,
            libc::SYS_sendmmsg,
            libc::SYS_shutdown,
            libc::SYS_getsockopt,
            libc::SYS_setsockopt,
            libc::SYS_getsockname,
            libc::SYS_getpeername,
            libc::SYS_io_uring_setup,
            libc::SYS_io_uring_enter,
            libc::SYS_io_uring_register,
            libc::SYS_splice,
            libc::SYS_tee,
            libc::SYS_vmsplice,
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
        ];
        for syscall_nr in denied {
            code.push(jump_eq(*syscall_nr as u32, 0, 1));
            code.push(ins(BPF_RET | BPF_K, 0, 0, SECCOMP_RET_ERRNO | libc::EPERM as u32));
        }
        // Permit only the exact vfork-style clone used by posix_spawn, plus
        // supported thread-only clones. fork and clone3 remain denied; raw
        // SYS_vfork is denied by default and allowed only in compiler mode.
        // The posix_spawn child inherits Landlock, no_new_privs, seccomp and
        // all rlimits. Unknown and namespace/process flags are rejected.
        // Use calculated branch offsets for the two compiler-only spawn words;
        // this keeps the BPF jumps correct as the strict thread filter evolves.
        let clone_jump = code.len();
        code.push(jump_eq(libc::SYS_clone as u32, 0, 0));
        let mut spawn_jumps = Vec::new();
        if allow_vfork {
            code.push(ins(BPF_LD | BPF_W | BPF_ABS, 0, 0, OFF_ARG0));
            spawn_jumps.push(code.len());
            code.push(jump_eq(POSIX_SPAWN_FLAGS, 0, 0));
            code.push(ins(BPF_LD | BPF_W | BPF_ABS, 0, 0, OFF_ARG0));
            spawn_jumps.push(code.len());
            code.push(jump_eq(POSIX_SPAWN_PARENT_TID_FLAGS, 0, 0));
        }
        if allow_fork {
            code.push(ins(BPF_LD | BPF_W | BPF_ABS, 0, 0, OFF_ARG0));
            spawn_jumps.push(code.len());
            code.push(jump_eq(CLONE_FORK_FLAGS, 0, 0));
        }
        code.push(ins(BPF_LD | BPF_W | BPF_ABS, 0, 0, OFF_ARG0));
        code.push(ins(BPF_ALU | BPF_AND | BPF_K, 0, 0, THREAD_FLAGS));
        code.push(jump_eq(THREAD_FLAGS, 1, 0));
        code.push(ins(BPF_RET | BPF_K, 0, 0, SECCOMP_RET_ERRNO | libc::EPERM as u32));
        code.push(ins(BPF_LD | BPF_W | BPF_ABS, 0, 0, OFF_ARG0));
        code.push(ins(BPF_ALU | BPF_AND | BPF_K, 0, 0, !THREAD_ALLOWED_FLAGS));
        code.push(jump_eq(0, 1, 0));
        code.push(ins(BPF_RET | BPF_K, 0, 0, SECCOMP_RET_ERRNO | libc::EPERM as u32));
        // Exact posix-spawn words and supported thread flags reach this allow.
        let clone_allow = code.len();
        code.push(ins(BPF_RET | BPF_K, 0, 0, SECCOMP_RET_ALLOW));
        // Default allow for syscalls not covered by the deny list or clone rule.
        let default_allow = code.len();
        code.push(ins(BPF_RET | BPF_K, 0, 0, SECCOMP_RET_ALLOW));
        code[clone_jump].jf = (default_allow - clone_jump - 1) as u8;
        for jump in spawn_jumps {
            code[jump].jt = (clone_allow - jump - 1) as u8;
        }
        code
    }
}

#[cfg(not(target_os = "linux"))]
fn build_seccomp_filter(_allow_vfork: bool, _allow_fork: bool) -> Vec<libc::sock_filter> {
    Vec::new()
}

#[cfg(target_os = "linux")]
unsafe fn close_inherited_fds() -> io::Result<()> {
    const SYS_CLOSE_RANGE: libc::c_long = 436;
    let result = libc::syscall(SYS_CLOSE_RANGE, 3u32, u32::MAX, 0u32);
    if result == 0 {
        return Ok(());
    }
    let error = io::Error::last_os_error();
    if !matches!(error.raw_os_error(), Some(libc::ENOSYS) | Some(libc::EINVAL)) {
        return Err(error);
    }

    // Older kernels have no close_range syscall. Close descriptors one by one;
    // this path is only used on those older kernels and preserves stdin/out/err.
    let mut limit = libc::rlimit { rlim_cur: 65_536, rlim_max: 65_536 };
    if libc::getrlimit(libc::RLIMIT_NOFILE, &mut limit) != 0 {
        return Err(io::Error::last_os_error());
    }
    let max_fd = limit.rlim_cur.min(1_048_576) as libc::c_int;
    for fd in 3..max_fd {
        libc::close(fd);
    }
    Ok(())
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
        // rustc, its linker, and many language runtimes use TMPDIR for transient files.
        // Keep those writes inside the Landlock-authorized scratch tree.
        .env("TMPDIR", &config.scratch_dir)
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
                close_inherited_fds()?;
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

    #[cfg(target_os = "linux")]
    #[test]
    fn inherited_network_descriptors_are_closed_before_exec() {
        use std::os::unix::io::AsRawFd;
        use std::os::unix::net::UnixStream;
        let root = root();
        let config = shell_config(&root);
        let (mut receiver, inherited) = UnixStream::pair().unwrap();
        let fd = inherited.as_raw_fd();
        assert_eq!(unsafe { libc::fcntl(fd, libc::F_SETFD, 0) }, 0);
        let script = format!("import os; os.write({}, b'x')", fd);
        let args = [
            OsString::from("-S"),
            OsString::from("-c"),
            OsString::from(script),
        ];
        let output = run_sandboxed("/usr/bin/python3", &args, &config).unwrap();
        assert!(!output.status.success(), "child retained an inherited socket descriptor");
        drop(inherited);
        let mut received = [0u8; 1];
        assert_eq!(receiver.read(&mut received).unwrap_or(0), 0);
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



    #[cfg(target_os = "linux")]
    #[test]
    fn seccomp_rejects_extra_clone_process_flags() {
        if !Path::new("/usr/bin/python3").exists() {
            return;
        }
        let root = root();
        let config = shell_config(&root);
        let flags = 0x0001_0900u32 | 0x0000_1000u32; // pthread flags + CLONE_PIDFD
        let spawn_flags = 0x0000_4111u32; // CLONE_VM | CLONE_VFORK | SIGCHLD
        let script = format!(
            "import ctypes, errno; libc=ctypes.CDLL(None, use_errno=True); clone={}; flags=[{}, {}]; results=[]; exec('for f in flags:\\n ctypes.set_errno(0); r=libc.syscall(clone, f, 0, 0, 0, 0); e=ctypes.get_errno(); results.append(e); assert r == -1 and e == errno.EPERM'); print(*results)",
            libc::SYS_clone,
            flags,
            spawn_flags,
        );
        let args = [
            OsString::from("-S"),
            OsString::from("-c"),
            OsString::from(script),
        ];
        let output = run_sandboxed("/usr/bin/python3", &args, &config).unwrap();
        assert!(
            output.status.success(),
            "seccomp did not reject CLONE_PIDFD: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        assert!(String::from_utf8_lossy(&output.stdout).trim().ends_with(&libc::EPERM.to_string()));
        let _ = std::fs::remove_dir_all(root);
    }




    #[cfg(target_os = "linux")]
    #[test]
    fn compiler_filter_allows_only_exact_spawn_clone_flags() {
        if !Path::new("/usr/bin/python3").exists() {
            return;
        }
        let root = root();
        let mut config = shell_config(&root);
        config.allow_vfork_for_toolchain(true);
        config.allow_fork_for_toolchain(true);
        let spawn_flags = 0x0000_4111u32; // CLONE_VM | CLONE_VFORK | SIGCHLD
        let parent_tid_flags = spawn_flags | 0x0010_0000u32; // + CLONE_PARENT_SETTID
        let namespace_flags = spawn_flags | 0x0002_0000u32; // + CLONE_NEWNS
        let script = format!(
            "import ctypes, errno; libc=ctypes.CDLL(None, use_errno=True); clone={}; cases=[({}, errno.EINVAL), ({}, None), ({}, errno.EPERM)]; exec('for flags, expected in cases:\\n ctypes.set_errno(0); result=libc.syscall(clone, flags, 0, 0, 0, 0); actual=ctypes.get_errno(); print('CASE', flags, result, actual, expected, flush=True); assert result == -1; assert actual != errno.EPERM if expected is None else actual == expected'); print('SANDBOX_CLONE_FLAGS=pass')",
            libc::SYS_clone,
            spawn_flags,
            parent_tid_flags,
            namespace_flags,
        );
        let args = [
            OsString::from("-S"),
            OsString::from("-c"),
            OsString::from(script),
        ];
        let output = run_sandboxed("/usr/bin/python3", &args, &config).unwrap();
        assert!(
            output.status.success(),
            "compiler clone policy mismatch: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("SANDBOX_CLONE_FLAGS=pass"));
        let _ = std::fs::remove_dir_all(root);
    }


    #[cfg(target_os = "linux")]
    #[test]
    fn compiler_profile_rejects_unapproved_clone_flags() {
        if !Path::new("/usr/bin/python3").exists() {
            return;
        }
        let root = root();
        let mut config = shell_config(&root);
        config.allow_vfork_for_toolchain(true);
        config.allow_fork_for_toolchain(true);
        let bad_flags = 0x0000_0011u32 | 0x0000_1000u32; // SIGCHLD | CLONE_PIDFD
        let script = format!(
            "import ctypes, errno; libc=ctypes.CDLL(None, use_errno=True); r=libc.syscall({}, {}, 0, 0, 0, 0); e=ctypes.get_errno(); print(e); assert r == -1 and e == errno.EPERM",
            libc::SYS_clone,
            bad_flags,
        );
        let args = [OsString::from("-S"), OsString::from("-c"), OsString::from(script)];
        let output = run_sandboxed("/usr/bin/python3", &args, &config).unwrap();
        assert!(output.status.success(), "bad clone flags were not denied: {}", String::from_utf8_lossy(&output.stderr));
        assert!(String::from_utf8_lossy(&output.stdout).trim().ends_with(&libc::EPERM.to_string()));
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn compiler_profile_allows_exact_sigchld_clone_and_waits_for_child() {
        if !Path::new("/usr/bin/python3").exists() {
            return;
        }
        let root = root();
        let mut config = shell_config(&root);
        config.allow_fork_for_toolchain(true);
        let script = format!(
            "import ctypes, os; libc=ctypes.CDLL(None); pid=libc.syscall({}, 0x11, 0, 0, 0, 0); assert pid >= 0, 'clone(SIGCHLD) failed'; os._exit(0) if pid == 0 else None; waited, status=os.waitpid(pid, 0); assert waited == pid and status == 0; print('SANDBOX_SIGCHLD_CLONE=pass')",
            libc::SYS_clone,
        );
        let args = [OsString::from("-S"), OsString::from("-c"), OsString::from(script)];
        let output = run_sandboxed("/usr/bin/python3", &args, &config).unwrap();
        assert!(
            output.status.success() && String::from_utf8_lossy(&output.stdout).contains("SANDBOX_SIGCHLD_CLONE=pass"),
            "compiler profile failed clone(SIGCHLD): status={:?} stdout={} stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn compiler_profile_can_spawn_cc() {
        if !Path::new("/usr/bin/python3").exists() {
            return;
        }
        let root = root();
        let mut config = shell_config(&root);
        config.allow_vfork_for_toolchain(true);
        config.allow_fork_for_toolchain(true);
        let args = [
            OsString::from("-S"),
            OsString::from("-c"),
            OsString::from("import subprocess; p=subprocess.run(['cc','--version'],check=True,capture_output=True,text=True); print('SANDBOX_CC_SPAWN=pass '+p.stdout.splitlines()[0])"),
        ];
        let output = run_sandboxed("/usr/bin/python3", &args, &config).unwrap();
        assert!(
            output.status.success() && !output.timed_out,
            "compiler-profile cc spawn failed: status={:?} stdout={} stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("SANDBOX_CC_SPAWN=pass"),
            "spawn probe marker missing: {}",
            String::from_utf8_lossy(&output.stdout),
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn strict_default_blocks_fork() {
        if !Path::new("/usr/bin/python3").exists() {
            return;
        }
        let root = root();
        let config = shell_config(&root);
        let args = [
            OsString::from("-S"),
            OsString::from("-c"),
            OsString::from("import os; os.fork()"),
        ];
        let output = run_sandboxed("/usr/bin/python3", &args, &config).unwrap();
        assert!(!output.status.success(), "strict sandbox allowed fork");
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn rustc_test_binary_compiles_and_runs_inside_scratch() {
        use std::path::PathBuf;

        let root = root();
        let scratch = root.join("scratch");
        let source = scratch.join("tiny.rs");
        let binary = scratch.join("tiny-tests");
        let mut config = SandboxConfig::new(&scratch);
        config.cpu_seconds = 90;
        config.address_space_bytes = 4 * 1024 * 1024 * 1024;
        config.file_size_bytes = 128 * 1024 * 1024;
        config.wall_timeout = Duration::from_secs(120);
        config.allow_vfork_for_toolchain(true);
        config.allow_fork_for_toolchain(true);

        if let Some(home) = std::env::var_os("HOME") {
            let home = PathBuf::from(home);
            for tool_dir in [home.join(".rustup"), home.join(".cargo")] {
                config.allow_readonly_path(tool_dir);
            }
        }
        if let Some(rustup_home) = std::env::var_os("RUSTUP_HOME") {
            config.allow_readonly_path(PathBuf::from(rustup_home));
        }
        if let Some(cargo_home) = std::env::var_os("CARGO_HOME") {
            config.allow_readonly_path(PathBuf::from(cargo_home));
        }
        let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| OsString::from("rustc"));
        std::fs::create_dir_all(&scratch).unwrap();
        std::fs::write(
            &source,
            "#[test] fn sandboxed_test_runs() { assert_eq!(2 + 2, 4); } #[test] fn sandboxed_thread_runs() { assert_eq!(std::thread::spawn(|| 7).join().unwrap(), 7); }",
        )
        .unwrap();

        let cc_args = [OsString::from("--version")];
        let cc_probe = run_sandboxed("cc", &cc_args, &config).unwrap();
        assert!(
            cc_probe.status.success() && !cc_probe.timed_out,
            "sandboxed cc --version probe failed: status={:?} stderr={}",
            cc_probe.status,
            String::from_utf8_lossy(&cc_probe.stderr),
        );
        println!("SANDBOX_CC_DIRECT=pass");

        let compile_args = [
            OsString::from("--edition=2021"),
            OsString::from("--test"),
            OsString::from("--crate-name"),
            OsString::from("air_sandbox_linker_smoke"),
            source.as_os_str().to_owned(),
            OsString::from("-o"),
            binary.as_os_str().to_owned(),
        ];
        let compiled = run_sandboxed(&rustc, &compile_args, &config).unwrap();
        assert!(
            compiled.status.success() && !compiled.timed_out,
            "sandboxed rustc/linker failed: status={:?} timed_out={} stdout={} stderr={}",
            compiled.status,
            compiled.timed_out,
            String::from_utf8_lossy(&compiled.stdout),
            String::from_utf8_lossy(&compiled.stderr),
        );
        assert!(binary.exists(), "rustc output was not created in scratch");

        let mut runtime_config = config.clone();
        runtime_config.allow_vfork_for_toolchain(false);
        runtime_config.allow_fork_for_toolchain(false);
        let ran = run_sandboxed(&binary, &[], &runtime_config).unwrap();
        assert!(
            ran.status.success() && !ran.timed_out,
            "sandboxed rust test binary failed: status={:?} timed_out={} stdout={} stderr={}",
            ran.status,
            ran.timed_out,
            String::from_utf8_lossy(&ran.stdout),
            String::from_utf8_lossy(&ran.stderr),
        );
        assert!(
            String::from_utf8_lossy(&ran.stdout).contains("2 passed"),
            "expected two passing tests, got: {}",
            String::from_utf8_lossy(&ran.stdout),
        );

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
