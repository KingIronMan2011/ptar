use clap::Parser;

#[derive(Parser)]
#[command(
    name    = "ptar",
    version,
    about   = "Parallel tar — rayon reads + zstd multi-core compression",
    after_help = if cfg!(windows) {
        "Windows: run `ptar --install` (or just double-click ptar.exe) to copy it to \
         Program Files and add it to PATH; `ptar --uninstall` reverses this."
    } else {
        ""
    },
    long_about = "Creates .ptar archives using all available CPU cores.\n\
                  Parallelises both I/O (rayon) and compression (zstd workers)."
)]
pub struct Cli {
    /// Create archive from paths
    #[arg(short = 'c', long, conflicts_with_all = ["extract", "list", "verify", "update"])]
    pub create: bool,

    /// Extract archive
    #[arg(short = 'x', long, conflicts_with_all = ["create", "list", "verify", "update"])]
    pub extract: bool,

    /// List archive contents
    #[arg(short = 't', long, conflicts_with_all = ["create", "extract", "verify", "update"])]
    pub list: bool,

    /// Verify archive integrity (checksums only, no extraction)
    #[arg(short = 'k', long, conflicts_with_all = ["create", "extract", "list", "update"])]
    pub verify: bool,

    /// Update archive: add new files, replace changed files, keep unchanged blobs
    #[arg(short = 'u', long, conflicts_with_all = ["create", "extract", "list", "verify"])]
    pub update: bool,

    /// Archive file path (.ptar)
    #[arg(short = 'f', long)]
    pub file: String,

    /// Print each entry
    #[arg(short = 'v', long)]
    pub verbose: bool,

    /// Worker threads (0 = all cores)
    #[arg(short = 'T', long = "threads", default_value = "0")]
    pub threads: u32,

    /// Zstd compression level 1–19
    #[arg(short = 'l', long = "level", default_value = "3")]
    pub level: i32,

    /// Write a checksums manifest to <file> (create mode only)
    #[arg(short = 'C', long = "checksums")]
    pub checksums: Option<String>,

    /// Paths to archive (create mode)
    pub paths: Vec<String>,
}
