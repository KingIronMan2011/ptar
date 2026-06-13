use std::{fs, path::PathBuf};

use jwalk::WalkDir;

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
            let display  = raw
                .trim_start_matches("./")
                .trim_start_matches('/')
                .to_owned();

            #[cfg(unix)]
            let (mode, uid, gid, mtime) = {
                use std::os::unix::fs::{MetadataExt, PermissionsExt};
                (meta.permissions().mode(), meta.uid(), meta.gid(), meta.mtime())
            };
            #[cfg(not(unix))]
            let (mode, uid, gid, mtime) = (0o644u32, 0u32, 0u32, 0i64);

            entries.push(Entry {
                path: de.path().to_path_buf(),
                display,
                size:      if meta.is_file() { meta.len() } else { 0 },
                is_file:   meta.is_file(),
                is_dir:    meta.is_dir(),
                is_symlink,
                link_target,
                mode,
                uid,
                gid,
                mtime,
            });
        }
    }

    entries
}
