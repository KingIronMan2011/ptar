use std::path::Path;

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;
#[cfg(not(any(unix, windows)))]
mod other;

#[cfg(unix)]
pub use unix::*;
#[cfg(windows)]
pub use windows::*;
#[cfg(not(any(unix, windows)))]
pub use other::*;

/// Normalise path separators to `/` so archives are portable across OSes.
pub fn normalize_path(s: &str) -> String {
    s.replace('\\', "/")
}

/// Strip unsafe leading components before writing a path to disk during extraction:
///   - Unix absolute paths  (`/etc/passwd`)
///   - Windows absolute paths (`C:\Windows`, `\Windows`, `//server/share`)
///   - Backslash separators are converted to forward slashes first via the stored name.
pub fn safe_dest(name: &str) -> &Path {
    // Strip leading slashes / backslashes
    let s = name.trim_start_matches('/').trim_start_matches('\\');
    // Strip a Windows drive letter ("C:" or "c:")
    let s = if s.len() >= 2 && s.as_bytes()[1] == b':' {
        s[2..].trim_start_matches('/').trim_start_matches('\\')
    } else {
        s
    };
    Path::new(s)
}
