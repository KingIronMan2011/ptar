use std::{
    collections::HashMap,
    fs::File,
    io::{self, BufReader, BufWriter, Read, Seek, SeekFrom, Write},
    path::Path,
    time::Instant,
};

use rayon::prelude::*;
use xxhash_rust::xxh3::xxh3_64;

use crate::{
    cli::Cli,
    create::{bytes_bar, compress_entry},
    entry::collect_entries,
    format::{read_entries, read_u64, record_size, verify_magic, write_entry, StoredEntry, MAGIC},
};

const OUT_BUF: usize = 4 * 1024 * 1024;
const IN_BUF:  usize = 4 * 1024 * 1024;

// What data will populate an entry's blob in the new archive.
enum BlobSrc {
    // Blob is identical to an existing blob in the old archive — copy it verbatim.
    ReuseOld { offset: u64, size: u64, checksum: u64, orig_checksum: u64 },
    // Blob was freshly compressed (new or changed file).
    Fresh    { data: Vec<u8>, orig_checksum: u64 },
    // No blob (directory, symlink, or empty file).
    Empty,
}

struct Item {
    name:      String,
    kind:      u8,
    mode:      u32,
    uid:       u32,
    gid:       u32,
    mtime:     i64,
    orig_size: u64,
    link:      String,
    blob:      BlobSrc,
}

pub fn cmd_update(cli: &Cli) -> io::Result<()> {
    if cli.paths.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "no paths specified"));
    }

    let t0           = Instant::now();
    let archive_path = Path::new(&cli.file);

    // ---- Read old archive (if present) ----
    let old_entries: Vec<StoredEntry> = if archive_path.exists() {
        let f   = File::open(archive_path)?;
        let mut br = BufReader::with_capacity(IN_BUF, f);
        verify_magic(&mut br)?;
        let n = read_u64(&mut br)?;
        read_entries(&mut br, n)?
    } else {
        Vec::new()
    };

    let old_map: HashMap<&str, &StoredEntry> = old_entries.iter()
        .map(|e| (e.name.as_str(), e))
        .collect();

    // ---- Walk the filesystem ----
    let fs_entries = collect_entries(&cli.paths);
    eprintln!("[ptar] scanned {} entries  ({:.2}s)", fs_entries.len(), t0.elapsed().as_secs_f64());

    let fs_names: std::collections::HashSet<&str> = fs_entries.iter()
        .map(|e| e.display.as_str())
        .collect();

    // ---- Classify every entry that will appear in the new archive ----
    let mut items: Vec<Item> = Vec::new();

    // New / updated entries from the filesystem walk.
    for e in &fs_entries {
        let kind = if e.is_dir { 1 } else if e.is_symlink { 2 } else { 0 };
        let link = e.link_target.as_ref()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();

        let blob = if kind == 0 && e.size > 0 {
            match old_map.get(e.display.as_str()) {
                // Same size and mtime — reuse the existing compressed blob.
                Some(old) if old.orig_size == e.size && old.mtime == e.mtime =>
                    BlobSrc::ReuseOld {
                        offset:        old.data_offset,
                        size:          old.data_size,
                        checksum:      old.checksum,
                        orig_checksum: old.orig_checksum,
                    },
                // New file or size/mtime changed — needs fresh compression.
                _ => BlobSrc::Fresh { data: Vec::new(), orig_checksum: 0 },
            }
        } else {
            BlobSrc::Empty
        };

        items.push(Item { name: e.display.clone(), kind, mode: e.mode, uid: e.uid,
                          gid: e.gid, mtime: e.mtime, orig_size: e.size, link, blob });
    }

    // Old-only entries — not touched by the current walk, kept verbatim.
    for old_e in &old_entries {
        if !fs_names.contains(old_e.name.as_str()) {
            let blob = if old_e.data_size > 0 {
                BlobSrc::ReuseOld {
                    offset:        old_e.data_offset,
                    size:          old_e.data_size,
                    checksum:      old_e.checksum,
                    orig_checksum: old_e.orig_checksum,
                }
            } else {
                BlobSrc::Empty
            };
            items.push(Item {
                name:      old_e.name.clone(),
                kind:      old_e.kind,
                mode:      old_e.mode,
                uid:       old_e.uid,
                gid:       old_e.gid,
                mtime:     old_e.mtime,
                orig_size: old_e.orig_size,
                link:      old_e.link.clone(),
                blob,
            });
        }
    }

    // ---- Parallel-compress new / changed files ----
    // Indices into `items` that need fresh compression.
    let compress_idx: Vec<usize> = items.iter().enumerate()
        .filter(|(_, it)| matches!(it.blob, BlobSrc::Fresh { .. }))
        .map(|(i, _)| i)
        .collect();

    let n_compress = compress_idx.len();
    let level      = cli.level;
    let n_threads  = rayon::current_num_threads();

    let fs_by_name: HashMap<&str, _> = fs_entries.iter()
        .map(|e| (e.display.as_str(), e))
        .collect();

    let total_fresh_bytes: u64 = compress_idx.iter()
        .map(|&i| items[i].orig_size)
        .sum();
    let pb = bytes_bar(total_fresh_bytes, "compressing");

    for chunk in compress_idx.chunks(n_threads) {
        let results: io::Result<Vec<(usize, Vec<u8>, u64)>> = chunk.par_iter()
            .map(|&i| {
                let entry = fs_by_name[items[i].name.as_str()];
                let (blob, orig_ck) = compress_entry(entry, level)?;
                Ok((i, blob, orig_ck))
            })
            .collect();
        for (i, blob, orig_ck) in results? {
            pb.inc(items[i].orig_size);
            items[i].blob = BlobSrc::Fresh { data: blob, orig_checksum: orig_ck };
        }
    }

    pb.finish_and_clear();
    if n_compress > 0 {
        eprintln!("[ptar] {n_compress} new/changed file(s) compressed");
    }

    // ---- Compute header size ----
    let header_size: u64 = (8 + 8
        + items.iter().map(|it| record_size(&it.name, &it.link)).sum::<usize>())
        as u64;

    // ---- Write to temp file, then atomically replace the original ----
    let tmp_path = format!("{}.tmp", cli.file);
    {
        let mut out = File::create(&tmp_path)?;
        out.seek(SeekFrom::Start(header_size))?;
        let mut bw = BufWriter::with_capacity(OUT_BUF, out);

        // Seekable handle into the old archive for blob copying.
        let mut old_file: Option<File> = if archive_path.exists() {
            Some(File::open(archive_path)?)
        } else {
            None
        };

        let mut final_offsets:     Vec<u64> = Vec::with_capacity(items.len());
        let mut final_sizes:       Vec<u64> = Vec::with_capacity(items.len());
        let mut final_checksums:   Vec<u64> = Vec::with_capacity(items.len());
        let mut final_orig_cksums: Vec<u64> = Vec::with_capacity(items.len());
        let mut cur = header_size;

        for it in &items {
            match &it.blob {
                BlobSrc::Empty => {
                    final_offsets.push(cur);
                    final_sizes.push(0);
                    final_checksums.push(0);
                    final_orig_cksums.push(0);
                }
                BlobSrc::Fresh { data, orig_checksum } => {
                    let ck = xxh3_64(data);
                    if cli.verbose { eprintln!("  + {}", it.name); }
                    bw.write_all(data)?;
                    final_offsets.push(cur);
                    final_sizes.push(data.len() as u64);
                    final_checksums.push(ck);
                    final_orig_cksums.push(*orig_checksum);
                    cur += data.len() as u64;
                }
                BlobSrc::ReuseOld { offset, size, checksum, orig_checksum } => {
                    if cli.verbose { eprintln!("    {}", it.name); }
                    if let Some(ref mut of) = old_file {
                        of.seek(SeekFrom::Start(*offset))?;
                        copy_n(&mut *of, &mut bw, *size)?;
                    }
                    final_offsets.push(cur);
                    final_sizes.push(*size);
                    final_checksums.push(*checksum);
                    final_orig_cksums.push(*orig_checksum);
                    cur += size;
                }
            }
        }

        bw.flush()?;
        let mut out = bw.into_inner().unwrap();

        // Write the header at offset 0.
        out.seek(SeekFrom::Start(0))?;
        let mut bw = BufWriter::with_capacity(OUT_BUF, out);
        bw.write_all(MAGIC)?;
        bw.write_all(&(items.len() as u64).to_le_bytes())?;
        for (i, it) in items.iter().enumerate() {
            write_entry(&mut bw, &StoredEntry {
                name:          it.name.clone(),
                kind:          it.kind,
                mode:          it.mode,
                uid:           it.uid,
                gid:           it.gid,
                mtime:         it.mtime,
                data_offset:   final_offsets[i],
                data_size:     final_sizes[i],
                orig_size:     it.orig_size,
                checksum:      final_checksums[i],
                orig_checksum: final_orig_cksums[i],
                link:          it.link.clone(),
            })?;
        }
        bw.flush()?;

        // Optional checksums manifest.
        if let Some(ref cksum_path) = cli.checksums {
            let mut cf = BufWriter::new(File::create(cksum_path)?);
            for (i, it) in items.iter().enumerate() {
                if it.kind == 0 && final_sizes[i] > 0 {
                    writeln!(cf, "{:#018x}  {}", final_orig_cksums[i], it.name)?;
                }
            }
            cf.flush()?;
        }
    }

    std::fs::rename(&tmp_path, archive_path)?;

    eprintln!("[ptar] done — {:.2}s total", t0.elapsed().as_secs_f64());
    Ok(())
}

/// Copy exactly `n` bytes from `r` to `w`.
fn copy_n<R: Read, W: Write>(r: &mut R, w: &mut W, mut n: u64) -> io::Result<()> {
    let mut buf = [0u8; 64 * 1024];
    while n > 0 {
        let chunk = (buf.len() as u64).min(n) as usize;
        r.read_exact(&mut buf[..chunk])?;
        w.write_all(&buf[..chunk])?;
        n -= chunk as u64;
    }
    Ok(())
}
