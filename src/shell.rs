//! Windows Explorer context-menu entry points.
//!
//! The verbs registered by [`crate::install`] launch `ptar.exe` with one of the
//! `--shell-*` flags handled here. Each flag is translated into the equivalent
//! regular command line (after switching to the right working directory), so
//! the normal create/extract code paths do the actual work.

use std::{
    env, fs, io,
    path::{Path, PathBuf},
};

pub const EXTRACT_HERE: &str = "--shell-extract-here";
pub const EXTRACT_TO: &str = "--shell-extract-to";
pub const CREATE: &str = "--shell-create";

/// `true` if the args are an Explorer-launched invocation.
pub fn is_shell(args: &[String]) -> bool {
    matches!(args.get(1).map(String::as_str), Some(EXTRACT_HERE | EXTRACT_TO | CREATE))
}

/// Translate a `--shell-*` command line into a normal ptar command line.
pub fn translate(args: &[String]) -> io::Result<Vec<String>> {
    let (mode, target) = match (args.get(1), args.get(2)) {
        (Some(m), Some(t)) => (m.as_str(), PathBuf::from(t)),
        _ => return Err(io::Error::other("missing path argument")),
    };
    let target = fs::canonicalize(&target)?;
    let parent = target
        .parent()
        .ok_or_else(|| io::Error::other("path has no parent directory"))?
        .to_path_buf();
    let name = file_name(&target)?;

    match mode {
        EXTRACT_HERE => {
            env::set_current_dir(&parent)?;
            Ok(extract_args(&target))
        }
        EXTRACT_TO => {
            let dir = unique_path(&parent, &stem(&name), "");
            fs::create_dir_all(&dir)?;
            env::set_current_dir(&dir)?;
            Ok(extract_args(&target))
        }
        CREATE => {
            // Work from the parent so stored names are relative (just `name/...`).
            env::set_current_dir(&parent)?;
            let base = stem_if_file(&target, &name);
            let out = unique_path(&parent, &base, ".ptar");
            Ok(vec![
                "ptar".into(),
                "-c".into(),
                "-f".into(),
                out.to_string_lossy().into_owned(),
                name,
            ])
        }
        _ => Err(io::Error::other("unknown shell command")),
    }
}

fn extract_args(archive: &Path) -> Vec<String> {
    // `canonicalize` yields `\\?\` paths on Windows; strip it for friendlier output.
    let s = archive.to_string_lossy();
    let s = s.strip_prefix(r"\\?\").unwrap_or(&s).to_string();
    vec!["ptar".into(), "-x".into(), "-f".into(), s]
}

fn file_name(p: &Path) -> io::Result<String> {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or_else(|| io::Error::other("path has no file name"))
}

fn stem(name: &str) -> String {
    Path::new(name)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| name.to_string())
}

/// `photo.jpg` -> `photo`, directory `src` -> `src`.
fn stem_if_file(p: &Path, name: &str) -> String {
    if p.is_dir() { name.to_string() } else { stem(name) }
}

/// `dir/base<ext>`, or `dir/base (2)<ext>`, ... if it already exists.
fn unique_path(dir: &Path, base: &str, ext: &str) -> PathBuf {
    let first = dir.join(format!("{base}{ext}"));
    if !first.exists() {
        return first;
    }
    (2..)
        .map(|n| dir.join(format!("{base} ({n}){ext}")))
        .find(|p| !p.exists())
        .unwrap()
}

