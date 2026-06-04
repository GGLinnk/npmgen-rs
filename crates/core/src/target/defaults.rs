//! Platform mapping data consulted by [`Target`](super::Target): the default
//! key/triple set and the token tables that translate between Rust target
//! triples and npm `<os>-<cpu>` keys. Data only; the lookups are methods on
//! `Target`.

/// npm key -> default Rust target triple. Also defines the default platform set
/// (every entry) used when neither npmgen config nor cargo declares targets.
pub(super) const KEY_TRIPLES: &[(&str, &str)] = &[
    ("win32-x64", "x86_64-pc-windows-msvc"),
    ("win32-arm64", "aarch64-pc-windows-msvc"),
    ("darwin-x64", "x86_64-apple-darwin"),
    ("darwin-arm64", "aarch64-apple-darwin"),
    ("linux-x64", "x86_64-unknown-linux-gnu"),
    ("linux-arm64", "aarch64-unknown-linux-gnu"),
];

/// Triple arch token -> npm `process.arch` (`cpu`).
pub(super) const ARCH_CPU: &[(&str, &str)] = &[
    ("x86_64", "x64"),
    ("aarch64", "arm64"),
    ("i686", "ia32"),
    ("armv7", "arm"),
    ("riscv64gc", "riscv64"),
    ("powerpc64le", "ppc64"),
    ("s390x", "s390x"),
];

/// Triple system token and npm os value for Windows; the binary carries `.exe`.
pub(super) const WINDOWS_SYSTEM: &str = "windows";
pub(super) const WINDOWS_OS: &str = "win32";

/// Triple system token -> npm `process.platform` (`os`).
pub(super) const SYSTEM_OS: &[(&str, &str)] = &[
    (WINDOWS_SYSTEM, WINDOWS_OS),
    ("darwin", "darwin"),
    ("linux", "linux"),
    ("freebsd", "freebsd"),
    ("openbsd", "openbsd"),
    ("netbsd", "netbsd"),
    ("android", "android"),
];
