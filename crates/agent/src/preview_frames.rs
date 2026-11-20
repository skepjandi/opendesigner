//! A preview picture as the editor keeps it in memory: raw premultiplied RGBA squeezed by a
//! run-length code over whole pixels. Design frames are mostly flat colour, so the code shrinks
//! them a lot and costs about a millisecond either way; a picture it cannot shrink (footage) is
//! kept raw. PNG was both slower to make and slower for Qt to read back.

use od_render::Pixmap;

const BYTES_PER_PIXEL: usize = 4;
/// A token below this copies the next `token + 1` pixels; from it, repeats the next pixel
/// `token - REPEAT_BASE + MIN_REPEAT` times.
const REPEAT_BASE: u8 = 128;
const MIN_REPEAT: usize = 3;
const MAX_LITERAL: usize = REPEAT_BASE as usize;
const MAX_REPEAT: usize = 255 - REPEAT_BASE as usize + MIN_REPEAT;

/// One picture: its size and its pixels, run-length coded when that is smaller.
#[derive(Debug, PartialEq, Eq)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    coded: bool,
    bytes: Vec<u8>,
}

impl Frame {
    pub fn encode(pixmap: &Pixmap) -> Frame {
        let raw = pixmap.data();
        let packed = pack(raw);
        let coded = packed.len() < raw.len();
        Frame {
            width: pixmap.width(),
            height: pixmap.height(),
            coded,
            bytes: if coded { packed } else { raw.to_vec() },
        }
    }

    /// Bytes this picture takes in memory.
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// The premultiplied RGBA pixels, `width * height * 4` bytes, into `out` (replacing it).
    pub fn decode_into(&self, out: &mut Vec<u8>) {
        out.clear();
        if self.coded {
            unpack(&self.bytes, out);
        } else {
            out.extend_from_slice(&self.bytes);
        }
    }
}

fn pack(raw: &[u8]) -> Vec<u8> {
    let count = raw.len() / BYTES_PER_PIXEL;
    let pixel = |i: usize| &raw[i * BYTES_PER_PIXEL..(i + 1) * BYTES_PER_PIXEL];
    let mut out = Vec::with_capacity(raw.len() / 4);
    let mut i = 0;
    while i < count {
        let run = (i..count)
            .take(MAX_REPEAT)
            .take_while(|&k| pixel(k) == pixel(i))
            .count();
        if run >= MIN_REPEAT {
            out.push(REPEAT_BASE + (run - MIN_REPEAT) as u8);
            out.extend_from_slice(pixel(i));
            i += run;
            continue;
        }
        let start = i;
        while i < count && i - start < MAX_LITERAL && !starts_run(raw, i) {
            i += 1;
        }
        out.push((i - start - 1) as u8);
        for k in start..i {
            out.extend_from_slice(pixel(k));
        }
    }
    out
}

/// Whether the pixels at `at` start a run of at least [`MIN_REPEAT`] equal pixels.
fn starts_run(raw: &[u8], at: usize) -> bool {
    let count = raw.len() / BYTES_PER_PIXEL;
    count >= at + MIN_REPEAT
        && (1..MIN_REPEAT).all(|k| {
            raw[(at + k) * BYTES_PER_PIXEL..(at + k + 1) * BYTES_PER_PIXEL]
                == raw[at * BYTES_PER_PIXEL..(at + 1) * BYTES_PER_PIXEL]
        })
}

fn unpack(coded: &[u8], out: &mut Vec<u8>) {
    let mut i = 0;
    while i < coded.len() {
        let token = coded[i];
        i += 1;
        if token >= REPEAT_BASE {
            let Some(pixel) = coded.get(i..i + BYTES_PER_PIXEL) else {
                return;
            };
            for _ in 0..usize::from(token - REPEAT_BASE) + MIN_REPEAT {
                out.extend_from_slice(pixel);
            }
            i += BYTES_PER_PIXEL;
        } else {
            let end = i + (usize::from(token) + 1) * BYTES_PER_PIXEL;
            let Some(literal) = coded.get(i..end) else {
                return;
            };
            out.extend_from_slice(literal);
            i = end;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixmap_of(w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 4]) -> Pixmap {
        let mut p = Pixmap::new(w, h).unwrap();
        for y in 0..h {
            for x in 0..w {
                let at = ((y * w + x) * 4) as usize;
                p.data_mut()[at..at + 4].copy_from_slice(&f(x, y));
            }
        }
        p
    }

    fn round_trip(p: &Pixmap) -> Frame {
        let frame = Frame::encode(p);
        let mut out = Vec::new();
        frame.decode_into(&mut out);
        assert_eq!(out, p.data(), "decoded pixels equal the original");
        assert_eq!((frame.width, frame.height), (p.width(), p.height()));
        frame
    }

    #[test]
    fn flat_pictures_shrink_a_lot_and_round_trip() {
        let flat = pixmap_of(360, 640, |_, _| [10, 20, 30, 255]);
        let frame = round_trip(&flat);
        assert!(frame.len() * 100 < flat.data().len(), "{}", frame.len());
        let bands = pixmap_of(360, 640, |_, y| [(y / 40) as u8 * 9, 0, 0, 255]);
        assert!(round_trip(&bands).len() * 20 < bands.data().len());
    }

    #[test]
    fn noise_stays_raw_and_odd_runs_round_trip() {
        let noise = pixmap_of(97, 61, |x, y| {
            [(x * 7 + y * 13) as u8, (x * y) as u8, (x ^ y) as u8, 255]
        });
        let frame = round_trip(&noise);
        assert!(frame.len() <= noise.data().len(), "never bigger than raw");
        for w in [1, 2, 3, 4, 127, 128, 129, 130, 131, 132, 257, 258, 259] {
            round_trip(&pixmap_of(w, 3, |x, _| {
                if x % 5 < 3 {
                    [1, 2, 3, 4]
                } else {
                    [9, 9, 9, 9]
                }
            }));
            round_trip(&pixmap_of(w, 1, |x, _| [x as u8, 0, 0, 0]));
        }
    }

    #[test]
    fn a_damaged_code_decodes_to_something_shorter_instead_of_panicking() {
        let flat = pixmap_of(16, 16, |_, _| [1, 2, 3, 4]);
        let mut frame = Frame::encode(&flat);
        frame.bytes.truncate(frame.bytes.len() - 2);
        let mut out = Vec::new();
        frame.decode_into(&mut out);
        assert!(out.len() < flat.data().len());
    }
}
