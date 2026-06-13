use std::{
    fs::{self, File},
    io::{self, BufReader, BufWriter, Cursor, Read, Write},
    path::Path,
    sync::mpsc,
};

use rayon::iter::{ParallelBridge, ParallelIterator};
use xxhash_rust::xxh3::xxh3_64;

use crate::{
    cli::Cli,
    create::bytes_bar,
    format::{read_entries, read_u64, verify_magic, StoredEntry},
    hash::HashingWriter,
    platform,
};

const IN_BUF: usize = 4 * 1024 * 1024;

struct Blob {
    name:          String,
    data:          Vec<u8>,
    orig_size:     u64,
    checksum:      u64,
    orig_checksum: u64,
}

pub fn cmd_extract(cli: &Cli) -> io::Result<()> {
    let f      = File::open(&cli.file)?;
    let mut br = BufReader::with_capacity(IN_BUF, f);

    verify_magic(&mut br)?;
    let n       = read_u64(&mut br)?;
    let entries = read_entries(&mut br, n)?;

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

    // Pass 1: create directories.
    for e in &dirs {
        fs::create_dir_all(platform::safe_dest(&e.name))?;
        if cli.verbose { eprintln!("  {}/", e.name); }
    }

    // Pass 2: pipelined file extraction.
    let total_bytes: u64 = files.iter().map(|e| e.orig_size).sum();
    let pb        = bytes_bar(total_bytes, "extracting");
    let n_workers = rayon::current_num_threads();
    let verbose   = cli.verbose;

    let (tx, rx) = mpsc::sync_channel::<io::Result<Blob>>(n_workers * 2);

    let reader = std::thread::spawn(move || {
        for e in files {
            let result = read_blob(&mut br, e);
            let failed = result.is_err();
            if tx.send(result).is_err() || failed { break; }
        }
    });

    let pb2 = pb.clone();
    let extract_result = rx
        .into_iter()
        .par_bridge()
        .try_for_each(move |item: io::Result<Blob>| -> io::Result<()> {
            let blob      = item?;
            let orig_size = blob.orig_size;
            let dest_str  = blob.name.clone();
            let dest      = platform::safe_dest(&dest_str);
            if let Some(p) = dest.parent() { fs::create_dir_all(p)?; }
            if verbose { pb2.println(format!("  {}", blob.name)); }
            write_file(dest, blob)?;
            pb2.inc(orig_size);
            Ok(())
        });

    reader.join().expect("reader thread panicked");
    pb.finish_and_clear();
    extract_result?;

    // Pass 3: symlinks (must come after files).
    for e in &links {
        let dest = platform::safe_dest(&e.name);
        if let Some(p) = dest.parent() { fs::create_dir_all(p)?; }
        platform::create_symlink(dest, &e.link)?;
        if cli.verbose { eprintln!("  {} -> {}", e.name, e.link); }
    }

    Ok(())
}

fn read_blob(r: &mut impl Read, e: StoredEntry) -> io::Result<Blob> {
    if e.data_size == 0 {
        return Ok(Blob {
            name: e.name, data: Vec::new(),
            orig_size: 0, checksum: 0, orig_checksum: 0,
        });
    }
    let mut data = vec![0u8; e.data_size as usize];
    r.read_exact(&mut data)?;
    Ok(Blob {
        name:          e.name,
        data,
        orig_size:     e.orig_size,
        checksum:      e.checksum,
        orig_checksum: e.orig_checksum,
    })
}

fn write_file(dest: &Path, blob: Blob) -> io::Result<()> {
    if blob.data.is_empty() {
        return File::create(dest).map(|_| ());
    }

    // Verify compressed blob integrity before touching the filesystem.
    let actual = xxh3_64(&blob.data);
    if actual != blob.checksum {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "checksum mismatch for '{}': expected {:#018x}, got {:#018x}",
                blob.name, blob.checksum, actual
            ),
        ));
    }

    // Stream decompression to disk, hashing the output to verify content integrity.
    let mut dec    = zstd::Decoder::new(Cursor::new(&blob.data))?;
    let raw_out    = BufWriter::with_capacity(256 * 1024, File::create(dest)?);
    let mut hw     = HashingWriter::new(raw_out);
    io::copy(&mut dec, &mut hw)?;
    hw.inner.flush()?;
    let actual_orig = hw.finish();

    if blob.orig_checksum != 0 && actual_orig != blob.orig_checksum {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "content checksum mismatch for '{}': expected {:#018x}, got {:#018x}",
                blob.name, blob.orig_checksum, actual_orig
            ),
        ));
    }
    Ok(())
}
