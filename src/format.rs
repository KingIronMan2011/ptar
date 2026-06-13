//! Binary format for .ptar archives (version 3).
//!
//! Layout:
//!   [8]   magic + version
//!   [8]   entry count (u64 LE)
//!   [var] entry records (see StoredEntry / write_entry / read_entries)
//!   [var] compressed file blobs, in entry-table order

use std::io::{self, Read, Write};

/// Magic bytes — last two bytes are the format version (currently 0x03).
pub const MAGIC: &[u8; 8] = b"PTAR\x00\x03\x00\x00";

/// Fixed byte cost per entry record, excluding the variable-length name and link strings.
///
/// 2  name_len
/// 1  kind
/// 4  mode  +  4 uid  +  4 gid  +  8 mtime
/// 8  data_offset  +  8 data_size  +  8 orig_size
/// 8  checksum      (xxh3-64 of compressed blob)
/// 8  orig_checksum (xxh3-64 of uncompressed content)
/// 2  link_len
/// = 65
const ENTRY_FIXED: usize = 65;

pub struct StoredEntry {
    pub name:        String,
    pub kind:        u8,      // 0 = file, 1 = dir, 2 = symlink
    pub mode:        u32,
    pub uid:         u32,
    pub gid:         u32,
    pub mtime:       i64,
    pub data_offset: u64,     // absolute byte offset in the archive file
    pub data_size:   u64,     // compressed size  (0 for dirs/symlinks)
    pub orig_size:   u64,     // uncompressed size
    pub checksum:      u64,   // xxh3-64 of the compressed blob      (0 when data_size == 0)
    pub orig_checksum: u64,   // xxh3-64 of the uncompressed content (0 when data_size == 0)
    pub link:          String,
}

/// Serialised byte length of one entry record.
pub fn record_size(name: &str, link: &str) -> usize {
    ENTRY_FIXED + name.len() + link.len()
}

pub fn write_entry<W: Write>(w: &mut W, e: &StoredEntry) -> io::Result<()> {
    write_str(w, &e.name)?;
    w.write_all(&[e.kind])?;
    w.write_all(&e.mode.to_le_bytes())?;
    w.write_all(&e.uid.to_le_bytes())?;
    w.write_all(&e.gid.to_le_bytes())?;
    w.write_all(&e.mtime.to_le_bytes())?;
    w.write_all(&e.data_offset.to_le_bytes())?;
    w.write_all(&e.data_size.to_le_bytes())?;
    w.write_all(&e.orig_size.to_le_bytes())?;
    w.write_all(&e.checksum.to_le_bytes())?;
    w.write_all(&e.orig_checksum.to_le_bytes())?;
    write_str(w, &e.link)
}

pub fn verify_magic<R: Read>(r: &mut R) -> io::Result<()> {
    let mut buf = [0u8; 8];
    r.read_exact(&mut buf)?;
    if buf != *MAGIC {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "not a ptar archive or unsupported version",
        ));
    }
    Ok(())
}

pub fn read_u64<R: Read>(r: &mut R) -> io::Result<u64> {
    let mut b = [0u8; 8];
    r.read_exact(&mut b)?;
    Ok(u64::from_le_bytes(b))
}

pub fn read_entries<R: Read>(r: &mut R, n: u64) -> io::Result<Vec<StoredEntry>> {
    (0..n).map(|_| read_entry(r)).collect()
}

fn read_entry<R: Read>(r: &mut R) -> io::Result<StoredEntry> {
    Ok(StoredEntry {
        name:        read_str(r)?,
        kind:        read_u8(r)?,
        mode:        read_u32(r)?,
        uid:         read_u32(r)?,
        gid:         read_u32(r)?,
        mtime:       read_i64(r)?,
        data_offset: read_u64(r)?,
        data_size:   read_u64(r)?,
        orig_size:   read_u64(r)?,
        checksum:      read_u64(r)?,
        orig_checksum: read_u64(r)?,
        link:          read_str(r)?,
    })
}

// ─── Primitive helpers ────────────────────────────────────────────────────────

fn write_str<W: Write>(w: &mut W, s: &str) -> io::Result<()> {
    let b = s.as_bytes();
    w.write_all(&(b.len() as u16).to_le_bytes())?;
    w.write_all(b)
}

fn read_str<R: Read>(r: &mut R) -> io::Result<String> {
    let len = read_u16(r)? as usize;
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf)?;
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

fn read_u8<R: Read>(r: &mut R) -> io::Result<u8> {
    let mut b = [0u8; 1];
    r.read_exact(&mut b)?;
    Ok(b[0])
}

fn read_u16<R: Read>(r: &mut R) -> io::Result<u16> {
    let mut b = [0u8; 2];
    r.read_exact(&mut b)?;
    Ok(u16::from_le_bytes(b))
}

fn read_u32<R: Read>(r: &mut R) -> io::Result<u32> {
    let mut b = [0u8; 4];
    r.read_exact(&mut b)?;
    Ok(u32::from_le_bytes(b))
}

fn read_i64<R: Read>(r: &mut R) -> io::Result<i64> {
    let mut b = [0u8; 8];
    r.read_exact(&mut b)?;
    Ok(i64::from_le_bytes(b))
}
