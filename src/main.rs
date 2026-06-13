mod cli;
mod create;
mod entry;
mod extract;
mod format;
mod list;

use clap::Parser;
use std::io;

use cli::Cli;
use create::cmd_create;
use extract::cmd_extract;
use list::cmd_list;

fn main() {
    if let Err(e) = run() {
        if e.kind() != io::ErrorKind::BrokenPipe {
            eprintln!("ptar: error: {e}");
            std::process::exit(1);
        }
    }
}

fn run() -> io::Result<()> {
    let cli = Cli::parse();

    let threads = if cli.threads == 0 {
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
    } else {
        cli.threads as usize
    };

    // Initialise the global rayon pool once — affects jwalk traversal,
    // parallel compression (create) and parallel decompress/write (extract).
    rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build_global()
        .ok(); // ignore error if already initialised

    if cli.verbose || cli.create {
        eprintln!("[ptar] using {threads} thread(s)");
    }

    if cli.create {
        cmd_create(&cli)
    } else if cli.extract {
        cmd_extract(&cli)
    } else if cli.list {
        cmd_list(&cli)
    } else {
        eprintln!("ptar: specify -c (create), -x (extract), or -t (list)");
        eprintln!();
        eprintln!("  ptar -c -f out.ptar dir/ file.txt ...");
        eprintln!("  ptar -x -f out.ptar");
        eprintln!("  ptar -t -f out.ptar");
        std::process::exit(1);
    }
}
