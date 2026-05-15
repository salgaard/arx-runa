/// In-RAM EXIF/XMP/IPTC stripping for image formats before encryption.
///
/// Detection uses magic bytes, not file extension.
/// Unsupported containers pass through unchanged.
/// The source file on disk is never modified.
///
/// Supported: JPEG (APP1/APP2/APP13 segment removal), PNG (eXIf/tEXt/iTXt/zTXt chunk removal).
/// Returns `true` if the byte slice begins with a recognised image magic number.
pub(crate) fn is_image_magic(bytes: &[u8]) -> bool {
    is_jpeg(bytes) || is_png(bytes)
}

/// Strips EXIF, XMP, and IPTC metadata from image bytes and returns the cleaned bytes.
///
/// If the format is not recognised, the original `bytes` are returned as-is.
pub(crate) fn strip_exif(bytes: Vec<u8>) -> Vec<u8> {
    if is_jpeg(&bytes) {
        strip_jpeg_metadata(bytes)
    } else if is_png(&bytes) {
        strip_png_metadata(bytes)
    } else {
        bytes
    }
}

fn is_jpeg(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0xFF, 0xD8])
}

fn is_png(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A])
}

/// Rewrites the JPEG segment stream, dropping APP1 (EXIF/XMP), APP2 (XMP/ICC), and APP13 (IPTC).
///
/// APP0 (JFIF) and all image-data segments are preserved so the output remains
/// a valid JPEG.  After the SOS marker the compressed bitstream is copied verbatim.
fn strip_jpeg_metadata(bytes: Vec<u8>) -> Vec<u8> {
    if bytes.len() < 4 {
        return bytes;
    }

    let mut out = Vec::with_capacity(bytes.len());
    out.extend_from_slice(&bytes[0..2]); // SOI (FF D8)

    let mut pos = 2;
    while pos + 1 < bytes.len() {
        if bytes[pos] != 0xFF {
            // Sync error — pass remainder through unchanged
            out.extend_from_slice(&bytes[pos..]);
            break;
        }
        let marker = bytes[pos + 1];

        // Fill bytes between markers
        if marker == 0x00 {
            pos += 1;
            continue;
        }

        // SOI (duplicate) or EOI — no length field
        if marker == 0xD8 || marker == 0xD9 {
            out.push(0xFF);
            out.push(marker);
            pos += 2;
            if marker == 0xD9 {
                break;
            }
            continue;
        }

        // RST0-RST7 — no length field
        if (0xD0..=0xD7).contains(&marker) {
            out.push(0xFF);
            out.push(marker);
            pos += 2;
            continue;
        }

        // All remaining segments carry a 2-byte length (includes the length bytes)
        if pos + 3 >= bytes.len() {
            break;
        }
        let seg_len = u16::from_be_bytes([bytes[pos + 2], bytes[pos + 3]]) as usize;
        let seg_end = pos + 2 + seg_len;
        if seg_end > bytes.len() {
            break;
        }

        // Drop APP1 (0xE1 — EXIF/XMP), APP2 (0xE2 — XMP extended/FlashPix/ICC), APP13 (0xED — IPTC)
        let should_drop = marker == 0xE1 || marker == 0xE2 || marker == 0xED;
        if !should_drop {
            out.extend_from_slice(&bytes[pos..seg_end]);
        }

        pos = seg_end;

        // After SOS header, the compressed bitstream follows without segment framing
        if marker == 0xDA {
            out.extend_from_slice(&bytes[pos..]);
            break;
        }
    }

    out
}

/// Rewrites the PNG chunk stream, dropping metadata-only chunks.
///
/// Dropped chunk types: `eXIf` (EXIF), `tEXt`, `iTXt`, `zTXt` (text metadata).
/// All structural and image-data chunks (IHDR, IDAT, PLTE, IEND, etc.) are kept.
fn strip_png_metadata(bytes: Vec<u8>) -> Vec<u8> {
    const SIGNATURE_LEN: usize = 8;
    if bytes.len() < SIGNATURE_LEN + 12 {
        return bytes;
    }

    let mut out = Vec::with_capacity(bytes.len());
    out.extend_from_slice(&bytes[0..SIGNATURE_LEN]); // PNG signature

    let mut pos = SIGNATURE_LEN;
    while pos + 12 <= bytes.len() {
        let chunk_data_len =
            u32::from_be_bytes([bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]])
                as usize;
        let chunk_type = &bytes[pos + 4..pos + 8];
        let total_chunk_len = 4 + 4 + chunk_data_len + 4; // length + type + data + CRC

        if pos + total_chunk_len > bytes.len() {
            break;
        }

        let should_drop = matches!(chunk_type, b"eXIf" | b"tEXt" | b"iTXt" | b"zTXt");
        if !should_drop {
            out.extend_from_slice(&bytes[pos..pos + total_chunk_len]);
        }

        pos += total_chunk_len;
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_image_magic_detects_jpeg() {
        assert!(is_image_magic(&[0xFF, 0xD8, 0xFF, 0xE1]));
    }

    #[test]
    fn test_is_image_magic_detects_png() {
        assert!(is_image_magic(&[
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A
        ]));
    }

    #[test]
    fn test_is_image_magic_rejects_text() {
        assert!(!is_image_magic(b"hello world"));
    }

    #[test]
    fn test_strip_exif_passthrough_for_unknown_format() {
        let bytes = b"not an image".to_vec();
        assert_eq!(strip_exif(bytes.clone()), bytes);
    }

    #[test]
    fn test_strip_jpeg_metadata_removes_app1_preserves_other_segments() {
        // Minimal JPEG: SOI + APP0 + APP1(EXIF) + EOI
        let mut jpeg = Vec::new();
        jpeg.extend_from_slice(&[0xFF, 0xD8]); // SOI

        // APP0 (JFIF) — keep
        let app0_data = b"JFIF\x00\x01\x01\x00\x00\x01\x00\x01\x00\x00";
        let app0_len = (app0_data.len() + 2) as u16;
        jpeg.push(0xFF);
        jpeg.push(0xE0);
        jpeg.extend_from_slice(&app0_len.to_be_bytes());
        jpeg.extend_from_slice(app0_data);

        // APP1 (EXIF) — should be dropped
        let app1_data = b"Exif\x00\x00fake_exif_data";
        let app1_len = (app1_data.len() + 2) as u16;
        jpeg.push(0xFF);
        jpeg.push(0xE1);
        jpeg.extend_from_slice(&app1_len.to_be_bytes());
        jpeg.extend_from_slice(app1_data);

        // EOI
        jpeg.extend_from_slice(&[0xFF, 0xD9]);

        let stripped = strip_exif(jpeg);

        // APP0 should be present
        assert!(stripped.windows(2).any(|w| w == [0xFF, 0xE0]));
        // APP1 should be absent
        assert!(!stripped.windows(2).any(|w| w == [0xFF, 0xE1]));
        // SOI and EOI preserved
        assert_eq!(&stripped[0..2], &[0xFF, 0xD8]);
        assert_eq!(&stripped[stripped.len() - 2..], &[0xFF, 0xD9]);
    }

    #[test]
    fn test_strip_png_metadata_removes_exif_chunk_preserves_ihdr() {
        let mut png = Vec::new();
        // PNG signature
        png.extend_from_slice(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);

        // IHDR chunk — keep
        let ihdr_data = [0u8; 13];
        png.extend_from_slice(&(13u32).to_be_bytes()); // length
        png.extend_from_slice(b"IHDR");
        png.extend_from_slice(&ihdr_data);
        png.extend_from_slice(&[0u8; 4]); // CRC (dummy)

        // eXIf chunk — drop
        let exif_data = b"fake_exif";
        png.extend_from_slice(&(exif_data.len() as u32).to_be_bytes());
        png.extend_from_slice(b"eXIf");
        png.extend_from_slice(exif_data);
        png.extend_from_slice(&[0u8; 4]); // CRC (dummy)

        // IEND chunk — keep
        png.extend_from_slice(&0u32.to_be_bytes()); // length = 0
        png.extend_from_slice(b"IEND");
        png.extend_from_slice(&[0xAE, 0x42, 0x60, 0x82]); // CRC

        let stripped = strip_exif(png);

        // IHDR present
        assert!(stripped.windows(4).any(|w| w == *b"IHDR"));
        // eXIf absent
        assert!(!stripped.windows(4).any(|w| w == *b"eXIf"));
        // IEND present
        assert!(stripped.windows(4).any(|w| w == *b"IEND"));
    }
}
