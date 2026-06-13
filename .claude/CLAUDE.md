# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

`ptar` is a single-binary Rust CLI tool that creates/extracts/lists `.tar.zst` archives using parallel I/O (rayon) and multi-threaded zstd compression. It implements the POSIX ustar format directly — no tar library dependency.

## Commands

```bash
# Build (debug)
cargo build

# Build (release — optimised, stripped binary)
cargo build --release

# Run
cargo run -- -c -f out.tar.zst dir/ file.txt   # create
cargo run -- -x -f out.tar.zst                  # extract
cargo run -- -t -f out.tar.zst                  # list

# Tests
cargo test

# Lint
cargo clippy -- -D warnings

# Format
cargo fmt
```

## Architecture

The codebase is split into six modules under `src/`:

| Module       | Contents                                                                                                                      |
| ------------ | ----------------------------------------------------------------------------------------------------------------------------- |
| `main.rs`    | CLI dispatch (`run()`), thread-count resolution, `main()`                                                                     |
| `cli.rs`     | `Cli` struct (clap derive) — all flags and arguments                                                                          |
| `entry.rs`   | `Entry` struct + `collect_entries()` — `walkdir` traversal                                                                    |
| `header.rs`  | ustar format: `make_header()`, `read_header()`, `HeaderRecord` enum, `padding()`, `skip()`, shared `IO_BUF`/`CHUNK` constants |
| `create.rs`  | `cmd_create()` — parallel read phase + sequential zstd encode                                                                 |
| `extract.rs` | `cmd_extract()` — zstd decode + file/dir/symlink restoration                                                                  |
| `list.rs`    | `cmd_list()` — zstd decode + entry printing                                                                                   |

**Parallelism model:**

- `rayon` pre-reads all files ≤ 64 MiB in parallel into memory blobs (`create.rs`).
- The tar stream is written sequentially (ustar requires ordered blocks).
- `zstd::Encoder::multithread(n)` compresses in parallel on the write side.

**Header reading:** `read_header()` in `header.rs` returns a `HeaderRecord` enum (`Entry`, `Zero`, `Eof`) used by both `extract` and `list` to eliminate the duplicated loop pattern.

**Platform note:** Unix-specific metadata (`uid`, `gid`, `mtime`, `mode`, symlinks) is gated behind `#[cfg(unix)]` in `entry.rs` and `extract.rs`; the code compiles on non-Unix with stub values.

## Known Limitations

- Path names longer than 100 bytes are silently truncated (ustar limit; GNU `@LongLink` extension not yet implemented).
- Symlink creation on extract is Unix-only; non-Unix builds silently skip symlinks.
