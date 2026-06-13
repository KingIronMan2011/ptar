use clap::Parser;

#[derive(Parser)]
#[command(
    name    = "ptar",
    version = "0.1.0",
    about   = "Parallel tar — rayon reads + zstd multi-core compression",
    long_about = "Creates .tar.zst archives using all available CPU cores.\n\
                  Parallelises both I/O (rayon) and compression (zstd workers)."
)]
pub struct Cli {
    /// Create archive from paths
    #[arg(short = 'c', long, conflicts_with_all = ["extract", "list"])]
    pub create: bool,

    /// Extract archive
    #[arg(short = 'x', long, conflicts_with_all = ["create", "list"])]
    pub extract: bool,

    /// List archive contents
    #[arg(short = 't', long, conflicts_with_all = ["create", "extract"])]
    pub list: bool,

    /// Archive file path (.tar.zst)
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

    /// Paths to archive (create mode)
    pub paths: Vec<String>,
}
