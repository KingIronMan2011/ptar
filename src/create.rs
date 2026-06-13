use std::{
    fs::File,
    io::{self, BufReader, BufWriter, Seek, SeekFrom, Write},
    time::Instant,
};

use rayon::prelude::*;

use crate::{
    cli::Cli,
    entry::{collect_entries, Entry},
    format::{record_size, write_entry, StoredEntry, MAGIC},
};

const OUT_BUF: usize = 4 * 1024 * 1024;

pub fn cmd_create(cli: &Cli) -> io::Result<()> {
    if cli.paths.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "no paths specified"));
    }

    let t0 = Instant::now();
    let entries = collect_entries(&cli.paths);
    eprintln!("[ptar] scanned {} entries  ({:.2}s)", entries.len(), t0.elapsed().as_secs_f64());

    // Compute the header size upfront — it depends only on names, not file content.
    let header_size: u64 = (8 + 8  // magic + n_entries
        + entries.iter().map(|e| record_size(&e.display, &link_of(e))).sum::<usize>())
        as u64;

    // Open output and jump straight to the data section.
    // The header region (bytes 0..header_size) stays as a sparse hole until we fill it at
    // the end — this avoids writing any placeholder bytes.
    let mut out = File::create(&cli.file)?;
    out.seek(SeekFrom::Start(header_size))?;
    let mut bw = BufWriter::with_capacity(OUT_BUF, out);

    // Process files in chunks of n_threads: each chunk is compressed in parallel,
    // then written sequentially. At most n_threads compressed blobs live in RAM at once.
    let n = rayon::current_num_threads();
    let file_indices: Vec<usize> = entries.iter().enumerate()
        .filter(|(_, e)| e.is_file && e.size > 0)
        .map(|(i, _)| i)
        .collect();

    let mut data_sizes = vec![0u64; entries.len()]; // compressed size per entry index
    let level = cli.level;
    let t1 = Instant::now();
    let mut n_compressed = 0usize;

    for chunk in file_indices.chunks(n) {
        let blobs: io::Result<Vec<Vec<u8>>> = chunk.par_iter()
            .map(|&i| compress_entry(&entries[i], level))
            .collect();
        let blobs = blobs?;

        for (&entry_idx, blob) in chunk.iter().zip(blobs.iter()) {
            data_sizes[entry_idx] = blob.len() as u64;
            if cli.verbose { eprintln!("  {}", entries[entry_idx].display); }
            bw.write_all(blob)?;
            n_compressed += 1;
        }
    }

    eprintln!(
        "[ptar] compressed {n_compressed} files in parallel  ({:.2}s)",
        t1.elapsed().as_secs_f64()
    );

    // Flush and reclaim the File so we can seek back.
    bw.flush()?;
    let mut out = bw.into_inner().unwrap(); // safe: already flushed

    // Build the entry table now that we know every compressed size.
    let mut offset = header_size;
    let stored: Vec<StoredEntry> = entries.iter().enumerate().map(|(i, e)| {
        let data_size = data_sizes[i];
        let se = StoredEntry {
            name:        e.display.clone(),
            kind:        if e.is_dir { 1 } else if e.is_symlink { 2 } else { 0 },
            mode:        e.mode,
            uid:         e.uid,
            gid:         e.gid,
            mtime:       e.mtime,
            data_offset: offset,
            data_size,
            orig_size:   e.size,
            link:        link_of(e),
        };
        offset += data_size;
        se
    }).collect();

    // Seek to the front and write the real header over the sparse region.
    out.seek(SeekFrom::Start(0))?;
    let mut bw = BufWriter::with_capacity(OUT_BUF, out);
    bw.write_all(MAGIC)?;
    bw.write_all(&(stored.len() as u64).to_le_bytes())?;
    for se in &stored {
        write_entry(&mut bw, se)?;
    }
    bw.flush()?;

    eprintln!("[ptar] done — {:.2}s total", t0.elapsed().as_secs_f64());
    Ok(())
}

fn compress_entry(e: &Entry, level: i32) -> io::Result<Vec<u8>> {
    let f  = File::open(&e.path)?;
    let br = BufReader::with_capacity(256 * 1024, f);
    zstd::encode_all(br, level)
}

fn link_of(e: &Entry) -> String {
    e.link_target.as_ref()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default()
}
