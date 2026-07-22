//! Minimal ustar (POSIX.1-1988 tar) writer.
//!
//! The diagnose bundle is a plain uncompressed tar so testers can inspect it
//! with standard tools before attaching it to a bug report. Writing ustar by
//! hand keeps the CLI dependency-free (headers are fixed 512-byte records
//! with octal fields and a checksum; see `pax`(1) / GNU tar docs).

use std::io::{self, Write};

const BLOCK: usize = 512;

/// Streams `(path, contents)` entries as a ustar archive.
pub struct TarWriter<W: Write> {
    out: W,
}

impl<W: Write> TarWriter<W> {
    pub fn new(out: W) -> Self {
        Self { out }
    }

    /// Append a regular file entry (mode 0644). `path` must be relative,
    /// use `/` separators, and fit in the 100-byte ustar name field.
    pub fn append(&mut self, path: &str, contents: &[u8], mtime: u64) -> io::Result<()> {
        if path.len() > 100 || path.starts_with('/') {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("tar entry name invalid or too long: {path}"),
            ));
        }
        let mut header = [0u8; BLOCK];
        header[..path.len()].copy_from_slice(path.as_bytes());
        write_octal(&mut header[100..108], 0o644); // mode
        write_octal(&mut header[108..116], 0); // uid
        write_octal(&mut header[116..124], 0); // gid
        write_octal12(&mut header[124..136], contents.len() as u64); // size
        write_octal12(&mut header[136..148], mtime); // mtime
        header[148..156].fill(b' '); // checksum placeholder
        header[156] = b'0'; // typeflag: regular file
        header[257..262].copy_from_slice(b"ustar"); // magic
        header[263..265].copy_from_slice(b"00"); // version
        let checksum: u32 = header.iter().map(|&b| b as u32).sum();
        write_octal_n(&mut header[148..155], checksum as u64, 6); // 6 digits + NUL
        header[155] = b' ';

        self.out.write_all(&header)?;
        self.out.write_all(contents)?;
        let pad = (BLOCK - contents.len() % BLOCK) % BLOCK;
        self.out.write_all(&vec![0u8; pad])
    }

    /// Write the end-of-archive marker (two zero blocks) and return the
    /// underlying writer.
    pub fn finish(mut self) -> io::Result<W> {
        self.out.write_all(&[0u8; 2 * BLOCK])?;
        Ok(self.out)
    }
}

fn write_octal(field: &mut [u8], value: u64) {
    let width = field.len() - 1; // NUL-terminated
    write_octal_n(field, value, width);
}

fn write_octal_n(field: &mut [u8], value: u64, width: usize) {
    let s = format!("{value:0width$o}");
    field[..width].copy_from_slice(s.as_bytes());
    field[width] = 0;
}

/// 12-byte size/mtime fields: 11 octal digits + NUL.
fn write_octal12(field: &mut [u8], value: u64) {
    write_octal_n(field, value, 11);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parse the archive back with a tiny reader to verify structure.
    fn entries(archive: &[u8]) -> Vec<(String, Vec<u8>)> {
        let mut out = Vec::new();
        let mut off = 0;
        while off + BLOCK <= archive.len() {
            let header = &archive[off..off + BLOCK];
            if header.iter().all(|&b| b == 0) {
                break;
            }
            let name_end = header.iter().position(|&b| b == 0).unwrap();
            let name = String::from_utf8(header[..name_end].to_vec()).unwrap();
            let size_str = std::str::from_utf8(&header[124..135]).unwrap();
            let size = usize::from_str_radix(size_str, 8).unwrap();
            assert_eq!(&header[257..262], b"ustar");
            let expected: u32 = header
                .iter()
                .enumerate()
                .map(|(i, &b)| {
                    if (148..156).contains(&i) {
                        32
                    } else {
                        b as u32
                    }
                })
                .sum();
            let stored =
                u32::from_str_radix(std::str::from_utf8(&header[148..154]).unwrap(), 8).unwrap();
            assert_eq!(stored, expected, "checksum mismatch for {name}");
            off += BLOCK;
            out.push((name, archive[off..off + size].to_vec()));
            off += size.div_ceil(BLOCK) * BLOCK;
        }
        out
    }

    #[test]
    fn round_trips_entries() {
        let mut w = TarWriter::new(Vec::new());
        w.append("report.txt", b"hello diagnose\n", 1_700_000_000)
            .unwrap();
        w.append("vms/demo/logs/serial.log", &[b'x'; 513], 1_700_000_000)
            .unwrap();
        let archive = w.finish().unwrap();
        assert_eq!(archive.len() % BLOCK, 0);
        let got = entries(&archive);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].0, "report.txt");
        assert_eq!(got[0].1, b"hello diagnose\n");
        assert_eq!(got[1].0, "vms/demo/logs/serial.log");
        assert_eq!(got[1].1.len(), 513);
    }

    #[test]
    fn rejects_bad_names() {
        let mut w = TarWriter::new(Vec::new());
        assert!(w.append("/abs", b"", 0).is_err());
        assert!(w.append(&"a".repeat(101), b"", 0).is_err());
    }
}
