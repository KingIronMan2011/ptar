use std::{fs, io, path::Path};

pub fn file_mtime(_meta: &fs::Metadata) -> i64 { 0 }
pub fn file_mode(_meta: &fs::Metadata)  -> u32 { 0o644 }
pub fn file_uid(_meta: &fs::Metadata)   -> u32 { 0 }
pub fn file_gid(_meta: &fs::Metadata)   -> u32 { 0 }

pub fn create_symlink(_dest: &Path, _target: &str) -> io::Result<()> { Ok(()) }
