use std::{
    fs::File,
    io::{self, BufReader, BufWriter, Seek, SeekFrom, Write},
    time::Instant,
};

use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use xxhash_rust::xxh3::xxh3_64;

use crate::{
    cli::Cli,
    entry::{collect_entries, Entry},
    format::{record_size, write_entry, StoredEntry, MAGIC},
    hash::HashingReader,
};

const OUT_BUF: usize = 4 * 1024 * 1024;

pub fn cmd_create(cli: &Cli) -> io::Result<()> {
    if cli.paths.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "no paths specified"));
    }

    let t0 = Instant::now();
    let entries = collect_entries(&cli.paths);
    eprintln!("[ptar] scanned {} entries  ({:.2}s)", entries.len(), t0.elapsed().as_secs_f64());

    let header_size: u64 = (8 + 8
        + entries.iter().map(|e| record_size(&e.display, &link_of(e))).sum::<usize>())
        as u64;

    let mut out = File::create(&cli.file)?;
    out.seek(SeekFrom::Start(header_size))?;
    let mut bw = BufWriter::with_capacity(OUT_BUF, out);

    let n = rayon::current_num_threads();
    let file_indices: Vec<usize> = entries.iter().enumerate()
        .filter(|(_, e)| e.is_file && e.size > 0)
        .map(|(i, _)| i)
        .collect();

    // Progress bar tracks uncompressed bytes read — gives meaningful MB/s throughput.
    let total_bytes: u64 = file_indices.iter().map(|&i| entries[i].size).sum();
    let pb = bytes_bar(total_bytes, "compressing");

    let mut data_sizes          = vec![0u64; entries.len()];
    let mut data_checksums      = vec![0u64; entries.len()];
    let mut data_orig_checksums = vec![0u64; entries.len()];
    let level = cli.level;

    for chunk in file_indices.chunks(n) {
        let blobs: io::Result<Vec<(Vec<u8>, u64)>> = chunk.par_iter()
            .map(|&i| compress_entry(&entries[i], level))
            .collect();
        let blobs = blobs?;

        for (&entry_idx, (blob, orig_cksum)) in chunk.iter().zip(blobs.iter()) {
            data_sizes[entry_idx]          = blob.len() as u64;
            data_checksums[entry_idx]      = xxh3_64(blob);
            data_orig_checksums[entry_idx] = *orig_cksum;
            if cli.verbose { pb.println(format!("  {}", entries[entry_idx].display)); }
            bw.write_all(blob)?;
            pb.inc(entries[entry_idx].size);
        }
    }

    pb.finish_and_clear();

    bw.flush()?;
    let mut out = bw.into_inner().unwrap();

    let mut offset = header_size;
    let stored: Vec<StoredEntry> = entries.iter().enumerate().map(|(i, e)| {
        let data_size = data_sizes[i];
        let se = StoredEntry {
            name:          e.display.clone(),
            kind:          if e.is_dir { 1 } else if e.is_symlink { 2 } else { 0 },
            mode:          e.mode,
            uid:           e.uid,
            gid:           e.gid,
            mtime:         e.mtime,
            data_offset:   offset,
            data_size,
            orig_size:     e.size,
            checksum:      data_checksums[i],
            orig_checksum: data_orig_checksums[i],
            link:          link_of(e),
        };
        offset += data_size;
        se
    }).collect();

    out.seek(SeekFrom::Start(0))?;
    let mut bw = BufWriter::with_capacity(OUT_BUF, out);
    bw.write_all(MAGIC)?;
    bw.write_all(&(stored.len() as u64).to_le_bytes())?;
    for se in &stored {
        write_entry(&mut bw, se)?;
    }
    bw.flush()?;

    if let Some(ref cksum_path) = cli.checksums {
        let mut cf = BufWriter::new(File::create(cksum_path)?);
        for se in &stored {
            if se.kind == 0 && se.data_size > 0 {
                writeln!(cf, "{:#018x}  {}", se.orig_checksum, se.name)?;
            }
        }
        cf.flush()?;
    }

    eprintln!("[ptar] done — {:.2}s total", t0.elapsed().as_secs_f64());
    Ok(())
}

// Returns (compressed_blob, xxh3_of_uncompressed).
pub(crate) fn compress_entry(e: &Entry, level: i32) -> io::Result<(Vec<u8>, u64)> {
    let f      = File::open(&e.path)?;
    let br     = BufReader::with_capacity(256 * 1024, f);
    let mut hr = HashingReader::new(br);
    let blob   = zstd::encode_all(&mut hr, level)?;
    Ok((blob, hr.finish()))
}

pub(crate) fn bytes_bar(total: u64, action: &str) -> ProgressBar {
    let pb = ProgressBar::new(total);
    pb.set_style(
        ProgressStyle::with_template(&format!(
            "{{spinner:.green}} {action} [{{elapsed_precise}}] \
             [{{bar:40.cyan/blue}}] {{bytes}}/{{total_bytes}} \
             ({{bytes_per_sec}}, ETA {{eta}})"
        ))
        .unwrap()
        .progress_chars("=>-"),
    );
    pb.enable_steady_tick(std::time::Duration::from_millis(100));
    pb
}

fn link_of(e: &Entry) -> String {
    e.link_target.as_ref()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default()
}
