//! Reads the two archive formats packs come in, `.zip` (Kenney) and `.tgz` (npm), without a
//! dependency beyond `flate2`: only the members a pack keeps are inflated, every size is checked
//! before anything is allocated, and a name that could leave its folder is dropped.

use std::io::Read;

use flate2::read::{DeflateDecoder, GzDecoder};

use super::{Error, Result, msg};

/// What an archive may take once opened.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_member_bytes: u64,
    pub max_total_bytes: u64,
    pub max_members: usize,
}

impl Default for Limits {
    fn default() -> Limits {
        Limits {
            max_member_bytes: 8 * 1024 * 1024,
            max_total_bytes: 64 * 1024 * 1024,
            max_members: 20_000,
        }
    }
}

/// One kept file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    pub path: String,
    pub data: Vec<u8>,
}

/// A path inside an archive that stays inside it.
fn safe(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.contains('\0')
        && path.split('/').all(|p| p != "..")
}

struct Budget {
    limits: Limits,
    total: u64,
    kept: usize,
}

impl Budget {
    fn take(&mut self, size: u64, path: &str) -> Result<()> {
        if size > self.limits.max_member_bytes {
            return msg(format!("{path} is larger than a pack may hold"));
        }
        self.total += size;
        self.kept += 1;
        if self.total > self.limits.max_total_bytes || self.kept > self.limits.max_members {
            return msg("the archive holds more than a pack may");
        }
        Ok(())
    }
}

fn le16(b: &[u8], at: usize) -> Option<usize> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?) as usize)
}

fn le32(b: &[u8], at: usize) -> Option<u64> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?) as u64)
}

/// The members of a zip for which `keep(path)` is true. Stored and deflated members only; no
/// encryption, no zip64.
pub fn read_zip(bytes: &[u8], keep: &dyn Fn(&str) -> bool, limits: Limits) -> Result<Vec<Member>> {
    let bad = || Error::Msg("the zip file is damaged or not one this app reads".into());
    let tail = bytes.len().saturating_sub(22 + 65_535);
    let eocd = bytes[tail..]
        .windows(4)
        .rposition(|w| w == [0x50, 0x4b, 0x05, 0x06])
        .map(|p| p + tail)
        .ok_or_else(bad)?;
    let count = le16(bytes, eocd + 10).ok_or_else(bad)?;
    let mut at = le32(bytes, eocd + 16).ok_or_else(bad)? as usize;
    let mut budget = Budget {
        limits,
        total: 0,
        kept: 0,
    };
    let mut out = Vec::new();
    for _ in 0..count {
        if le32(bytes, at) != Some(0x0201_4b50) {
            return Err(bad());
        }
        let flags = le16(bytes, at + 8).ok_or_else(bad)?;
        let method = le16(bytes, at + 10).ok_or_else(bad)?;
        let csize = le32(bytes, at + 20).ok_or_else(bad)?;
        let usize_ = le32(bytes, at + 24).ok_or_else(bad)?;
        let nlen = le16(bytes, at + 28).ok_or_else(bad)?;
        let elen = le16(bytes, at + 30).ok_or_else(bad)?;
        let clen = le16(bytes, at + 32).ok_or_else(bad)?;
        let local = le32(bytes, at + 42).ok_or_else(bad)? as usize;
        let name = String::from_utf8_lossy(bytes.get(at + 46..at + 46 + nlen).ok_or_else(bad)?)
            .into_owned();
        at += 46 + nlen + elen + clen;
        if name.ends_with('/') || !safe(&name) || !keep(&name) {
            continue;
        }
        if flags & 1 != 0 || csize == 0xFFFF_FFFF || usize_ == 0xFFFF_FFFF {
            return msg(format!("{name} is encrypted or too large for this reader"));
        }
        budget.take(usize_, &name)?;
        if le32(bytes, local) != Some(0x0403_4b50) {
            return Err(bad());
        }
        let start = local
            + 30
            + le16(bytes, local + 26).ok_or_else(bad)?
            + le16(bytes, local + 28).ok_or_else(bad)?;
        let packed = bytes.get(start..start + csize as usize).ok_or_else(bad)?;
        let mut data = Vec::new();
        match method {
            0 => data.extend_from_slice(packed),
            8 => {
                DeflateDecoder::new(packed)
                    .take(usize_ + 1)
                    .read_to_end(&mut data)
                    .map_err(|_| bad())?;
            }
            _ => {
                return msg(format!(
                    "{name} uses a compression this reader does not know"
                ));
            }
        }
        if data.len() as u64 != usize_ {
            return Err(bad());
        }
        out.push(Member { path: name, data });
    }
    Ok(out)
}

/// The octal number in a tar header field.
fn octal(field: &[u8]) -> Option<u64> {
    let text: String = field
        .iter()
        .take_while(|b| **b != 0)
        .map(|b| *b as char)
        .collect();
    u64::from_str_radix(text.trim(), 8).ok()
}

/// The members of a gzipped tar for which `keep(path)` is true. `strip` removes the first path
/// component (npm puts everything under `package/`).
pub fn read_tgz(
    bytes: &[u8],
    strip: bool,
    keep: &dyn Fn(&str) -> bool,
    limits: Limits,
) -> Result<Vec<Member>> {
    let bad = || Error::Msg("the tar file is damaged or not one this app reads".into());
    let mut gz = GzDecoder::new(bytes).take(limits.max_total_bytes * 4);
    let mut budget = Budget {
        limits,
        total: 0,
        kept: 0,
    };
    let mut out = Vec::new();
    let mut long_name: Option<String> = None;
    loop {
        let mut head = [0u8; 512];
        if gz.read_exact(&mut head).is_err() || head.iter().all(|b| *b == 0) {
            break;
        }
        let size = octal(&head[124..136]).ok_or_else(bad)?;
        let kind = head[156];
        let mut name = head[..100]
            .iter()
            .take_while(|b| **b != 0)
            .map(|b| *b as char)
            .collect::<String>();
        if &head[257..262] == b"ustar" && head[345] != 0 {
            let prefix: String = head[345..500]
                .iter()
                .take_while(|b| **b != 0)
                .map(|b| *b as char)
                .collect();
            name = format!("{prefix}/{name}");
        }
        let padded = size.div_ceil(512) * 512;
        let wanted = matches!(kind, b'0' | 0);
        let meta = matches!(kind, b'x' | b'L');
        if !wanted && !meta {
            std::io::copy(&mut (&mut gz).take(padded), &mut std::io::sink()).map_err(|_| bad())?;
            continue;
        }
        let path = long_name.take().unwrap_or(name);
        let path = if strip {
            path.split_once('/')
                .map_or(path.clone(), |(_, rest)| rest.to_string())
        } else {
            path
        };
        if wanted && !(safe(&path) && keep(&path)) {
            std::io::copy(&mut (&mut gz).take(padded), &mut std::io::sink()).map_err(|_| bad())?;
            continue;
        }
        if size > limits.max_member_bytes {
            if wanted {
                return msg(format!("{path} is larger than a pack may hold"));
            }
            return Err(bad());
        }
        let mut data = vec![0u8; padded as usize];
        gz.read_exact(&mut data).map_err(|_| bad())?;
        data.truncate(size as usize);
        if meta {
            // `x` is a pax header ("<len> path=<name>\n" records), `L` a GNU long name.
            let text = String::from_utf8_lossy(&data).into_owned();
            long_name = if kind == b'L' {
                Some(text.trim_end_matches('\0').to_string())
            } else {
                text.lines()
                    .find_map(|l| {
                        l.split_once(' ')
                            .and_then(|(_, kv)| kv.strip_prefix("path="))
                    })
                    .map(str::to_string)
            };
            // A pax name still carries its `package/` prefix, which the next entry strips.
            continue;
        }
        budget.take(size, &path)?;
        out.push(Member { path, data });
    }
    Ok(out)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use flate2::Compression;
    use flate2::write::{DeflateEncoder, GzEncoder};
    use std::io::Write;

    /// A zip with the given files, the first deflated and the rest stored.
    pub fn make_zip(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut central = Vec::new();
        for (i, (name, data)) in files.iter().enumerate() {
            let (method, packed) = if i == 0 {
                let mut e = DeflateEncoder::new(Vec::new(), Compression::default());
                e.write_all(data).unwrap();
                (8u16, e.finish().unwrap())
            } else {
                (0u16, data.to_vec())
            };
            let offset = out.len() as u32;
            let mut local = vec![0x50, 0x4b, 0x03, 0x04, 20, 0, 0, 0];
            local.extend_from_slice(&method.to_le_bytes());
            local.extend_from_slice(&[0; 8]);
            local.extend_from_slice(&(packed.len() as u32).to_le_bytes());
            local.extend_from_slice(&(data.len() as u32).to_le_bytes());
            local.extend_from_slice(&(name.len() as u16).to_le_bytes());
            local.extend_from_slice(&[0, 0]);
            out.extend_from_slice(&local);
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(&packed);
            let mut c = vec![0x50, 0x4b, 0x01, 0x02, 20, 0, 20, 0, 0, 0];
            c.extend_from_slice(&method.to_le_bytes());
            c.extend_from_slice(&[0; 8]);
            c.extend_from_slice(&(packed.len() as u32).to_le_bytes());
            c.extend_from_slice(&(data.len() as u32).to_le_bytes());
            c.extend_from_slice(&(name.len() as u16).to_le_bytes());
            c.extend_from_slice(&[0; 12]);
            c.extend_from_slice(&offset.to_le_bytes());
            c.extend_from_slice(name.as_bytes());
            central.extend_from_slice(&c);
        }
        let at = out.len() as u32;
        out.extend_from_slice(&central);
        out.extend_from_slice(&[0x50, 0x4b, 0x05, 0x06, 0, 0, 0, 0]);
        out.extend_from_slice(&(files.len() as u16).to_le_bytes());
        out.extend_from_slice(&(files.len() as u16).to_le_bytes());
        out.extend_from_slice(&(central.len() as u32).to_le_bytes());
        out.extend_from_slice(&at.to_le_bytes());
        out.extend_from_slice(&[0, 0]);
        out
    }

    /// A gzipped tar with the given files.
    pub fn make_tgz(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut tar = Vec::new();
        for (name, data) in files {
            let mut h = [0u8; 512];
            h[..name.len()].copy_from_slice(name.as_bytes());
            h[124..135].copy_from_slice(format!("{:011o}", data.len()).as_bytes());
            h[156] = b'0';
            tar.extend_from_slice(&h);
            tar.extend_from_slice(data);
            tar.resize(tar.len().div_ceil(512) * 512, 0);
        }
        tar.extend_from_slice(&[0; 1024]);
        let mut g = GzEncoder::new(Vec::new(), Compression::default());
        g.write_all(&tar).unwrap();
        g.finish().unwrap()
    }

    #[test]
    fn zip_members_are_filtered_and_unsafe_names_dropped() {
        let zip = make_zip(&[
            ("PNG/a.png", b"AAAA"),
            ("Audio/b.ogg", b"BBBBBB"),
            ("../evil.png", b"x"),
        ]);
        let got = read_zip(
            &zip,
            &|p| p.ends_with(".png") || p.ends_with(".ogg"),
            Limits::default(),
        )
        .unwrap();
        let names: Vec<_> = got.iter().map(|m| m.path.as_str()).collect();
        assert_eq!(names, ["PNG/a.png", "Audio/b.ogg"]);
        assert_eq!(got[0].data, b"AAAA");
        assert_eq!(got[1].data, b"BBBBBB");
        assert!(read_zip(b"not a zip", &|_| true, Limits::default()).is_err());
        let small = Limits {
            max_member_bytes: 3,
            ..Limits::default()
        };
        assert!(read_zip(&zip, &|p| p == "PNG/a.png", small).is_err());
    }

    #[test]
    fn tgz_members_lose_their_top_folder_and_big_ones_are_refused() {
        let tgz = make_tgz(&[
            ("package/icons/a.svg", b"<svg/>"),
            ("package/README.md", b"hi"),
        ]);
        let got = read_tgz(&tgz, true, &|p| p.starts_with("icons/"), Limits::default()).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].path, "icons/a.svg");
        assert_eq!(got[0].data, b"<svg/>");
        let small = Limits {
            max_member_bytes: 2,
            ..Limits::default()
        };
        assert!(read_tgz(&tgz, true, &|p| p.starts_with("icons/"), small).is_err());
    }
}
