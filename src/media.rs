//! Native media probing for the `image(...)` schema type.
//!
//! Abstract validates image assets without decoding them. Every probe reads
//! only the few header bytes that carry the signature and the dimensions, so
//! validating a project with hundreds of textures costs a handful of tiny
//! reads instead of loading pixel data into memory.
//!
//! Supported formats: PNG, JPEG, GIF, BMP, WebP (lossy, lossless, extended).

use std::fmt;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// The image containers Abstract can probe natively.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageFormat {
    Png,
    Jpeg,
    Gif,
    Bmp,
    Webp,
}

impl ImageFormat {
    /// Maps a file extension (without the dot, any case) to a format.
    pub fn from_extension(extension: &str) -> Option<Self> {
        match extension.to_ascii_lowercase().as_str() {
            "png" => Some(Self::Png),
            "jpg" | "jpeg" => Some(Self::Jpeg),
            "gif" => Some(Self::Gif),
            "bmp" => Some(Self::Bmp),
            "webp" => Some(Self::Webp),
            _ => None,
        }
    }

    /// Canonical lowercase name used in diagnostics.
    pub fn name(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Gif => "gif",
            Self::Bmp => "bmp",
            Self::Webp => "webp",
        }
    }
}

impl fmt::Display for ImageFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Header data recovered by a probe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageInfo {
    pub format: ImageFormat,
    pub width: u32,
    pub height: u32,
}

/// Reads the header of an image file and returns its real format and
/// dimensions. Only header bytes are read; pixel data is never touched.
pub fn probe_image(path: &Path) -> Result<ImageInfo, String> {
    let mut file =
        File::open(path).map_err(|error| format!("could not open '{}': {error}", path.display()))?;
    let mut header = [0u8; 32];
    let read = read_up_to(&mut file, &mut header)?;
    let header = &header[..read];

    if header.len() >= 24 && header.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        return probe_png(header);
    }
    if header.len() >= 4 && header.starts_with(&[0xFF, 0xD8]) {
        return probe_jpeg(&mut file);
    }
    if header.len() >= 10 && (header.starts_with(b"GIF87a") || header.starts_with(b"GIF89a")) {
        return probe_gif(header);
    }
    if header.len() >= 26 && header.starts_with(b"BM") {
        return probe_bmp(header);
    }
    if header.len() >= 30 && header.starts_with(b"RIFF") && &header[8..12] == b"WEBP" {
        return probe_webp(header);
    }
    Err("unrecognized image signature (expected png, jpg, gif, bmp, or webp)".to_string())
}

fn read_up_to(file: &mut File, buffer: &mut [u8]) -> Result<usize, String> {
    let mut total = 0usize;
    while total < buffer.len() {
        match file.read(&mut buffer[total..]) {
            Ok(0) => break,
            Ok(read) => total += read,
            Err(error) => return Err(format!("could not read image header: {error}")),
        }
    }
    Ok(total)
}

fn probe_png(header: &[u8]) -> Result<ImageInfo, String> {
    // Signature (8) + IHDR length (4) + "IHDR" (4) + width (4) + height (4).
    if &header[12..16] != b"IHDR" {
        return Err("png file is missing its IHDR chunk".to_string());
    }
    Ok(ImageInfo {
        format: ImageFormat::Png,
        width: u32::from_be_bytes([header[16], header[17], header[18], header[19]]),
        height: u32::from_be_bytes([header[20], header[21], header[22], header[23]]),
    })
}

fn probe_gif(header: &[u8]) -> Result<ImageInfo, String> {
    Ok(ImageInfo {
        format: ImageFormat::Gif,
        width: u16::from_le_bytes([header[6], header[7]]) as u32,
        height: u16::from_le_bytes([header[8], header[9]]) as u32,
    })
}

fn probe_bmp(header: &[u8]) -> Result<ImageInfo, String> {
    let width = i32::from_le_bytes([header[18], header[19], header[20], header[21]]);
    let height = i32::from_le_bytes([header[22], header[23], header[24], header[25]]);
    Ok(ImageInfo {
        format: ImageFormat::Bmp,
        width: width.unsigned_abs(),
        height: height.unsigned_abs(),
    })
}

fn probe_webp(header: &[u8]) -> Result<ImageInfo, String> {
    match &header[12..16] {
        b"VP8 " => {
            // Lossy: 3-byte frame tag, 3-byte sync code, then 14-bit sizes.
            if header[23..26] != [0x9D, 0x01, 0x2A] {
                return Err("webp (VP8) sync code not found".to_string());
            }
            let width = u16::from_le_bytes([header[26], header[27]]) & 0x3FFF;
            let height = u16::from_le_bytes([header[28], header[29]]) & 0x3FFF;
            Ok(ImageInfo {
                format: ImageFormat::Webp,
                width: width as u32,
                height: height as u32,
            })
        }
        b"VP8L" => {
            // Lossless: signature byte 0x2F, then 14+14 bits (minus one each).
            if header[20] != 0x2F {
                return Err("webp (VP8L) signature byte not found".to_string());
            }
            let bits = u32::from_le_bytes([header[21], header[22], header[23], header[24]]);
            Ok(ImageInfo {
                format: ImageFormat::Webp,
                width: (bits & 0x3FFF) + 1,
                height: ((bits >> 14) & 0x3FFF) + 1,
            })
        }
        b"VP8X" => {
            // Extended: canvas size as 24-bit little-endian values minus one.
            let width = u32::from_le_bytes([header[24], header[25], header[26], 0]) + 1;
            let height = u32::from_le_bytes([header[27], header[28], header[29], 0]) + 1;
            Ok(ImageInfo {
                format: ImageFormat::Webp,
                width,
                height,
            })
        }
        other => Err(format!(
            "unsupported webp variant '{}'",
            String::from_utf8_lossy(other)
        )),
    }
}

fn probe_jpeg(file: &mut File) -> Result<ImageInfo, String> {
    // Walk the segment chain until a start-of-frame marker carries the size.
    // Each iteration reads a 4-byte segment header and seeks past the body,
    // so even large photos cost only a few dozen bytes of reads.
    file.seek(SeekFrom::Start(2))
        .map_err(|error| format!("could not seek jpg stream: {error}"))?;
    for _ in 0..256 {
        let mut marker = [0u8; 2];
        file.read_exact(&mut marker)
            .map_err(|_| "jpg stream ended before a size marker was found".to_string())?;
        if marker[0] != 0xFF {
            return Err("jpg stream is malformed (lost marker alignment)".to_string());
        }
        let mut code = marker[1];
        // Skip padding bytes between segments.
        while code == 0xFF {
            let mut next = [0u8; 1];
            file.read_exact(&mut next)
                .map_err(|_| "jpg stream ended while skipping padding".to_string())?;
            code = next[0];
        }
        match code {
            // Standalone markers without a length field.
            0xD0..=0xD7 | 0x01 => continue,
            // End of image / start of scan: no dimensions were found.
            0xD9 | 0xDA => break,
            _ => {}
        }
        let mut length = [0u8; 2];
        file.read_exact(&mut length)
            .map_err(|_| "jpg segment header is truncated".to_string())?;
        let length = u16::from_be_bytes(length);
        if length < 2 {
            return Err("jpg segment declares an invalid length".to_string());
        }
        let is_start_of_frame = matches!(
            code,
            0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF
        );
        if is_start_of_frame {
            let mut frame = [0u8; 5];
            file.read_exact(&mut frame)
                .map_err(|_| "jpg frame header is truncated".to_string())?;
            return Ok(ImageInfo {
                format: ImageFormat::Jpeg,
                height: u16::from_be_bytes([frame[1], frame[2]]) as u32,
                width: u16::from_be_bytes([frame[3], frame[4]]) as u32,
            });
        }
        file.seek(SeekFrom::Current(i64::from(length) - 2))
            .map_err(|error| format!("could not seek jpg stream: {error}"))?;
    }
    Err("jpg file has no start-of-frame marker with dimensions".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_file(name: &str, bytes: &[u8]) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("abstract_media_{name}_{suffix}"));
        fs::write(&path, bytes).expect("write probe fixture");
        path
    }

    fn png_bytes(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        bytes.extend_from_slice(&13u32.to_be_bytes());
        bytes.extend_from_slice(b"IHDR");
        bytes.extend_from_slice(&width.to_be_bytes());
        bytes.extend_from_slice(&height.to_be_bytes());
        bytes.extend_from_slice(&[8, 6, 0, 0, 0]);
        bytes
    }

    #[test]
    fn probes_png_dimensions_from_header_only() {
        let path = temp_file("png", &png_bytes(128, 64));
        let info = probe_image(&path).expect("png probe");
        assert_eq!(info.format, ImageFormat::Png);
        assert_eq!((info.width, info.height), (128, 64));
        fs::remove_file(path).ok();
    }

    #[test]
    fn probes_gif_dimensions() {
        let mut bytes = b"GIF89a".to_vec();
        bytes.extend_from_slice(&320u16.to_le_bytes());
        bytes.extend_from_slice(&200u16.to_le_bytes());
        bytes.extend_from_slice(&[0, 0, 0]);
        let path = temp_file("gif", &bytes);
        let info = probe_image(&path).expect("gif probe");
        assert_eq!(info.format, ImageFormat::Gif);
        assert_eq!((info.width, info.height), (320, 200));
        fs::remove_file(path).ok();
    }

    #[test]
    fn probes_bmp_dimensions() {
        let mut bytes = vec![0u8; 26];
        bytes[0] = b'B';
        bytes[1] = b'M';
        bytes[18..22].copy_from_slice(&512i32.to_le_bytes());
        bytes[22..26].copy_from_slice(&(-256i32).to_le_bytes());
        let path = temp_file("bmp", &bytes);
        let info = probe_image(&path).expect("bmp probe");
        assert_eq!(info.format, ImageFormat::Bmp);
        assert_eq!((info.width, info.height), (512, 256));
        fs::remove_file(path).ok();
    }

    #[test]
    fn probes_jpeg_dimensions_via_marker_walk() {
        let mut bytes = vec![0xFF, 0xD8];
        // APP0 segment to force the walker to skip a body.
        bytes.extend_from_slice(&[0xFF, 0xE0, 0x00, 0x10]);
        bytes.extend_from_slice(&[0u8; 14]);
        // SOF0 with height 240, width 320.
        bytes.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x11, 0x08, 0x00, 0xF0, 0x01, 0x40]);
        bytes.extend_from_slice(&[0u8; 10]);
        let path = temp_file("jpg", &bytes);
        let info = probe_image(&path).expect("jpg probe");
        assert_eq!(info.format, ImageFormat::Jpeg);
        assert_eq!((info.width, info.height), (320, 240));
        fs::remove_file(path).ok();
    }

    #[test]
    fn probes_webp_lossy_dimensions() {
        let mut bytes = b"RIFF".to_vec();
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(b"WEBP");
        bytes.extend_from_slice(b"VP8 ");
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&[0, 0, 0]);
        bytes.extend_from_slice(&[0x9D, 0x01, 0x2A]);
        bytes.extend_from_slice(&96u16.to_le_bytes());
        bytes.extend_from_slice(&48u16.to_le_bytes());
        bytes.extend_from_slice(&[0, 0]);
        let path = temp_file("webp", &bytes);
        let info = probe_image(&path).expect("webp probe");
        assert_eq!(info.format, ImageFormat::Webp);
        assert_eq!((info.width, info.height), (96, 48));
        fs::remove_file(path).ok();
    }

    #[test]
    fn rejects_files_with_unknown_signatures() {
        let path = temp_file("junk", b"not an image at all");
        let error = probe_image(&path).expect_err("junk should not probe");
        assert!(error.contains("unrecognized image signature"));
        fs::remove_file(path).ok();
    }

    #[test]
    fn detects_extension_content_mismatch_material() {
        // A PNG header probed as PNG regardless of what the file is named.
        let path = temp_file("renamed", &png_bytes(16, 16));
        let info = probe_image(&path).expect("probe");
        assert_eq!(info.format, ImageFormat::Png);
        fs::remove_file(path).ok();
    }
}
