use std::{
    fs::File,
    io::{self, BufReader, Cursor, Read},
};

use rayon::iter::{ParallelBridge, ParallelIterator};
use xxhash_rust::xxh3::xxh3_64;

use crate::{
    cli::Cli,
    format::{read_entries, read_u64, verify_magic},
    hash::HashingWriter,
};

const IN_BUF: usize = 4 * 1024 * 1024;

pub fn cmd_verify(cli: &Cli) -> io::Result<()> {
    let f   = File::open(&cli.file)?;
    let mut br = BufReader::with_capacity(IN_BUF, f);

    verify_magic(&mut br)?;
    let n = read_u64(&mut br)?;
    let entries = read_entries(&mut br, n)?;

    // Only files with content need checking.
    let file_entries: Vec<_> = entries.into_iter()
        .filter(|e| e.kind == 0 && e.data_size > 0)
        .collect();

    // Read all blobs sequentially (the file must be read in order).
    // Each blob is sent to a rayon worker for verification.
    use std::sync::mpsc;
    let n_workers  = rayon::current_num_threads();
    let verbose    = cli.verbose;
    let (tx, rx)   = mpsc::sync_channel::<io::Result<(String, Vec<u8>, u64, u64)>>(n_workers * 2);

    let reader = std::thread::spawn(move || {
        for e in file_entries {
            let result = (|| -> io::Result<_> {
                let mut data = vec![0u8; e.data_size as usize];
                br.read_exact(&mut data)?;
                Ok((e.name, data, e.checksum, e.orig_checksum))
            })();
            let failed = result.is_err();
            if tx.send(result).is_err() || failed { break; }
        }
    });

    let verify_result = rx
        .into_iter()
        .par_bridge()
        .try_for_each(|item| -> io::Result<()> {
            let (name, data, exp_cksum, exp_orig_cksum) = item?;

            // Verify compressed blob checksum.
            let actual = xxh3_64(&data);
            if actual != exp_cksum {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "FAIL {name}: compressed checksum {actual:#018x} != {exp_cksum:#018x}"
                    ),
                ));
            }

            // Decompress and verify original content checksum.
            let mut dec = zstd::Decoder::new(Cursor::new(&data))?;
            let mut hw  = HashingWriter::new(io::sink());
            io::copy(&mut dec, &mut hw)?;
            let actual_orig = hw.finish();

            if exp_orig_cksum != 0 && actual_orig != exp_orig_cksum {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "FAIL {name}: content checksum {actual_orig:#018x} != {exp_orig_cksum:#018x}"
                    ),
                ));
            }

            if verbose { eprintln!("  OK  {name}"); }
            Ok(())
        });

    reader.join().expect("reader thread panicked");
    verify_result?;

    eprintln!("[ptar] verify OK");
    Ok(())
}
