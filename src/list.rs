use std::{fs::File, io::{self, BufReader}};

use crate::{
    cli::Cli,
    format::{read_entries, read_u64, verify_magic},
};

const IN_BUF: usize = 4 * 1024 * 1024;

pub fn cmd_list(cli: &Cli) -> io::Result<()> {
    let f  = File::open(&cli.file)?;
    let mut br = BufReader::with_capacity(IN_BUF, f);

    verify_magic(&mut br)?;
    let n = read_u64(&mut br)?;
    let entries = read_entries(&mut br, n)?;

    for e in &entries {
        let kind = match e.kind {
            1 => 'd',
            2 => 'l',
            _ => 'f',
        };
        if cli.verbose {
            println!("{kind}  {:>12}  {}", e.orig_size, e.name);
        } else {
            println!("{}", e.name);
        }
    }

    Ok(())
}
