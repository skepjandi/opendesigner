//! Files made to be sent to someone: JPEG pictures and PDF documents of rendered frames. A frame
//! with transparency is flattened onto white, because neither format keeps it.

use image::ExtendedColorType;
use image::codecs::jpeg::JpegEncoder;
use tiny_skia::Pixmap;

use crate::RenderError;

/// Pixels to PDF points: a PDF page is 72 points to the inch, a screen picture is 96 pixels.
const POINTS_PER_PIXEL: f32 = 0.75;
const PDF_JPEG_QUALITY: u8 = 92;

fn rgb_over_white(pixmap: &Pixmap) -> Vec<u8> {
    let mut rgb = Vec::with_capacity(pixmap.width() as usize * pixmap.height() as usize * 3);
    for px in pixmap.data().chunks_exact(4) {
        let over_white = 255 - px[3];
        rgb.extend([
            px[0].saturating_add(over_white),
            px[1].saturating_add(over_white),
            px[2].saturating_add(over_white),
        ]);
    }
    rgb
}

/// A baseline JPEG of `pixmap`; `quality` is 1 to 100.
pub fn encode_jpeg(pixmap: &Pixmap, quality: u8) -> Result<Vec<u8>, RenderError> {
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, quality.clamp(1, 100))
        .encode(
            &rgb_over_white(pixmap),
            pixmap.width(),
            pixmap.height(),
            ExtendedColorType::Rgb8,
        )
        .map_err(|e| RenderError::Encode(e.to_string()))?;
    Ok(out)
}

/// A PDF with one page per picture, each page the size of its picture at 96 pixels to the inch.
/// The pictures are embedded as JPEG (a raster document, not vector art).
pub fn encode_pdf(pages: &[Pixmap]) -> Result<Vec<u8>, RenderError> {
    if pages.is_empty() {
        return Err(RenderError::Encode("a PDF needs at least one page".into()));
    }
    let mut jpegs = Vec::with_capacity(pages.len());
    for page in pages {
        jpegs.push(encode_jpeg(page, PDF_JPEG_QUALITY)?);
    }
    // object numbers: 1 catalog, 2 page tree, then per page: page, content, image
    let kids: Vec<String> = (0..pages.len())
        .map(|i| format!("{} 0 R", 3 + i * 3))
        .collect();
    let mut objects: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        format!(
            "<< /Type /Pages /Kids [{}] /Count {} >>",
            kids.join(" "),
            pages.len()
        )
        .into_bytes(),
    ];
    for (i, (page, jpeg)) in pages.iter().zip(&jpegs).enumerate() {
        let (w, h) = (page.width(), page.height());
        let (pw, ph) = (w as f32 * POINTS_PER_PIXEL, h as f32 * POINTS_PER_PIXEL);
        let (content, image) = (4 + i * 3, 5 + i * 3);
        objects.push(
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {pw:.2} {ph:.2}] /Resources << /XObject << /Im0 {image} 0 R >> >> /Contents {content} 0 R >>"
            )
            .into_bytes(),
        );
        let draw = format!("q {pw:.2} 0 0 {ph:.2} 0 0 cm /Im0 Do Q");
        objects
            .push(format!("<< /Length {} >>\nstream\n{draw}\nendstream", draw.len()).into_bytes());
        let mut stream = format!(
            "<< /Type /XObject /Subtype /Image /Width {w} /Height {h} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length {} >>\nstream\n",
            jpeg.len()
        )
        .into_bytes();
        stream.extend_from_slice(jpeg);
        stream.extend_from_slice(b"\nendstream");
        objects.push(stream);
    }
    let mut out = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets = Vec::with_capacity(objects.len());
    for (n, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", n + 1).as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for offset in &offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: u32, h: u32, rgba: [u8; 4]) -> Pixmap {
        let mut pixmap = Pixmap::new(w, h).unwrap();
        for px in pixmap.data_mut().chunks_exact_mut(4) {
            px.copy_from_slice(&rgba);
        }
        pixmap
    }

    #[test]
    fn a_jpeg_decodes_back_to_the_same_size_and_flattens_transparency_onto_white() {
        let jpeg = encode_jpeg(&solid(40, 24, [0, 0, 0, 0]), 90).unwrap();
        assert_eq!(&jpeg[..2], [0xFF, 0xD8]);
        let decoded = image::load_from_memory(&jpeg).unwrap().to_rgb8();
        assert_eq!(decoded.dimensions(), (40, 24));
        assert!(decoded.get_pixel(5, 5).0.iter().all(|c| *c >= 250));
    }

    fn find(haystack: &[u8], needle: &str) -> Vec<usize> {
        let n = needle.as_bytes();
        (0..haystack.len().saturating_sub(n.len() - 1))
            .filter(|&i| &haystack[i..i + n.len()] == n)
            .collect()
    }

    #[test]
    fn a_pdf_has_a_valid_cross_reference_table_and_one_page_per_picture() {
        let pdf = encode_pdf(&[
            solid(100, 50, [10, 20, 30, 255]),
            solid(40, 40, [255, 0, 0, 255]),
        ])
        .unwrap();
        assert!(pdf.starts_with(b"%PDF-1.4"));
        assert!(pdf.ends_with(b"%%EOF\n"));
        assert_eq!(find(&pdf, "/Type /Page ").len(), 2);
        assert_eq!(find(&pdf, "/MediaBox [0 0 75.00 37.50]").len(), 1);
        let marker = find(&pdf, "startxref\n")[0] + "startxref\n".len();
        let tail = String::from_utf8_lossy(&pdf[marker..]).into_owned();
        let start: usize = tail.lines().next().unwrap().parse().unwrap();
        assert!(pdf[start..].starts_with(b"xref"));
        let table = String::from_utf8_lossy(&pdf[start..]).into_owned();
        let offsets: Vec<usize> = table
            .lines()
            .skip(3)
            .take_while(|l| l.ends_with(" n "))
            .map(|l| l[..10].parse().unwrap())
            .collect();
        assert_eq!(offsets.len(), 8);
        for (i, offset) in offsets.iter().enumerate() {
            let expected = format!("{} 0 obj", i + 1);
            assert!(
                pdf[*offset..].starts_with(expected.as_bytes()),
                "object {} is not at {offset}",
                i + 1
            );
        }
    }

    #[test]
    fn an_empty_pdf_is_refused() {
        assert!(encode_pdf(&[]).is_err());
    }
}
