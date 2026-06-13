use std::{fs, io, path::Path};
use std::os::unix::fs::{MetadataExt, PermissionsExt};

pub fn file_mtime(meta: &fs::Metadata) -> i64 { meta.mtime() }
pub fn file_mode(meta: &fs::Metadata)  -> u32  { meta.permissions().mode() }
pub fn file_uid(meta: &fs::Metadata)   -> u32  { meta.uid() }
pub fn file_gid(meta: &fs::Metadata)   -> u32  { meta.gid() }

pub fn create_symlink(dest: &Path, target: &str) -> io::Result<()> {
    use std::os::unix::fs::symlink;
    let _ = fs::remove_file(dest);
    symlink(target, dest)
}
