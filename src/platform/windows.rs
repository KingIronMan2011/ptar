use std::{fs, io, path::Path};
use std::os::windows::fs::MetadataExt;

/// Convert a Windows FILETIME (100-ns ticks since 1601-01-01) to a Unix timestamp.
pub fn file_mtime(meta: &fs::Metadata) -> i64 {
    const EPOCH_DIFF: u64 = 116_444_736_000_000_000; // ticks between 1601 and 1970
    let ft = meta.last_write_time();
    if ft >= EPOCH_DIFF { ((ft - EPOCH_DIFF) / 10_000_000) as i64 } else { 0 }
}

/// Windows has no Unix-style permission bits; store a sensible default.
pub fn file_mode(_meta: &fs::Metadata) -> u32 { 0o644 }
pub fn file_uid(_meta: &fs::Metadata)  -> u32 { 0 }
pub fn file_gid(_meta: &fs::Metadata)  -> u32 { 0 }

/// Windows symlinks require knowing whether the target is a file or directory.
pub fn create_symlink(dest: &Path, target: &str) -> io::Result<()> {
    let _ = fs::remove_file(dest);
    // Best-effort: check if the target path looks like a directory.
    let is_dir = Path::new(target).is_dir()
        || target.ends_with('/')
        || target.ends_with('\\');
    if is_dir {
        std::os::windows::fs::symlink_dir(target, dest)
    } else {
        std::os::windows::fs::symlink_file(target, dest)
    }
}
