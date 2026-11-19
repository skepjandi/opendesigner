//! The one compact file format assets are stored in: a bundled icon set and every installed pack.
//!
//! `ODA1`, then one zlib stream of records
//! `u16le name_len, u16le tags_len, u32le body_len, name, tags (comma separated), body`.
//! Reading one icon means inflating the stream (a few milliseconds for megabytes), so a pack is
//! one file on disk however many thousand entries it has. `scripts/build-icon-bundle.py` writes
//! the same format for the bundled set.

use std::io::{Read, Write};

use flate2::Compression;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;

use super::{Result, msg};

const MAGIC: &[u8; 4] = b"ODA1";
/// The most a container may inflate to: a damaged or hostile file cannot fill the memory.
pub const MAX_RAW_BYTES: u64 = 96 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub tags: String,
    pub body: Vec<u8>,
}

/// Packs entries into the container.
pub fn encode(entries: &[Entry]) -> Result<Vec<u8>> {
    let mut z = ZlibEncoder::new(MAGIC.to_vec(), Compression::best());
    for e in entries {
        let (Ok(n), Ok(t), Ok(b)) = (
            u16::try_from(e.name.len()),
            u16::try_from(e.tags.len()),
            u32::try_from(e.body.len()),
        ) else {
            return msg(format!("{:?} is too large to store", e.name));
        };
        let mut head = Vec::with_capacity(8);
        head.extend_from_slice(&n.to_le_bytes());
        head.extend_from_slice(&t.to_le_bytes());
        head.extend_from_slice(&b.to_le_bytes());
        z.write_all(&head)
            .and_then(|()| z.write_all(e.name.as_bytes()))
            .and_then(|()| z.write_all(e.tags.as_bytes()))
            .and_then(|()| z.write_all(&e.body))
            .map_err(|e| super::Error::Msg(e.to_string()))?;
    }
    z.finish().map_err(|e| super::Error::Msg(e.to_string()))
}

/// Reads the entries of a container.
pub fn decode(bytes: &[u8]) -> Result<Vec<Entry>> {
    let Some(rest) = bytes.strip_prefix(MAGIC) else {
        return msg("not an asset container");
    };
    let mut raw = Vec::new();
    ZlibDecoder::new(rest)
        .take(MAX_RAW_BYTES + 1)
        .read_to_end(&mut raw)
        .map_err(|e| super::Error::Msg(format!("the asset container is damaged: {e}")))?;
    if raw.len() as u64 > MAX_RAW_BYTES {
        return msg("the asset container inflates to more than it may");
    }
    let mut out = Vec::new();
    let mut at = 0usize;
    while at < raw.len() {
        let Some(head) = raw.get(at..at + 8) else {
            return msg("the asset container ends inside a record");
        };
        let n = u16::from_le_bytes([head[0], head[1]]) as usize;
        let t = u16::from_le_bytes([head[2], head[3]]) as usize;
        let b = u32::from_le_bytes([head[4], head[5], head[6], head[7]]) as usize;
        at += 8;
        let Some(data) = raw.get(at..at + n + t + b) else {
            return msg("the asset container ends inside a record");
        };
        let text = |s: &[u8]| String::from_utf8_lossy(s).into_owned();
        out.push(Entry {
            name: text(&data[..n]),
            tags: text(&data[n..n + t]),
            body: data[n + t..].to_vec(),
        });
        at += n + t + b;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_survive_a_round_trip_and_garbage_is_refused() {
        let entries = vec![
            Entry {
                name: "a".into(),
                tags: "x,y".into(),
                body: b"<path/>".to_vec(),
            },
            Entry {
                name: "b/c.ogg".into(),
                tags: String::new(),
                body: vec![0, 1, 2, 255],
            },
        ];
        let bytes = encode(&entries).unwrap();
        assert_eq!(decode(&bytes).unwrap(), entries);
        assert!(decode(b"nope").is_err());
        assert!(decode(&bytes[..bytes.len() - 6]).is_err());
    }
}
