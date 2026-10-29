mod cli;
mod create;
mod entry;
mod extract;
mod format;
mod hash;
#[cfg(windows)]
mod install;
mod list;
mod platform;
mod update;
mod verify;

use clap::Parser;
use std::io;

use cli::Cli;
use create::cmd_create;
use extract::cmd_extract;
use list::cmd_list;
use update::cmd_update;
use verify::cmd_verify;

fn main() {
    if let Err(e) = run() {
        if e.kind() != io::ErrorKind::BrokenPipe {
            eprintln!("ptar: error: {e}");
            std::process::exit(1);
        }
    }
}

fn run() -> io::Result<()> {
    // Windows: running with no args (or --install/--uninstall) installs/removes ptar.
    #[cfg(windows)]
    {
        let args: Vec<String> = std::env::args().collect();
        if let Some(result) = install::try_run(&args) {
            return result;
        }
    }

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
    } else if cli.verify {
        cmd_verify(&cli)
    } else if cli.update {
        cmd_update(&cli)
    } else {
        eprintln!("ptar: specify -c (create), -x (extract), -t (list), -k (verify), or -u (update)");
        eprintln!();
        eprintln!("  ptar -c -f out.ptar dir/ file.txt ...");
        eprintln!("  ptar -x -f out.ptar");
        eprintln!("  ptar -t -f out.ptar");
        eprintln!("  ptar -k -f out.ptar");
        eprintln!("  ptar -u -f out.ptar dir/ file.txt ...");
        std::process::exit(1);
    }
}
