use std::{fs, path::PathBuf};

use jwalk::WalkDir;

use crate::platform;

pub struct Entry {
    pub path:        PathBuf,
    pub display:     String,
    pub size:        u64,
    pub is_file:     bool,
    pub is_dir:      bool,
    pub is_symlink:  bool,
    pub link_target: Option<PathBuf>,
    pub mode:        u32,
    pub uid:         u32,
    pub gid:         u32,
    pub mtime:       i64,
}

pub fn collect_entries(paths: &[String]) -> Vec<Entry> {
    let mut entries = Vec::new();

    for root in paths {
        for de in WalkDir::new(root)
            .sort(true)
            .skip_hidden(false)
            .follow_links(false)
        {
            let Ok(de)   = de                          else { continue };
            let Ok(meta) = de.path().symlink_metadata() else { continue };

            let is_symlink  = meta.file_type().is_symlink();
            let link_target = if is_symlink { fs::read_link(de.path()).ok() } else { None };

            let path_buf = de.path();
            let raw      = path_buf.to_string_lossy();

            // Normalise separators to '/' and strip leading './' or '/' so archive
            // paths are relative and portable on both Windows and Unix.
            let display = platform::normalize_path(
                raw.trim_start_matches("./")
                   .trim_start_matches('/')
                   .trim_start_matches(".\\")
                   .trim_start_matches('\\'),
            );

            entries.push(Entry {
                path: path_buf.to_path_buf(),
                display,
                size:      if meta.is_file() { meta.len() } else { 0 },
                is_file:   meta.is_file(),
                is_dir:    meta.is_dir(),
                is_symlink,
                link_target,
                mode:  platform::file_mode(&meta),
                uid:   platform::file_uid(&meta),
                gid:   platform::file_gid(&meta),
                mtime: platform::file_mtime(&meta),
            });
        }
    }

    entries
}
