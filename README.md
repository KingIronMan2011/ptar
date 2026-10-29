# ptar

A fast parallel archiver. Uses every CPU core for both creating and extracting archives.

## How it works

Standard archivers compress a stream sequentially — core 0 does all the work while the rest sit idle. ptar avoids this by giving each file its own independent zstd context, so all cores compress simultaneously. The archive format stores a small offset table at the front so extraction can decompress files in parallel too, with I/O and CPU overlapping via a reader→worker pipeline.

**Create:** parallel directory walk → all files compressed simultaneously across all cores → sequential write to disk (optimal I/O pattern).

**Extract:** sequential read from disk → bounded channel → all cores decompress and write files simultaneously.

RAM usage is bounded: at most `n_threads` compressed blobs exist in memory at any time during create, and at most `n_threads × 2` during extract.

## Install

You need the Rust toolchain. If you don't have it:

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Then clone and install:

```sh
git clone <repo-url>
cd ptar
cargo install --path .
```

This compiles a release binary and places it in `~/.cargo/bin/ptar`. Make sure `~/.cargo/bin` is in your `PATH` (Rust's installer adds it automatically).

To verify:

```sh
ptar --version
```

### Build manually (without installing)

```sh
cargo build --release
# binary is at ./target/release/ptar
```

You can copy it anywhere on your `PATH`:

```sh
cp target/release/ptar /usr/local/bin/
```

## Usage

```command-syntax
ptar -c -f <archive.ptar> <paths...>   # create
ptar -x -f <archive.ptar>             # extract
ptar -t -f <archive.ptar>             # list contents
```

### Create an archive

```sh
# Archive a directory
ptar -c -f backup.ptar ~/projects/myapp

# Archive multiple paths
ptar -c -f backup.ptar src/ Cargo.toml README.md

# Verbose (print each file as it's added)
ptar -c -f backup.ptar src/ -v

# Higher compression (slower, smaller — levels 1–19, default 3)
ptar -c -f backup.ptar src/ -l 9

# Limit to 4 threads
ptar -c -f backup.ptar src/ -T 4
```

### Extract an archive

```sh
# Extract into the current directory
ptar -x -f backup.ptar

# Verbose (print each file as it's extracted)
ptar -x -f backup.ptar -v
```

### List contents

```sh
# Names only
ptar -t -f backup.ptar

# With file types and sizes
ptar -t -f backup.ptar -v
```

Verbose list output format:

```command-preview
d             0  src/
f         12048  src/main.rs
f          3200  src/lib.rs
```

`d` = directory, `f` = file, `l` = symlink. Size is the original uncompressed size in bytes.

## Options

| Flag        | Default   | Description                          |
| ----------- | --------- | ------------------------------------ |
| `-c`        | —         | Create archive                       |
| `-x`        | —         | Extract archive                      |
| `-t`        | —         | List archive contents                |
| `-f <path>` | —         | Archive file (required)              |
| `-v`        | off       | Print each entry                     |
| `-l <1–19>` | `3`       | zstd compression level               |
| `-T <n>`    | all cores | Number of worker threads (`0` = all) |

## Archive format

ptar uses its own binary format (`.ptar`), not tar. The format is:

```preview
[8 bytes]  magic + version
[8 bytes]  number of entries
[variable] entry table — name, permissions, timestamps, data offset, compressed size
[variable] file data — each file as an independent zstd-compressed blob
```

Because each file is its own zstd blob with a known byte offset, extraction can decompress any file without reading the others, enabling full parallel extraction. The entry table is written last (after all blobs) so the archiver never needs to hold more than one chunk of compressed data in memory at once.

## Requirements

- Rust 1.65 or later
- Linux, macOS, Windows, or other platforms (symlinks on Windows require **Developer Mode** or administrator privileges — without one of those, symlink entries in an archive will fail to extract with a permission error; all other entry types work normally)

### Windows: self-install and Explorer integration

Run the release binary once (double-click, or `ptar.exe --install`). It copies itself to `Program Files\ptar`, adds that folder to the system `PATH` (UAC prompt if not elevated) and registers the `.ptar` file type with Explorer context-menu entries:

- on `.ptar` files: **Extract here (ptar)** and **Extract to folder (ptar)**
- on any file or folder: **Add to .ptar archive** (creates `<name>.ptar` next to it)

On Windows 11 these are under *Show more options*. `ptar --uninstall` removes the PATH entry, the context-menu entries and the folder.
