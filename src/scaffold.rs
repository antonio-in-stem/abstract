//! The project scaffold written by `abstract init` (SPEC §9.2).
//!
//! The scaffold is a valid Abstract project that exercises the constructs
//! an author meets first: one `.abt` carrying a `versions` range, a schema
//! using `text`, `int`, `float`, `bool` and `enum`, a group, a list group with
//! `@tag`, one `@optional` field, one default and one `ref`; a `logic` block
//! with one `require` and one `derive`; two instances, the second cloning the
//! first; and one `image` asset under `assets/`.
//!
//! Every source lives under `data/`, so the discovery root of SPEC §2.3 sees
//! all of them however the project is named on the command line. A file beside
//! `data/` would be invisible to `abstract compile <directory>` and would make
//! single-file mode disagree with a whole-project compile.
//!
//! The PNG is generated rather than embedded: the bytes are built here with a
//! stored (uncompressed) DEFLATE block, so the crate ships no binary blob and
//! the image is byte-identical on every platform.

use std::fs;
use std::io;
use std::path::Path;

use abstract_lang::source::normalise_display_path;
use abstract_lang::{Diagnostic, ErrorId};

/// The width and height of the scaffold icon, which `image(png 16x16)`
/// declares.
pub const ICON_SIZE: u32 = 16;

/// Where the icon is written, relative to the scaffold root.
pub const ICON_PATH: &str = "assets/textures/icon.png";

/// One text file of the scaffold.
struct TextFile {
    /// Path relative to the scaffold root, `/`-separated.
    path: &'static str,
    text: &'static str,
}

const README: &str = "\
# An Abstract project

    data/templates/catalog.abt   the schemas and the logic
    data/packs/starter.ab        one Pack instance
    data/items/*.ab              two Item instances, the second a clone
    assets/textures/icon.png     the image the items point at

Every source lives under `data/`, so the whole project is discovered whichever
path is named on the command line.

    abstract compile .           the compiled document, as JSON, on stdout
    abstract compile . YML       the same document as YAML
    abstract compile . --out build/data.json
    abstract lint .              every check, no output
    abstract templates .         the schema names this project declares
";

const CATALOG_ABT: &str = "\
// Template files (.abt) declare schemas and logic. Instance files (.ab)
// declare the data those schemas describe.

versions 1..2

schema Pack {
    id: text(1..40)
    title: text(1..60)
}

schema Item {
    id: text(1..40)
    name: text(1..60)
    pack: ref(Pack)
    rarity: enum(common, rare, epic)
    price: float(0..9999) = 0.0
    tradable: bool
    icon: image(png 16x16)
    note: text(1..160) @optional
    release_wave: int(1..99)

    stats {
        power: int(0..100)
        agility: int(0..100)
    }

    tags[] {
        id: enum(starter, quest, seasonal) @tag
        weight: int(1..10) = 1
    }
}

logic Item {
    // A rule the schema cannot state on its own.
    if .rarity == \"epic\" {
        require .note exists else throw \"An epic item needs a note saying why.\"
    }
    // 'version' is the version being compiled, so the two versions of this
    // project differ and the document carries an overlay.
    derive .release_wave = version
}
";

const STARTER_PACK_AB: &str = "\
// The id comes from the '@id' header tag, or from the file name when the
// header carries none.

Pack :: @id.starter
    title: Starter Kit
";

const STARTER_SWORD_AB: &str = "\
// Header tags assign root fields; the body assigns everything else. The icon
// path resolves under <project root>/assets.

Item :: @id.starter_sword, @rarity.common
    name: Starter Sword
    pack: starter
    tradable: true
    price: 12.5
    icon: ./textures/icon.png
    note: \"The first sword a new player receives.\"
    stats {
        power: 12
        agility: 4
    }
    tags: [#starter, #quest(weight: 3)]
";

const PRACTICE_SWORD_AB: &str = "\
// '&starter_sword.*' copies everything that instance authored; the statements
// below then override what differs. A clone statement goes directly under the
// header, before any body statement.

Item :: @id.practice_sword
&starter_sword.*
    name: Practice Sword
    price: 0.0
    tradable: false
    note: \"A blunted copy kept in the training yard.\"
    tags: [#starter]
";

const FILES: [TextFile; 5] = [
    TextFile {
        path: "README.md",
        text: README,
    },
    TextFile {
        path: "data/templates/catalog.abt",
        text: CATALOG_ABT,
    },
    TextFile {
        path: "data/packs/starter.ab",
        text: STARTER_PACK_AB,
    },
    TextFile {
        path: "data/items/starter_sword.ab",
        text: STARTER_SWORD_AB,
    },
    TextFile {
        path: "data/items/practice_sword.ab",
        text: PRACTICE_SWORD_AB,
    },
];

/// Writes the scaffold into `target`, which must not exist or must be empty
/// (E813). `written` is the path as the command line spelled it, so that no
/// message names an absolute or canonical path (SPEC §9.8).
pub fn create(target: &Path, written: &str) -> Result<(), Diagnostic> {
    if !is_available(target) {
        return Err(Diagnostic::new(
            ErrorId::E813,
            format!("'{written}' is not empty."),
        ));
    }

    for file in &FILES {
        // Source text is written with LF on every platform (SPEC §9.7); a
        // literal in this file that picked up CRLF must not leak into it.
        write_file(
            target,
            file.path,
            file.text.replace("\r\n", "\n").as_bytes(),
            written,
        )?;
    }
    write_file(target, ICON_PATH, &icon_png(), written)
}

/// True when `target` may receive a scaffold: it does not exist, or it is an
/// empty directory (SPEC §9.2). A path that exists and is not an empty
/// directory — including a plain file — is refused.
fn is_available(target: &Path) -> bool {
    let Ok(metadata) = fs::metadata(target) else {
        return true;
    };
    if !metadata.is_dir() {
        return false;
    }
    match fs::read_dir(target) {
        Ok(mut entries) => entries.next().is_none(),
        Err(_) => false,
    }
}

fn write_file(
    target: &Path,
    relative: &str,
    contents: &[u8],
    written: &str,
) -> Result<(), Diagnostic> {
    let path = target.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| write_error(written, relative, &error))?;
    }
    fs::write(&path, contents).map_err(|error| write_error(written, relative, &error))
}

fn write_error(written: &str, relative: &str, error: &io::Error) -> Diagnostic {
    let root = normalise_display_path(written);
    let path = format!("{}/{relative}", root.trim_end_matches('/'));
    Diagnostic::new(
        ErrorId::E810,
        format!(
            "Cannot write '{path}': {}.",
            crate::cli::write_reason(error)
        ),
    )
}

// ---------------------------------------------------------------------------
// The icon
// ---------------------------------------------------------------------------

/// Builds the 16x16 greyscale PNG the scaffold ships as its one asset.
///
/// The image data is stored, not compressed: a zlib stream whose single
/// DEFLATE block has type 00 holds the raw scanlines verbatim, which needs no
/// compressor and is a valid zlib stream that every decoder accepts.
fn icon_png() -> Vec<u8> {
    let size = ICON_SIZE as usize;
    let mut scanlines = Vec::with_capacity(size * (size + 1));
    for y in 0..size {
        scanlines.push(0); // filter type 0: None
        for x in 0..size {
            let border = x == 0 || y == 0 || x + 1 == size || y + 1 == size;
            let diagonal = x == y || x + y + 1 == size;
            scanlines.push(if border {
                0x2A
            } else if diagonal {
                0x60
            } else {
                0xD8
            });
        }
    }

    let mut header = Vec::with_capacity(13);
    header.extend_from_slice(&ICON_SIZE.to_be_bytes());
    header.extend_from_slice(&ICON_SIZE.to_be_bytes());
    // bit depth 8, colour type 0 (greyscale), deflate, adaptive filtering,
    // no interlacing.
    header.extend_from_slice(&[8, 0, 0, 0, 0]);

    let mut png = Vec::new();
    png.extend_from_slice(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
    chunk(b"IHDR", &header, &mut png);
    chunk(b"IDAT", &zlib_stored(&scanlines), &mut png);
    chunk(b"IEND", &[], &mut png);
    png
}

/// Wraps `data` in a zlib stream holding one final stored DEFLATE block. The
/// caller keeps `data` under 65 535 bytes, which one stored block can carry.
fn zlib_stored(data: &[u8]) -> Vec<u8> {
    debug_assert!(data.len() <= u16::MAX as usize);
    let length = data.len() as u16;
    let mut stream = Vec::with_capacity(data.len() + 11);
    // CMF 0x78 (deflate, 32 KiB window) and FLG 0x01: 0x7801 is a multiple of
    // 31, which is the header's check condition.
    stream.extend_from_slice(&[0x78, 0x01]);
    stream.push(0x01); // BFINAL = 1, BTYPE = 00 (stored)
    stream.extend_from_slice(&length.to_le_bytes());
    stream.extend_from_slice(&(!length).to_le_bytes());
    stream.extend_from_slice(data);
    stream.extend_from_slice(&adler32(data).to_be_bytes());
    stream
}

/// Appends one PNG chunk: length, type, body and the CRC of type and body.
fn chunk(kind: &[u8; 4], body: &[u8], out: &mut Vec<u8>) {
    out.extend_from_slice(&(body.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(body);

    let mut covered = Vec::with_capacity(kind.len() + body.len());
    covered.extend_from_slice(kind);
    covered.extend_from_slice(body);
    out.extend_from_slice(&crc32(&covered).to_be_bytes());
}

/// CRC-32/ISO-HDLC, the checksum PNG chunks carry.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            // Branch-free: the mask is all ones when the low bit is set.
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

/// Adler-32, the checksum that ends a zlib stream.
fn adler32(bytes: &[u8]) -> u32 {
    let mut low = 1u32;
    let mut high = 0u32;
    for byte in bytes {
        low = (low + u32::from(*byte)) % 65521;
        high = (high + low) % 65521;
    }
    (high << 16) | low
}

#[cfg(test)]
mod tests {
    use super::*;
    use abstract_lang::media::{probe_image, ImageFormat};

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("abstract_scaffold_{name}_{suffix}"));
        let _ = fs::remove_dir_all(&path);
        path
    }

    #[test]
    fn the_icon_is_a_probeable_16x16_png() {
        let root = temp_dir("icon");
        create(&root, "scaffold").expect("scaffold");

        let info = probe_image(&root.join(ICON_PATH)).expect("the icon probes");
        assert_eq!(info.format, ImageFormat::Png);
        assert_eq!(info.width, ICON_SIZE);
        assert_eq!(info.height, ICON_SIZE);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn every_source_lives_under_the_data_directory() {
        for file in &FILES {
            if file.path.ends_with(".ab") || file.path.ends_with(".abt") {
                assert!(
                    file.path.starts_with("data/"),
                    "{} is outside the discovery root",
                    file.path
                );
            }
        }
    }

    #[test]
    fn the_scaffold_is_written_with_lf_only() {
        let root = temp_dir("endings");
        create(&root, "scaffold").expect("scaffold");

        for file in &FILES {
            let text = fs::read_to_string(root.join(file.path)).expect("read back");
            assert!(!text.contains('\r'), "{} carries a CR", file.path);
        }

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_non_empty_directory_is_refused() {
        let root = temp_dir("occupied");
        fs::create_dir_all(root.join("data")).expect("make dir");

        let error = create(&root, "here").expect_err("a non-empty directory is refused");
        assert_eq!(error.id, ErrorId::E813);
        assert_eq!(error.message, "'here' is not empty.");

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn an_empty_directory_is_accepted() {
        let root = temp_dir("empty");
        fs::create_dir_all(&root).expect("make dir");

        create(&root, "here").expect("an empty directory is a valid target");
        assert!(root.join("data/templates/catalog.abt").is_file());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn the_checksums_match_their_published_test_vectors() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
        assert_eq!(adler32(b""), 1);
    }
}
