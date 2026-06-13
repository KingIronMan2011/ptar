# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```sh
# Build
cargo build
cargo build --release

# Run
cargo run -- -c -f out.ptar dir/ file.txt   # create
cargo run -- -x -f out.ptar                  # extract
cargo run -- -t -f out.ptar                  # list

# Test
cargo test

# Lint / Format
cargo clippy -- -D warnings
cargo fmt
```

## Architecture

The codebase is split into six modules under `src/`:

| Module | Contents |
|--------|----------|
| `main.rs` | Global rayon pool init, CLI dispatch, `main()` |
| `cli.rs` | `Cli` struct (clap derive) — all flags and arguments |
| `entry.rs` | `Entry` struct + `collect_entries()` — parallel walk via `jwalk` |
| `format.rs` | Binary format: `StoredEntry`, `write_entry`, `read_entries`, `verify_magic`, `record_size` |
| `create.rs` | `cmd_create()` — chunked parallel compression + two-phase header write |
| `extract.rs` | `cmd_extract()` — reader→channel→rayon pipeline, streaming decompress |
| `list.rs` | `cmd_list()` — read entry table, print |

## Archive format (.ptar)

Custom binary format, not tar-compatible:

```
[8]   magic + version  ("PTAR\x00\x01\x00\x00")
[8]   entry count (u64 LE)
[var] entry records (name, kind, mode/uid/gid/mtime, data_offset, data_size, orig_size, link)
[var] file data blobs — each file independently zstd-compressed, in table order
```

Each entry record uses length-prefixed strings (u16 LE) for name and link target. All integers are little-endian. `kind`: 0 = file, 1 = dir, 2 = symlink.

## Parallelism model

**Create (chunked, bounded RAM):**
1. `jwalk` parallel directory walk fills `Vec<Entry>`
2. Compute header size from metadata only (no I/O)
3. `seek()` past the header region — blobs are written first, header filled in last
4. Files are processed in chunks of `n_threads`: each chunk is compressed in parallel via rayon, then written sequentially. Only `n_threads` compressed blobs live in RAM at once.
5. After all blobs written, `seek(0)` and write the complete header with correct offsets.

**Extract (pipelined, bounded RAM):**
1. Read and parse entry table (sequential, tiny)
2. Create directories (sequential pass)
3. One reader thread reads compressed blobs sequentially from disk and sends them into a bounded `mpsc::sync_channel` (capacity = `n_workers × 2`)
4. `par_bridge()` distributes blobs from the channel to rayon workers, which stream-decompress directly to disk via `zstd::Decoder` + `io::copy` — no fully-decompressed `Vec` ever lives in RAM
5. Symlinks created last (sequential)

**Platform note:** Unix metadata (`uid`, `gid`, `mtime`, `mode`) and symlink creation are gated with `#[cfg(unix)]` in `entry.rs` and `extract.rs`. The binary compiles on Windows but symlinks are silently skipped.

## Known Limitations

- Path names longer than 65535 bytes are unsupported (u16 length prefix).
- Not compatible with standard `tar` or `.tar.zst`.
- Symlink creation on extract is Unix-only.
