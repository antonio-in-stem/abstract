//! Native media probing for the `image(...)` schema type (SPEC §4.4.7.1).
//!
//! Abstract validates image assets without decoding them. A probe reads a
//! bounded prefix of the file — at most [`PROBE_BUDGET`] bytes — decides the
//! format from the signature at offset 0, and reads the dimensions out of the
//! header, so validating a project with hundreds of textures costs a handful
//! of tiny reads instead of loading pixel data into memory.
//!
//! Supported formats: PNG, JPEG, GIF, BMP, WebP (lossy, lossless, extended).
//!
//! Three rules of §4.4.7.1 shape the whole module, and each of them was a bug
//! before it was a rule:
//!
//! - **Each format's byte requirement gates only that format.** A lossless
//!   WebP header is complete at 25 bytes; demanding the 30 bytes a lossy WebP
//!   needs reported a valid file as having no signature at all.
//! - **A short file is never an unrecognised signature, and an unrecognised
//!   file is never truncated.** The two are different mistakes — a wrong file
//!   and a broken file — and [`ProbeError`] keeps them apart so that the
//!   diagnostic sends the author to the right place.
//! - **Nothing depends on the file's total length.** The JPEG walk is bounded
//!   by the bytes it has read, never by a segment count and never by a length
//!   the filesystem reports, so the same file probes the same way whatever is
//!   appended to it.

use std::fmt;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// The most a probe may read from the head of a file (SPEC §4.4.7.1).
///
/// Every signature this module knows sits at offset 0 and is decided within
/// the first 30 bytes, so only the JPEG segment walk can approach this budget.
/// That is also why no probe here can produce the `no header found in the
/// first 65536 bytes.` note of §4.4.7.1: deciding the format never reaches the
/// budget, and a JPEG that exhausts it has decided its format and failed to
/// find a start-of-frame, which is [`ProbeError::NoStartOfFrame`].
pub const PROBE_BUDGET: u64 = 65_536;

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

    /// Canonical lowercase name used in diagnostics. Every diagnostic spells
    /// the JPEG format `jpg`, whatever spelling the schema used (SPEC §4.4.7).
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

/// Why a probe failed.
///
/// The variants exist so that a caller can attach the exact note SPEC
/// §4.4.7.1 requires on E422 without parsing a message: [`ProbeError::note`]
/// returns it. [`fmt::Display`] renders the human half of the diagnostic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProbeError {
    /// The file could not be opened or read.
    Io(String),
    /// No signature matched. Carries the first bytes of the file, rendered.
    UnrecognisedSignature(String),
    /// A signature matched, but the file is shorter than that format's byte
    /// requirement.
    Truncated {
        format: ImageFormat,
        needed: usize,
        found: usize,
    },
    /// A BMP declares a width of zero or less.
    MalformedBmpWidth(i32),
    /// A decoded dimension lies outside the range its format allows. The
    /// ceiling depends on the variant, not only on the format: the two WebP
    /// variants that pack a dimension into 14 bits stop at 16 383, while the
    /// extended variant reaches 16 777 216 (SPEC §4.4.7.1).
    MalformedCanvasSize {
        format: ImageFormat,
        width: u32,
        height: u32,
        ceiling: u32,
    },
    /// A JPEG walk reached the start of scan, the end of the file or the byte
    /// budget without meeting a start-of-frame marker.
    NoStartOfFrame,
    /// The signature matched but the header contradicts the format.
    Malformed(String),
}

impl ProbeError {
    /// The note SPEC §4.4.7.1 attaches to E422 for this failure, without the
    /// `note: ` prefix. `None` where the specification prescribes no note.
    pub fn note(&self) -> Option<&'static str> {
        match self {
            Self::Truncated { .. } => Some("file is truncated."),
            Self::MalformedBmpWidth(_) => Some("malformed BMP width."),
            Self::MalformedCanvasSize { .. } => Some("malformed canvas size."),
            Self::NoStartOfFrame => Some("no start-of-frame marker."),
            Self::Io(_) | Self::UnrecognisedSignature(_) | Self::Malformed(_) => None,
        }
    }
}

impl fmt::Display for ProbeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(message) => f.write_str(message),
            Self::UnrecognisedSignature(bytes) => {
                write!(f, "unrecognised image signature; the file begins {bytes}")
            }
            Self::Truncated {
                format,
                needed,
                found,
            } => write!(
                f,
                "{format} header needs at least {needed} bytes; the file has {found}"
            ),
            Self::MalformedBmpWidth(width) => {
                write!(f, "bmp declares a width of {width}; it must be at least 1")
            }
            Self::MalformedCanvasSize {
                format,
                width,
                height,
                ceiling,
            } => write!(
                f,
                "{format} declares a canvas of {width}x{height}, outside the range 1..{ceiling}"
            ),
            Self::NoStartOfFrame => {
                f.write_str("jpg stream carries no start-of-frame marker with dimensions")
            }
            Self::Malformed(message) => f.write_str(message),
        }
    }
}

/// Bytes each format's header needs before it yields dimensions
/// (SPEC §4.4.7.1). One format's requirement never gates another's.
const PNG_HEADER_BYTES: usize = 24;
const GIF_HEADER_BYTES: usize = 10;
const BMP_HEADER_BYTES: usize = 26;
const WEBP_VARIANT_BYTES: usize = 16;
const WEBP_LOSSLESS_BYTES: usize = 25;
const WEBP_LOSSY_BYTES: usize = 30;

/// The signature of a PNG file.
const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

/// Reads the header of an image file and returns its real format and
/// dimensions, whatever the file is named. Pixel data is never touched.
pub fn probe_image(path: &Path) -> Result<ImageInfo, ProbeError> {
    let mut file = File::open(path)
        .map_err(|error| ProbeError::Io(format!("could not open '{}': {error}", path.display())))?;
    let mut buffer = [0u8; 32];
    let read = read_up_to(&mut file, &mut buffer)?;
    let header = &buffer[..read];

    let info = if header.starts_with(&PNG_SIGNATURE) {
        require_bytes(ImageFormat::Png, header, PNG_HEADER_BYTES)?;
        probe_png(header)?
    } else if header.starts_with(&[0xFF, 0xD8]) {
        probe_jpeg(&mut file)?
    } else if header.starts_with(b"GIF87a") || header.starts_with(b"GIF89a") {
        require_bytes(ImageFormat::Gif, header, GIF_HEADER_BYTES)?;
        probe_gif(header)?
    } else if header.starts_with(b"BM") {
        require_bytes(ImageFormat::Bmp, header, BMP_HEADER_BYTES)?;
        probe_bmp(header)?
    } else if header.len() >= 12 && header.starts_with(b"RIFF") && &header[8..12] == b"WEBP" {
        require_bytes(ImageFormat::Webp, header, WEBP_VARIANT_BYTES)?;
        probe_webp(header)?
    } else {
        return Err(ProbeError::UnrecognisedSignature(render_first_bytes(
            header,
        )));
    };

    Ok(info)
}

/// Builds an [`ImageInfo`] after checking the canvas: every decoded dimension
/// is at least 1 and at most `ceiling`, which each format and variant fixes
/// for itself (SPEC §4.4.7.1). For VP8X this is the only validation the
/// variant admits, since it carries no signature byte of its own, so it is
/// applied there too even though a 24-bit field can never overflow it.
fn canvas(
    format: ImageFormat,
    width: u32,
    height: u32,
    ceiling: u32,
) -> Result<ImageInfo, ProbeError> {
    if width < 1 || height < 1 || width > ceiling || height > ceiling {
        return Err(ProbeError::MalformedCanvasSize {
            format,
            width,
            height,
            ceiling,
        });
    }
    Ok(ImageInfo {
        format,
        width,
        height,
    })
}

/// Refuses a file whose signature is valid but whose header stops short. A
/// short file is never reported as an unrecognised signature.
fn require_bytes(format: ImageFormat, header: &[u8], needed: usize) -> Result<(), ProbeError> {
    if header.len() < needed {
        return Err(ProbeError::Truncated {
            format,
            needed,
            found: header.len(),
        });
    }
    Ok(())
}

/// Renders the head of a file for the "no signature" diagnostic: the first
/// eight bytes in hexadecimal, with their printable ASCII beside them.
fn render_first_bytes(header: &[u8]) -> String {
    if header.is_empty() {
        return "nothing: the file is empty".to_string();
    }
    let shown = &header[..header.len().min(8)];
    let hex: Vec<String> = shown.iter().map(|byte| format!("{byte:02X}")).collect();
    let ascii: String = shown
        .iter()
        .map(|&byte| {
            if (0x20..0x7F).contains(&byte) {
                byte as char
            } else {
                '.'
            }
        })
        .collect();
    format!("{} ('{}')", hex.join(" "), ascii)
}

fn read_up_to(file: &mut File, buffer: &mut [u8]) -> Result<usize, ProbeError> {
    let mut total = 0usize;
    while total < buffer.len() {
        match file.read(&mut buffer[total..]) {
            Ok(0) => break,
            Ok(read) => total += read,
            Err(error) => {
                return Err(ProbeError::Io(format!(
                    "could not read image header: {error}"
                )))
            }
        }
    }
    Ok(total)
}

fn probe_png(header: &[u8]) -> Result<ImageInfo, ProbeError> {
    // Signature (8) + IHDR length (4) + "IHDR" (4) + width (4) + height (4).
    if &header[12..16] != b"IHDR" {
        return Err(ProbeError::Malformed(
            "png file is missing its IHDR chunk".to_string(),
        ));
    }
    canvas(
        ImageFormat::Png,
        u32::from_be_bytes([header[16], header[17], header[18], header[19]]),
        u32::from_be_bytes([header[20], header[21], header[22], header[23]]),
        u32::MAX,
    )
}

fn probe_gif(header: &[u8]) -> Result<ImageInfo, ProbeError> {
    canvas(
        ImageFormat::Gif,
        u32::from(u16::from_le_bytes([header[6], header[7]])),
        u32::from(u16::from_le_bytes([header[8], header[9]])),
        u32::MAX,
    )
}

fn probe_bmp(header: &[u8]) -> Result<ImageInfo, ProbeError> {
    let width = i32::from_le_bytes([header[18], header[19], header[20], header[21]]);
    let height = i32::from_le_bytes([header[22], header[23], header[24], header[25]]);
    // A negative height marks a top-down bitmap and its absolute value is the
    // height; a negative width means nothing at all. Taking the absolute value
    // of both turned a corrupt file into a plausible size.
    if width <= 0 {
        return Err(ProbeError::MalformedBmpWidth(width));
    }
    canvas(
        ImageFormat::Bmp,
        width as u32,
        height.unsigned_abs(),
        u32::MAX,
    )
}

fn probe_webp(header: &[u8]) -> Result<ImageInfo, ProbeError> {
    match &header[12..16] {
        b"VP8 " => {
            // Lossy: chunk size, 3-byte frame tag, the sync code at 23, then
            // two 14-bit sizes ending at byte 30.
            require_bytes(ImageFormat::Webp, header, WEBP_LOSSY_BYTES)?;
            if header[23..26] != [0x9D, 0x01, 0x2A] {
                return Err(ProbeError::Malformed(
                    "webp (VP8) sync code not found".to_string(),
                ));
            }
            canvas(
                ImageFormat::Webp,
                u32::from(u16::from_le_bytes([header[26], header[27]]) & 0x3FFF),
                u32::from(u16::from_le_bytes([header[28], header[29]]) & 0x3FFF),
                WEBP_14_BIT_MAX,
            )
        }
        b"VP8L" => {
            // Lossless: the signature byte at 20, then 14+14 bits packed into
            // the four bytes at 21, each minus one. The header is complete at
            // 25 bytes — not 30, which is what a shared gate used to demand.
            require_bytes(ImageFormat::Webp, header, WEBP_LOSSLESS_BYTES)?;
            if header[20] != 0x2F {
                return Err(ProbeError::Malformed(
                    "webp (VP8L) signature byte not found".to_string(),
                ));
            }
            let bits = u32::from_le_bytes([header[21], header[22], header[23], header[24]]);
            canvas(
                ImageFormat::Webp,
                (bits & 0x3FFF) + 1,
                ((bits >> 14) & 0x3FFF) + 1,
                WEBP_14_BIT_MAX,
            )
        }
        b"VP8X" => {
            // Extended: the canvas as two 24-bit little-endian values at 24
            // and 27, each plus one. VP8X carries no signature byte, so the
            // canvas range check is the only validation it can receive.
            require_bytes(ImageFormat::Webp, header, WEBP_LOSSY_BYTES)?;
            canvas(
                ImageFormat::Webp,
                u32::from_le_bytes([header[24], header[25], header[26], 0]) + 1,
                u32::from_le_bytes([header[27], header[28], header[29], 0]) + 1,
                WEBP_EXTENDED_MAX,
            )
        }
        other => Err(ProbeError::Malformed(format!(
            "unsupported webp variant '{}'",
            String::from_utf8_lossy(other)
        ))),
    }
}

/// The lossy and lossless WebP variants pack a dimension into 14 bits, so
/// neither can exceed this (SPEC §4.4.7.1). VP8 writes the value directly and
/// VP8L writes it minus one, which is why only VP8L can express 16 384.
const WEBP_14_BIT_MAX: u32 = 16_383;

/// The extended WebP variant writes a 24-bit canvas minus one (SPEC §4.4.7.1).
const WEBP_EXTENDED_MAX: u32 = 16_777_216;

fn probe_jpeg(file: &mut File) -> Result<ImageInfo, ProbeError> {
    // Walk the segment chain until a start-of-frame marker carries the size.
    //
    // The walk is bounded by the bytes it has read, and by nothing else. A
    // segment count cannot bound the work, because one length field moves up
    // to 65535 bytes; the file's own length must not bound it either, since
    // SPEC §4.4.7.1 forbids a probe from depending on it. Every byte the walk
    // moves over — fill bytes, segment headers and skipped bodies alike —
    // counts against PROBE_BUDGET, and reaching that budget, the start of
    // scan, or the end of the file without a start-of-frame is one and the
    // same diagnostic.
    file.seek(SeekFrom::Start(2))
        .map_err(|error| ProbeError::Io(format!("could not seek jpg stream: {error}")))?;
    let mut position: u64 = 2;

    loop {
        if read_byte(file, &mut position)? != 0xFF {
            return Err(ProbeError::Malformed(
                "jpg stream is malformed (lost marker alignment)".to_string(),
            ));
        }
        // Any run of 0xFF fill bytes may precede the marker code, and the scan
        // over them counts against the budget like every other byte.
        let mut code = read_byte(file, &mut position)?;
        while code == 0xFF {
            code = read_byte(file, &mut position)?;
        }
        match code {
            // A stuffed zero belongs to entropy-coded data, never here.
            0x00 => {
                return Err(ProbeError::Malformed(
                    "jpg stream is malformed (stuffed byte outside scan data)".to_string(),
                ))
            }
            // Standalone markers, which carry no length field.
            0xD0..=0xD7 | 0x01 | 0xD8 => continue,
            // Start of scan or end of image: the header region is over.
            0xD9 | 0xDA => return Err(ProbeError::NoStartOfFrame),
            _ => {}
        }
        let high = read_byte(file, &mut position)?;
        let low = read_byte(file, &mut position)?;
        let length = u16::from_be_bytes([high, low]);
        if length < 2 {
            return Err(ProbeError::Malformed(
                "jpg segment declares an invalid length".to_string(),
            ));
        }
        let body = u64::from(length) - 2;

        let is_start_of_frame = matches!(
            code,
            0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF
        );
        if is_start_of_frame {
            if body < 5 {
                return Err(ProbeError::Malformed(
                    "jpg frame header is too short to carry a size".to_string(),
                ));
            }
            // Precision, then height and width, at marker + 5 and marker + 7.
            let mut frame = [0u8; 5];
            for slot in frame.iter_mut() {
                *slot = read_byte(file, &mut position)?;
            }
            return canvas(
                ImageFormat::Jpeg,
                u32::from(u16::from_be_bytes([frame[3], frame[4]])),
                u32::from(u16::from_be_bytes([frame[1], frame[2]])),
                u32::MAX,
            );
        }
        // Never follow an offset outside the budget.
        if position + body > PROBE_BUDGET {
            return Err(ProbeError::NoStartOfFrame);
        }
        file.seek(SeekFrom::Current(body as i64))
            .map_err(|error| ProbeError::Io(format!("could not seek jpg stream: {error}")))?;
        position += body;
    }
}

/// Reads one byte of a JPEG stream. The end of the file and the end of the
/// byte budget are the same answer: the walk found no start-of-frame marker.
fn read_byte(file: &mut File, position: &mut u64) -> Result<u8, ProbeError> {
    if *position >= PROBE_BUDGET {
        return Err(ProbeError::NoStartOfFrame);
    }
    let mut byte = [0u8; 1];
    match file.read(&mut byte) {
        Ok(0) => Err(ProbeError::NoStartOfFrame),
        Ok(_) => {
            *position += 1;
            Ok(byte[0])
        }
        Err(error) => Err(ProbeError::Io(format!(
            "could not read jpg stream: {error}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_file(name: &str, bytes: &[u8]) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("abstract_media_{name}_{suffix}_{unique}"));
        fs::write(&path, bytes).expect("write probe fixture");
        path
    }

    fn probe(name: &str, bytes: &[u8]) -> Result<ImageInfo, ProbeError> {
        let path = temp_file(name, bytes);
        let result = probe_image(&path);
        fs::remove_file(path).ok();
        result
    }

    fn png_bytes(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = PNG_SIGNATURE.to_vec();
        bytes.extend_from_slice(&13u32.to_be_bytes());
        bytes.extend_from_slice(b"IHDR");
        bytes.extend_from_slice(&width.to_be_bytes());
        bytes.extend_from_slice(&height.to_be_bytes());
        bytes.extend_from_slice(&[8, 6, 0, 0, 0]);
        bytes
    }

    fn webp_lossless_bytes(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = b"RIFF".to_vec();
        bytes.extend_from_slice(&17u32.to_le_bytes());
        bytes.extend_from_slice(b"WEBP");
        bytes.extend_from_slice(b"VP8L");
        bytes.extend_from_slice(&5u32.to_le_bytes());
        bytes.push(0x2F);
        bytes.extend_from_slice(&((width - 1) | ((height - 1) << 14)).to_le_bytes());
        bytes
    }

    fn webp_extended_bytes(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = b"RIFF".to_vec();
        bytes.extend_from_slice(&22u32.to_le_bytes());
        bytes.extend_from_slice(b"WEBP");
        bytes.extend_from_slice(b"VP8X");
        bytes.extend_from_slice(&10u32.to_le_bytes());
        bytes.push(0x10);
        bytes.extend_from_slice(&[0, 0, 0]);
        bytes.extend_from_slice(&(width - 1).to_le_bytes()[..3]);
        bytes.extend_from_slice(&(height - 1).to_le_bytes()[..3]);
        bytes
    }

    #[test]
    fn probes_png_dimensions_from_header_only() {
        let info = probe("png", &png_bytes(128, 64)).expect("png probe");
        assert_eq!(info.format, ImageFormat::Png);
        assert_eq!((info.width, info.height), (128, 64));
    }

    #[test]
    fn probes_gif_dimensions() {
        let mut bytes = b"GIF89a".to_vec();
        bytes.extend_from_slice(&320u16.to_le_bytes());
        bytes.extend_from_slice(&200u16.to_le_bytes());
        bytes.extend_from_slice(&[0, 0, 0]);
        let info = probe("gif", &bytes).expect("gif probe");
        assert_eq!(info.format, ImageFormat::Gif);
        assert_eq!((info.width, info.height), (320, 200));
    }

    #[test]
    fn probes_bmp_dimensions_and_reads_a_negative_height_as_top_down() {
        let mut bytes = vec![0u8; 26];
        bytes[0] = b'B';
        bytes[1] = b'M';
        bytes[18..22].copy_from_slice(&512i32.to_le_bytes());
        bytes[22..26].copy_from_slice(&(-256i32).to_le_bytes());
        let info = probe("bmp", &bytes).expect("bmp probe");
        assert_eq!(info.format, ImageFormat::Bmp);
        assert_eq!((info.width, info.height), (512, 256));
    }

    #[test]
    fn rejects_a_bmp_whose_width_is_zero_or_negative() {
        for width in [0i32, -512] {
            let mut bytes = vec![0u8; 26];
            bytes[0] = b'B';
            bytes[1] = b'M';
            bytes[18..22].copy_from_slice(&width.to_le_bytes());
            bytes[22..26].copy_from_slice(&256i32.to_le_bytes());
            let error = probe("bmp_bad_width", &bytes).expect_err("a negative width is not a size");
            assert_eq!(error, ProbeError::MalformedBmpWidth(width));
            assert_eq!(error.note(), Some("malformed BMP width."));
        }
    }

    #[test]
    fn probes_jpeg_dimensions_via_marker_walk() {
        let mut bytes = vec![0xFF, 0xD8];
        // An APP0 segment, to force the walker to skip a body.
        bytes.extend_from_slice(&[0xFF, 0xE0, 0x00, 0x10]);
        bytes.extend_from_slice(&[0u8; 14]);
        // Fill bytes before the next marker.
        bytes.extend_from_slice(&[0xFF, 0xFF]);
        // SOF0 with height 240, width 320.
        bytes.extend_from_slice(&[0xC0, 0x00, 0x11, 0x08, 0x00, 0xF0, 0x01, 0x40]);
        bytes.extend_from_slice(&[0u8; 10]);
        let info = probe("jpg", &bytes).expect("jpg probe");
        assert_eq!(info.format, ImageFormat::Jpeg);
        assert_eq!((info.width, info.height), (320, 240));
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
        let info = probe("webp", &bytes).expect("webp probe");
        assert_eq!(info.format, ImageFormat::Webp);
        assert_eq!((info.width, info.height), (96, 48));
    }

    #[test]
    fn probes_a_lossless_webp_whose_header_is_exactly_25_bytes() {
        // One format's byte requirement never gates another's: a shared
        // 30-byte gate reported this valid file as having no signature.
        let bytes = webp_lossless_bytes(64, 32);
        assert_eq!(bytes.len(), WEBP_LOSSLESS_BYTES);
        let info = probe("webp_lossless", &bytes).expect("lossless webp probe");
        assert_eq!((info.width, info.height), (64, 32));
    }

    #[test]
    fn probes_an_extended_webp_canvas() {
        let info = probe("webp_extended", &webp_extended_bytes(1024, 768)).expect("vp8x probe");
        assert_eq!((info.width, info.height), (1024, 768));
    }

    #[test]
    fn rejects_a_lossless_webp_canvas_wider_than_fourteen_bits() {
        // VP8L adds one to a 14-bit field, so 16384 is expressible and out of
        // range. This is the check the canvas rule exists for.
        let bytes = webp_lossless_bytes(WEBP_14_BIT_MAX + 1, 8);
        let error = probe("webp_wide", &bytes).expect_err("16384 is out of range");
        assert_eq!(error.note(), Some("malformed canvas size."));
    }

    #[test]
    fn truncated_files_with_a_valid_signature_say_so() {
        let cases: [(&str, Vec<u8>, ImageFormat, usize); 4] = [
            ("png", png_bytes(8, 8)[..16].to_vec(), ImageFormat::Png, 24),
            ("gif", b"GIF89a\x01".to_vec(), ImageFormat::Gif, 10),
            ("bmp", vec![b'B', b'M', 0, 0, 0, 0], ImageFormat::Bmp, 26),
            (
                "webp",
                webp_lossless_bytes(4, 4)[..20].to_vec(),
                ImageFormat::Webp,
                25,
            ),
        ];
        for (name, bytes, format, needed) in cases {
            let found = bytes.len();
            let error = probe(name, &bytes).expect_err("a truncated file must not probe");
            assert_eq!(
                error,
                ProbeError::Truncated {
                    format,
                    needed,
                    found
                },
                "{name}"
            );
            assert_eq!(error.note(), Some("file is truncated."), "{name}");
        }
    }

    #[test]
    fn an_unrecognised_file_names_the_bytes_it_found() {
        let error = probe("junk", b"not an image at all").expect_err("junk is not an image");
        assert_eq!(error.note(), None);
        let message = error.to_string();
        assert!(message.contains("6E 6F 74 20"), "{message}");
        assert!(message.contains("not an i"), "{message}");

        // A RIFF container that is not a WebP matches no signature either.
        let mut wave = b"RIFF".to_vec();
        wave.extend_from_slice(&36u32.to_le_bytes());
        wave.extend_from_slice(b"WAVEfmt ");
        let error = probe("wave", &wave).expect_err("a wave file is not an image");
        assert!(matches!(error, ProbeError::UnrecognisedSignature(_)));
    }

    #[test]
    fn an_empty_file_is_unrecognised_and_says_it_is_empty() {
        let error = probe("empty", b"").expect_err("an empty file is not an image");
        assert!(error.to_string().contains("empty"), "{error}");
    }

    #[test]
    fn a_jpeg_without_a_start_of_frame_is_never_reported_as_truncated() {
        // Ending at the start of scan, at the end of the file, and on a
        // segment that runs past the budget all give the same answer.
        let ends_at_scan = vec![0xFF, 0xD8, 0xFF, 0xDA, 0x00, 0x02];
        assert_eq!(
            probe("jpg_sos", &ends_at_scan).expect_err("no SOF"),
            ProbeError::NoStartOfFrame
        );

        let ends_early = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x00, 0x00];
        assert_eq!(
            probe("jpg_short", &ends_early).expect_err("no SOF"),
            ProbeError::NoStartOfFrame
        );

        let error = probe("jpg_short", &ends_early).expect_err("no SOF");
        assert_eq!(error.note(), Some("no start-of-frame marker."));
    }

    #[test]
    fn a_jpeg_walk_stops_at_the_byte_budget() {
        // Enough one-byte-body segments to run past 65536 bytes, followed by a
        // start-of-frame the walk must never reach. Without a byte bound the
        // walk would happily read it.
        let mut bytes = vec![0xFF, 0xD8];
        while bytes.len() < PROBE_BUDGET as usize + 8 {
            bytes.extend_from_slice(&[0xFF, 0xE0, 0x00, 0x40]);
            bytes.extend_from_slice(&[0u8; 62]);
        }
        bytes.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x11, 0x08, 0x00, 0xF0, 0x01, 0x40]);
        assert_eq!(
            probe("jpg_budget", &bytes).expect_err("the budget bounds the walk"),
            ProbeError::NoStartOfFrame
        );
    }

    #[test]
    fn a_zero_dimension_is_a_malformed_canvas() {
        let error = probe("png_zero", &png_bytes(0, 16)).expect_err("zero is not a size");
        assert_eq!(error.note(), Some("malformed canvas size."));
    }

    #[test]
    fn the_probe_reads_content_not_the_file_name() {
        // A PNG probes as a PNG whatever the file is called.
        let info = probe("renamed.jpg", &png_bytes(16, 16)).expect("probe");
        assert_eq!(info.format, ImageFormat::Png);
    }

    #[test]
    fn every_note_is_one_of_the_four_the_specification_defines() {
        let notes = [
            ProbeError::Truncated {
                format: ImageFormat::Png,
                needed: 24,
                found: 3,
            }
            .note(),
            ProbeError::MalformedBmpWidth(-1).note(),
            ProbeError::MalformedCanvasSize {
                format: ImageFormat::Webp,
                width: 0,
                height: 1,
                ceiling: WEBP_14_BIT_MAX,
            }
            .note(),
            ProbeError::NoStartOfFrame.note(),
            ProbeError::Malformed("x".to_string()).note(),
        ];
        assert_eq!(
            notes,
            [
                Some("file is truncated."),
                Some("malformed BMP width."),
                Some("malformed canvas size."),
                Some("no start-of-frame marker."),
                None,
            ]
        );
    }
}
