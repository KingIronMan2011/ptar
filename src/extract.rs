use std::{
    fs::{self, File},
    io::{self, BufReader, BufWriter, Cursor, Read},
    path::Path,
    sync::mpsc,
};

use rayon::iter::{ParallelBridge, ParallelIterator};

use crate::{
    cli::Cli,
    format::{read_entries, read_u64, verify_magic, StoredEntry},
};

const IN_BUF: usize = 4 * 1024 * 1024;

pub fn cmd_extract(cli: &Cli) -> io::Result<()> {
    let f  = File::open(&cli.file)?;
    let mut br = BufReader::with_capacity(IN_BUF, f);

    verify_magic(&mut br)?;
    let n = read_u64(&mut br)?;
    let entries = read_entries(&mut br, n)?;

    // Partition by kind so each pass operates on exactly the right set.
    let mut dirs  = Vec::new();
    let mut files = Vec::new();
    let mut links = Vec::new();
    for e in entries {
        match e.kind {
            0 => files.push(e),
            1 => dirs.push(e),
            _ => links.push(e),
        }
    }

    // Pass 1 (sequential): create all directories before any file writer touches them.
    for e in &dirs {
        fs::create_dir_all(safe_path(&e.name))?;
        if cli.verbose { eprintln!("  {}/", e.name); }
    }

    // Pass 2 (pipelined): one reader thread feeds a bounded channel; rayon workers
    // decompress and write files in parallel as blobs arrive — I/O and CPU overlap.
    let n_workers = rayon::current_num_threads();
    let verbose   = cli.verbose;

    let (tx, rx) = mpsc::sync_channel::<io::Result<(String, Vec<u8>)>>(n_workers * 2);

    let reader = std::thread::spawn(move || {
        for e in files {
            let result = read_blob(&mut br, e);
            let failed = result.is_err();
            if tx.send(result).is_err() || failed {
                break; // workers dropped rx (their error) or we hit a read error
            }
        }
    });

    let extract_result = rx
        .into_iter()
        .par_bridge()
        .try_for_each(|item: io::Result<(String, Vec<u8>)>| -> io::Result<()> {
            let (name, blob) = item?;
            let dest = safe_path(&name);
            if let Some(p) = dest.parent() { fs::create_dir_all(p)?; }
            if verbose { eprintln!("  {name}"); }
            write_file(dest, &blob)
        });

    // Always join — if workers errored early they dropped rx, which unblocks the reader.
    reader.join().expect("reader thread panicked");
    extract_result?;

    // Pass 3 (sequential): symlinks — usually few, must come after files are written.
    for e in &links {
        let dest = safe_path(&e.name);
        if let Some(p) = dest.parent() { fs::create_dir_all(p)?; }
        create_symlink(dest, &e.link)?;
        if cli.verbose { eprintln!("  {} -> {}", e.name, e.link); }
    }

    Ok(())
}

fn read_blob(r: &mut impl Read, e: StoredEntry) -> io::Result<(String, Vec<u8>)> {
    if e.data_size == 0 {
        return Ok((e.name, Vec::new()));
    }
    let mut buf = vec![0u8; e.data_size as usize];
    r.read_exact(&mut buf)?;
    Ok((e.name, buf))
}

fn write_file(dest: &Path, blob: &[u8]) -> io::Result<()> {
    if blob.is_empty() {
        return File::create(dest).map(|_| ());
    }
    // Stream decompression directly to disk — no fully-decompressed Vec ever lives in RAM.
    let mut dec = zstd::Decoder::new(Cursor::new(blob))?;
    let mut out = BufWriter::with_capacity(256 * 1024, File::create(dest)?);
    io::copy(&mut dec, &mut out)?;
    out.flush()
}

fn safe_path(name: &str) -> &Path {
    Path::new(name.trim_start_matches('/'))
}

fn create_symlink(dest: &Path, target: &str) -> io::Result<()> {
    #[cfg(unix)] {
        use std::os::unix::fs::symlink;
        let _ = fs::remove_file(dest);
        symlink(target, dest)?;
    }
    #[cfg(not(unix))]
    let _ = (dest, target);
    Ok(())
}
