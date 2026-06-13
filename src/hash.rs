//! Thin wrappers that compute an xxh3-64 hash while streaming data through.

use std::io::{self, Read, Write};

use xxhash_rust::xxh3::Xxh3Default;

// ─── HashingReader ────────────────────────────────────────────────────────────

/// Wraps a `Read` and hashes every byte that passes through.
pub struct HashingReader<R> {
    inner:  R,
    hasher: Xxh3Default,
}

impl<R> HashingReader<R> {
    pub fn new(inner: R) -> Self {
        Self { inner, hasher: Xxh3Default::new() }
    }

    /// Consume the wrapper and return the final hash.
    pub fn finish(self) -> u64 {
        self.hasher.digest()
    }
}

impl<R: Read> Read for HashingReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        if n > 0 { self.hasher.update(&buf[..n]); }
        Ok(n)
    }
}

// ─── HashingWriter ────────────────────────────────────────────────────────────

/// Wraps a `Write` and hashes every byte that passes through.
pub struct HashingWriter<W> {
    pub inner: W,
    hasher:    Xxh3Default,
}

impl<W> HashingWriter<W> {
    pub fn new(inner: W) -> Self {
        Self { inner, hasher: Xxh3Default::new() }
    }

    /// Consume the wrapper and return the final hash.
    pub fn finish(self) -> u64 {
        self.hasher.digest()
    }
}

impl<W: Write> Write for HashingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.inner.write(buf)?;
        if n > 0 { self.hasher.update(&buf[..n]); }
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}
