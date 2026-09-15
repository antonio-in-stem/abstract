//! ABX bundles: sealed, tamper-evident containers for compiled Abstract data.
//!
//! `abstract bundle` compiles a project and writes an `.abx` file that ships
//! inside an application. Sealed bundles encrypt the compiled JSON document
//! with ChaCha20-Poly1305 and authenticate it before returning plaintext.
//! Confidentiality requires keeping the key secret. Plain bundles carry an
//! unkeyed checksum and provide no authenticity.
//!
//! Layout (little-endian):
//!
//! ```text
//! offset  size  field
//! 0       4     magic "ABX1"
//! 4       1     flags (bit 0: encrypted)
//! 5       1     payload format (1 = compiled JSON document)
//! 6       12    nonce when the bundle is sealed, plain checksum when it is not
//! 18      4     payload length in bytes (u32)
//! 22      n     payload (ciphertext when encrypted, plus a 16-byte tag)
//! ```
//!
//! The 22-byte header is authenticated as associated data, so flags and
//! lengths cannot be altered without breaking the tag.
//!
//! Bytes 6..18 carry the AEAD nonce in a sealed container and, in a plain
//! container, a keyless checksum over the rest of the header and the payload
//! (see [`plain_checksum`]). A plain container is therefore not merely
//! unauthenticated data with a zero field: clearing the encryption flag of a
//! sealed container leaves an AEAD nonce where a checksum belongs, and the
//! container is refused without any key being involved.
//!
//! # Keys, plain containers and the flag rules (SPEC Appendix D.1)
//!
//! `bundle` and `unbundle` are optional tooling (SPEC §9.9): they operate on
//! an already-compiled document and never change compilation semantics. Their
//! flags are validated by [`validate_bundle_flags`], which enforces:
//!
//! - `--key` together with `--plain` is a usage error. A key that was supplied
//!   is never discarded, and no invocation writes an unencrypted container
//!   while a key was given (D.1 item 1).
//! - No flag value may begin with `-` in the separate-token form, so
//!   `bundle data --key --out x.abx` is a usage error and not a container
//!   sealed with the passphrase `--out` (D.1 item 2, SPEC §9.3). A value that
//!   genuinely begins with `-` is written `--key=-secret`.
//!
//! Two further rules live in [`decode`] rather than in the flag parser,
//! because they are properties of the container and not of the command line
//! (D.1 items 3 and 4):
//!
//! - Opening a container whose encryption flag has been cleared fails whether
//!   or not the caller supplied a key. With a key, the cleared flag is refused
//!   as a downgrade; without a key, the plain checksum in bytes 6..18 does not
//!   match, because those bytes hold the AEAD nonce of the sealed container it
//!   was made from. The refusal never depends on caller behaviour.
//! - `unbundle` without a key on a container that declares itself unencrypted
//!   is supported and returns the payload. `--key` against such a container is
//!   refused.
//!
//! # Nonces
//!
//! Every sealed container receives a new 96-bit nonce from the operating
//! system's secure random source. [`try_encode`] reports an error instead of
//! producing a container when that source fails.

use std::fmt;

use crate::crypto::{open, seal, sha256};

/// File signature for bundle version 1.
pub const MAGIC: [u8; 4] = *b"ABX1";
/// Flag bit marking an encrypted payload.
pub const FLAG_ENCRYPTED: u8 = 0b0000_0001;
/// Payload format identifier for the compiled JSON document.
pub const FORMAT_JSON: u8 = 1;

const HEADER_LENGTH: usize = 22;
const TAG_LENGTH: usize = 16;

/// The flags `bundle` and `unbundle` accept, after validation.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BundleFlags {
    /// The key material given with `--key`, exactly as written. `None` when no
    /// key was supplied.
    pub key: Option<String>,
    /// `--plain` was given: write an unencrypted container. Never true at the
    /// same time as `key` is `Some` — that combination is rejected.
    pub plain: bool,
    /// The destination given with `--out`.
    pub out: Option<String>,
}

/// One invocation of `bundle` or `unbundle`, split into its positionals and
/// its validated flags. Flags may appear anywhere among the positionals
/// (SPEC §9.3), so the two are separated rather than ordered.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BundleInvocation {
    pub positionals: Vec<String>,
    pub flags: BundleFlags,
}

/// Why a `bundle` or `unbundle` command line was refused. Every variant is a
/// usage error: the caller reports it on stderr and exits with code 2, the
/// exit code SPEC §9.6 assigns to usage errors.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BundleFlagError {
    /// A token that begins with `-` and names no known flag.
    UnknownFlag(String),
    /// A flag that needs a value did not get one, or was handed a token that
    /// begins with `-` (SPEC §9.3, Appendix D.1 item 2).
    MissingValue(String),
    /// A flag was given twice (SPEC §9.3).
    RepeatedFlag(String),
    /// `--key` and `--plain` together (Appendix D.1 item 1).
    KeyWithPlain,
}

impl fmt::Display for BundleFlagError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownFlag(flag) => write!(f, "Unknown flag '{flag}'."),
            Self::MissingValue(flag) => write!(f, "Flag '{flag}' requires a value."),
            Self::RepeatedFlag(flag) => write!(f, "Flag '{flag}' was given twice."),
            Self::KeyWithPlain => f.write_str(
                "Flags '--key' and '--plain' cannot be combined: \
                 '--plain' would write the container unencrypted and discard the key.",
            ),
        }
    }
}

impl BundleFlagError {
    /// The process exit code this error implies (SPEC §9.6: a usage error).
    pub fn exit_code(&self) -> i32 {
        2
    }
}

/// Validates the argument tokens of `bundle` or `unbundle` — everything after
/// the command word — and splits them into positionals and flags.
///
/// This is the whole of Appendix D.1 items 1 and 2, kept out of the CLI so
/// that it can be tested without a process. `src/cli.rs` calls it as
///
/// ```no_run
/// # use abstract_lang::bundle::{validate_bundle_flags, BundleInvocation};
/// # fn run(rest: &[String]) -> i32 {
/// let invocation: BundleInvocation = match validate_bundle_flags(rest) {
///     Ok(invocation) => invocation,
///     Err(error) => {
///         eprintln!("abstract: error: {error}");
///         return error.exit_code();
///     }
/// };
/// # 0 }
/// ```
///
/// where `rest` is `args[1..]` for `abstract bundle …` / `abstract unbundle …`.
/// On success `invocation.flags.key` and `invocation.flags.plain` are never
/// both set, so the caller cannot write an unencrypted container while holding
/// a key. Recognised flags are `--key`, `--plain` and `--out`; `--key` and
/// `--out` accept both `--flag value` and `--flag=value`.
///
/// A value that genuinely begins with `-` must use the `=` form
/// (`--out=-name`): in the separate-token form a leading `-` is refused, so a
/// forgotten value can never swallow the next flag.
pub fn validate_bundle_flags(args: &[String]) -> Result<BundleInvocation, BundleFlagError> {
    let mut invocation = BundleInvocation::default();
    let mut index = 0;

    while index < args.len() {
        let token = args[index].as_str();
        index += 1;

        let (name, inline) = match token.split_once('=') {
            Some((name, value)) if name.starts_with("--") => (name, Some(value.to_string())),
            _ => (token, None),
        };

        match name {
            "--key" => {
                if invocation.flags.key.is_some() {
                    return Err(BundleFlagError::RepeatedFlag("--key".to_string()));
                }
                invocation.flags.key = Some(take_value("--key", inline, args, &mut index)?);
            }
            "--out" => {
                if invocation.flags.out.is_some() {
                    return Err(BundleFlagError::RepeatedFlag("--out".to_string()));
                }
                invocation.flags.out = Some(take_value("--out", inline, args, &mut index)?);
            }
            "--plain" => {
                if inline.is_some() {
                    return Err(BundleFlagError::UnknownFlag(token.to_string()));
                }
                if invocation.flags.plain {
                    return Err(BundleFlagError::RepeatedFlag("--plain".to_string()));
                }
                invocation.flags.plain = true;
            }
            other if other.starts_with('-') && other != "-" => {
                return Err(BundleFlagError::UnknownFlag(token.to_string()));
            }
            _ => invocation.positionals.push(token.to_string()),
        }
    }

    if invocation.flags.plain && invocation.flags.key.is_some() {
        return Err(BundleFlagError::KeyWithPlain);
    }
    Ok(invocation)
}

/// Reads the value of a flag, either from `--flag=value` or from the next
/// token. In the separate-token form the value must not begin with `-`, so a
/// missing value is reported instead of consuming the following flag.
fn take_value(
    flag: &str,
    inline: Option<String>,
    args: &[String],
    index: &mut usize,
) -> Result<String, BundleFlagError> {
    if let Some(value) = inline {
        if value.is_empty() {
            return Err(BundleFlagError::MissingValue(flag.to_string()));
        }
        return Ok(value);
    }
    let Some(value) = args.get(*index) else {
        return Err(BundleFlagError::MissingValue(flag.to_string()));
    };
    if value.is_empty() || value.starts_with('-') {
        return Err(BundleFlagError::MissingValue(flag.to_string()));
    }
    *index += 1;
    Ok(value.clone())
}

/// Parses the explicit 32-byte key form required when creating a sealed
/// bundle. The accepted form is `hex:` followed by exactly 64 hexadecimal
/// digits.
pub fn parse_key(material: &str) -> Result<[u8; 32], String> {
    if !material.starts_with("hex:") {
        return Err(
            "sealed bundles require 'hex:' followed by exactly 64 hexadecimal digits".to_string(),
        );
    }
    parse_hex_key(material)
}

/// Legacy ABX1 decoder compatibility for key material accepted by earlier
/// releases.
///
/// - `hex:<64 hex digits>` uses the exact key bytes.
/// - anything else is hashed once with SHA-256.
///
/// This single-hash passphrase rule is not a password KDF and must not be used
/// to create new sealed bundles. It remains available so existing ABX1 files
/// can still be opened. New code should use [`parse_key`] with a key produced
/// by [`generate_key`].
pub fn derive_key(material: &str) -> Result<[u8; 32], String> {
    if let Some(hex_part) = material.strip_prefix("hex:") {
        return parse_hex_digits(hex_part);
    }
    if material.trim().is_empty() {
        return Err("bundle keys must not be empty".to_string());
    }
    Ok(sha256(material.as_bytes()))
}

fn parse_hex_key(material: &str) -> Result<[u8; 32], String> {
    let hex_part = material
        .strip_prefix("hex:")
        .expect("parse_key verifies the prefix");
    parse_hex_digits(hex_part)
}

fn parse_hex_digits(hex_part: &str) -> Result<[u8; 32], String> {
    if hex_part.len() != 64 || !hex_part.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("hex keys must contain exactly 64 hexadecimal digits after 'hex:'".to_string());
    }
    let mut key = [0u8; 32];
    for (index, pair) in hex_part.as_bytes().chunks_exact(2).enumerate() {
        let text = std::str::from_utf8(pair).expect("ASCII hex was validated");
        key[index] = u8::from_str_radix(text, 16).expect("ASCII hex was validated");
    }
    Ok(key)
}

/// Generates a new 32-byte bundle key with the operating system's secure
/// random source.
pub fn generate_key() -> Result<[u8; 32], String> {
    let mut key = [0u8; 32];
    fill_secure_random(&mut key)?;
    Ok(key)
}

/// Encodes `payload` into an ABX bundle. When `key` is provided the payload
/// is sealed with ChaCha20-Poly1305; otherwise it is stored as plain bytes.
/// Panics if encoding fails, including unavailable OS randomness. Call
/// [`try_encode`] when errors must be handled by the application.
pub fn encode(payload: &[u8], key: Option<&[u8; 32]>) -> Vec<u8> {
    try_encode(payload, key).unwrap_or_else(|error| panic!("failed to encode ABX bundle: {error}"))
}

/// Fallible form of [`encode`]. A sealed bundle is returned only after the
/// operating system has filled its complete nonce; entropy failure produces
/// an error and no container bytes.
pub fn try_encode(payload: &[u8], key: Option<&[u8; 32]>) -> Result<Vec<u8>, String> {
    encode_with_nonce_source(payload, key, fill_secure_random)
}

fn encode_with_nonce_source<F>(
    payload: &[u8],
    key: Option<&[u8; 32]>,
    fill_nonce: F,
) -> Result<Vec<u8>, String>
where
    F: FnOnce(&mut [u8]) -> Result<(), String>,
{
    let mut header = [0u8; HEADER_LENGTH];
    header[..4].copy_from_slice(&MAGIC);
    header[5] = FORMAT_JSON;

    match key {
        Some(key) => {
            header[4] = FLAG_ENCRYPTED;
            let mut nonce = [0u8; 12];
            fill_nonce(&mut nonce)?;
            header[6..18].copy_from_slice(&nonce);
            let sealed_length = payload
                .len()
                .checked_add(TAG_LENGTH)
                .and_then(|length| u32::try_from(length).ok())
                .ok_or_else(|| "bundle payload is too large".to_string())?;
            header[18..22].copy_from_slice(&sealed_length.to_le_bytes());
            let sealed = seal(key, &nonce, &header, payload);
            let mut output = header.to_vec();
            output.extend_from_slice(&sealed);
            Ok(output)
        }
        None => {
            let payload_length =
                u32::try_from(payload.len()).map_err(|_| "bundle payload is too large")?;
            header[18..22].copy_from_slice(&payload_length.to_le_bytes());
            let checksum = plain_checksum(&header, payload);
            header[6..18].copy_from_slice(&checksum);
            let mut output = header.to_vec();
            output.extend_from_slice(payload);
            Ok(output)
        }
    }
}

fn fill_secure_random(bytes: &mut [u8]) -> Result<(), String> {
    getrandom::fill(bytes)
        .map_err(|_| "operating-system secure randomness is unavailable".to_string())
}

/// The keyless checksum a plain container carries in bytes 6..18: the first 12
/// bytes of `SHA-256(header ‖ payload)`, computed with bytes 6..18 of the
/// header set to zero.
///
/// It is an integrity check, not an authentication tag: anyone can recompute
/// it, so it does not prove who wrote the container or how it originated.
/// A sealed container
/// whose encryption flag is cleared keeps its AEAD nonce in this field and
/// fails the check, which is how Appendix D.1 item 3 is satisfied without a
/// key (see [`decode`]).
pub fn plain_checksum(header: &[u8; HEADER_LENGTH], payload: &[u8]) -> [u8; 12] {
    let mut zeroed = *header;
    zeroed[6..18].fill(0);
    let mut material = Vec::with_capacity(HEADER_LENGTH + payload.len());
    material.extend_from_slice(&zeroed);
    material.extend_from_slice(payload);
    let digest = sha256(&material);
    let mut checksum = [0u8; 12];
    checksum.copy_from_slice(&digest[..12]);
    checksum
}

/// Decodes an ABX bundle back into its payload bytes. Encrypted bundles
/// require the matching key; a wrong key or a modified byte fails cleanly.
pub fn decode(bytes: &[u8], key: Option<&[u8; 32]>) -> Result<Vec<u8>, String> {
    if bytes.len() < HEADER_LENGTH {
        return Err("bundle is too small to contain an ABX header".to_string());
    }
    if bytes[..4] != MAGIC {
        return Err("not an ABX bundle (bad magic)".to_string());
    }
    let flags = bytes[4];
    if bytes[5] != FORMAT_JSON {
        return Err(format!("unsupported bundle payload format {}", bytes[5]));
    }
    let declared = u32::from_le_bytes([bytes[18], bytes[19], bytes[20], bytes[21]]) as usize;
    let body = &bytes[HEADER_LENGTH..];
    if body.len() != declared {
        return Err(format!(
            "bundle payload length mismatch: header declares {declared} bytes, found {}",
            body.len()
        ));
    }

    if flags & FLAG_ENCRYPTED == 0 {
        if key.is_some() {
            // A caller holding a key expects sealed data. Accepting a
            // "plain" bundle here would allow a downgrade attack where an
            // attacker clears the encryption flag to bypass authentication.
            return Err(
                "bundle claims to be unencrypted but a key was provided; refusing the downgrade"
                    .to_string(),
            );
        }
        // The same downgrade is refused without a key: a plain container
        // carries a keyless checksum where a sealed one carries its nonce, so
        // a cleared encryption flag never opens (Appendix D.1 item 3).
        let mut header = [0u8; HEADER_LENGTH];
        header.copy_from_slice(&bytes[..HEADER_LENGTH]);
        if plain_checksum(&header, body)[..] != bytes[6..18] {
            return Err(
                "bundle is not a valid plain container (checksum mismatch); it was modified \
                 or its encryption flag was cleared"
                    .to_string(),
            );
        }
        return Ok(body.to_vec());
    }
    let Some(key) = key else {
        return Err("bundle is encrypted; a key is required to open it".to_string());
    };
    let mut nonce = [0u8; 12];
    nonce.copy_from_slice(&bytes[6..18]);
    open(key, &nonce, &bytes[..HEADER_LENGTH], body)
        .ok_or_else(|| "bundle authentication failed (wrong key or modified data)".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base64(text: &str) -> Vec<u8> {
        let mut output = Vec::with_capacity(text.len() / 4 * 3);
        let mut block = [0u8; 4];
        let mut used = 0;
        for byte in text.bytes().filter(|byte| !byte.is_ascii_whitespace()) {
            block[used] = match byte {
                b'A'..=b'Z' => byte - b'A',
                b'a'..=b'z' => byte - b'a' + 26,
                b'0'..=b'9' => byte - b'0' + 52,
                b'+' => 62,
                b'/' => 63,
                b'=' => 64,
                _ => panic!("invalid base64 fixture"),
            };
            used += 1;
            if used == 4 {
                output.push((block[0] << 2) | (block[1] >> 4));
                if block[2] != 64 {
                    output.push((block[1] << 4) | (block[2] >> 2));
                }
                if block[3] != 64 {
                    output.push((block[2] << 6) | block[3]);
                }
                used = 0;
            }
        }
        assert_eq!(used, 0, "complete base64 fixture");
        output
    }

    #[test]
    fn plain_bundles_roundtrip() {
        let payload = br#"{"data": []}"#;
        let bundle = encode(payload, None);
        assert_eq!(decode(&bundle, None).unwrap(), payload);
    }

    #[test]
    fn encrypted_bundles_roundtrip_and_authenticate() {
        let payload = br#"{"data": [{"id": "atlas"}]}"#;
        let key = derive_key("sunny meadows").unwrap();
        let bundle = encode(payload, Some(&key));

        assert_eq!(decode(&bundle, Some(&key)).unwrap(), payload);
        assert!(decode(&bundle, None).is_err());

        let wrong = derive_key("cloudy meadows").unwrap();
        assert!(decode(&bundle, Some(&wrong)).is_err());

        let mut tampered = bundle.clone();
        let last = tampered.len() - 1;
        tampered[last] ^= 0x40;
        assert!(decode(&tampered, Some(&key)).is_err());

        let mut flag_flipped = bundle;
        flag_flipped[4] = 0;
        assert!(decode(&flag_flipped, Some(&key)).is_err());
    }

    #[test]
    fn hex_keys_are_used_verbatim() {
        let key =
            derive_key("hex:000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f")
                .unwrap();
        assert_eq!(key[0], 0x00);
        assert_eq!(key[31], 0x1f);
        assert!(derive_key("hex:abcd").is_err());
        assert!(derive_key("").is_err());
        assert!(parse_key("passphrase").is_err());
        assert!(
            parse_key("000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f").is_err()
        );
        assert_eq!(
            parse_key("hex:000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f")
                .unwrap(),
            key
        );
    }

    fn args(tokens: &[&str]) -> Vec<String> {
        tokens.iter().map(|token| (*token).to_string()).collect()
    }

    #[test]
    fn flags_are_split_from_positionals_in_any_order() {
        let invocation =
            validate_bundle_flags(&args(&["data", "--key", "secret", "--out", "x.abx"]))
                .expect("valid flags");
        assert_eq!(invocation.positionals, vec!["data".to_string()]);
        assert_eq!(invocation.flags.key.as_deref(), Some("secret"));
        assert_eq!(invocation.flags.out.as_deref(), Some("x.abx"));
        assert!(!invocation.flags.plain);

        let inline =
            validate_bundle_flags(&args(&["--out=x.abx", "data", "--plain"])).expect("valid flags");
        assert_eq!(inline.positionals, vec!["data".to_string()]);
        assert_eq!(inline.flags.out.as_deref(), Some("x.abx"));
        assert!(inline.flags.plain);
        assert!(inline.flags.key.is_none());
    }

    #[test]
    fn a_key_is_never_silently_discarded_by_plain() {
        // Appendix D.1 item 1.
        for tokens in [
            vec!["data", "--key", "secret", "--plain"],
            vec!["data", "--plain", "--key=secret"],
        ] {
            assert_eq!(
                validate_bundle_flags(&args(&tokens)),
                Err(BundleFlagError::KeyWithPlain)
            );
        }
    }

    #[test]
    fn a_flag_value_never_begins_with_a_dash() {
        // Appendix D.1 item 2: this is a usage error, not a container sealed
        // with the passphrase "--out".
        assert_eq!(
            validate_bundle_flags(&args(&["data", "--key", "--out", "x.abx"])),
            Err(BundleFlagError::MissingValue("--key".to_string()))
        );
        assert_eq!(
            validate_bundle_flags(&args(&["data", "--out"])),
            Err(BundleFlagError::MissingValue("--out".to_string()))
        );
        assert_eq!(
            validate_bundle_flags(&args(&["data", "--key="])),
            Err(BundleFlagError::MissingValue("--key".to_string()))
        );
        // A value that genuinely begins with '-' uses the '=' form.
        let invocation =
            validate_bundle_flags(&args(&["data", "--out=-name.abx"])).expect("inline value");
        assert_eq!(invocation.flags.out.as_deref(), Some("-name.abx"));
    }

    #[test]
    fn unknown_and_repeated_flags_are_refused() {
        assert_eq!(
            validate_bundle_flags(&args(&["data", "--keys", "secret"])),
            Err(BundleFlagError::UnknownFlag("--keys".to_string()))
        );
        assert_eq!(
            validate_bundle_flags(&args(&["data", "--plain=yes"])),
            Err(BundleFlagError::UnknownFlag("--plain=yes".to_string()))
        );
        assert_eq!(
            validate_bundle_flags(&args(&["data", "--key", "a", "--key", "b"])),
            Err(BundleFlagError::RepeatedFlag("--key".to_string()))
        );
        assert_eq!(
            validate_bundle_flags(&args(&["--plain", "--plain"])),
            Err(BundleFlagError::RepeatedFlag("--plain".to_string()))
        );
    }

    #[test]
    fn every_flag_error_is_a_usage_error() {
        assert_eq!(BundleFlagError::KeyWithPlain.exit_code(), 2);
        assert!(BundleFlagError::KeyWithPlain
            .to_string()
            .contains("cannot be combined"));
    }

    #[test]
    fn a_cleared_encryption_flag_never_opens_even_without_a_key() {
        // Appendix D.1 item 3: the refusal must not depend on the caller.
        let payload = br#"{"data": []}"#;
        let key = derive_key("sunny meadows").unwrap();
        let mut downgraded = encode(payload, Some(&key));
        downgraded[4] = 0;

        assert!(decode(&downgraded, Some(&key)).is_err());
        let error = decode(&downgraded, None).expect_err("a cleared flag must not open");
        assert!(error.contains("plain container"));
    }

    #[test]
    fn a_modified_plain_container_is_refused() {
        let payload = b"{\"data\": [1]}";
        let mut bundle = encode(payload, None);
        let last = bundle.len() - 1;
        bundle[last] = b' ';
        assert!(decode(&bundle, None).is_err());
    }

    #[test]
    fn fresh_seals_use_fresh_nonces() {
        let payload = b"same payload";
        let key = derive_key("same key").unwrap();
        let first = encode(payload, Some(&key));
        let second = encode(payload, Some(&key));
        assert_ne!(first[6..18], second[6..18]);
    }

    #[test]
    fn opens_bundle_written_by_released_1_4_0_compiler() {
        // Produced by out/delivery/abstract-1.4.0/abstract-windows-x64.exe
        // from examples/features/hello with this synthetic compatibility key.
        let fixture = base64(
            "QUJYMQEBKJ2e3ZCJilic4oZSCAEAALkqxYXamRupdFkVvB5RGezaMPUJgHWG8vZJ2ui7hyBsyFQj1iBRMX+jydvplikqn1biBzkjyBAZTU6U7A86oU6ufmRLeDuyPqZIuHKjr/lCoK1z2suhFfgOzGxFyQ27H+o2oJgkjMeQR3UYCnOuaeTVea20pPXWv7RQtxILUIZbvvHMANAen0ppDCPdKuI+toB3Qq+kNPSR7bEVC0DCR9pgr9emqwmCeJVU+cQftmYiuKqJ5cU6Q9jGKqwUywkUGSWTNvd7q+2P7ONcq+wsXFnLD1WcN6l/Lpb4fENYO+PA6O6oDVE8lJT25eP8+3fqrFETc3e9Bxkuo5hZxyXkfSW/bnNB0Bt5GA==",
        );
        let key = derive_key("abstract-1.4.0 compatibility fixture").unwrap();
        let payload = decode(&fixture, Some(&key)).expect("legacy ABX1 opens");

        assert!(String::from_utf8(payload)
            .unwrap()
            .contains("\"compiler\": \"1.4.0\""));
    }

    #[test]
    fn entropy_failure_returns_no_sealed_container() {
        let error = encode_with_nonce_source(b"payload", Some(&[7u8; 32]), |_| {
            Err("simulated entropy failure".to_string())
        })
        .expect_err("sealing must fail closed");

        assert_eq!(error, "simulated entropy failure");
    }

    #[test]
    fn plain_encoding_does_not_request_entropy() {
        let bundle = encode_with_nonce_source(b"payload", None, |_| {
            panic!("plain containers must not request entropy")
        })
        .expect("plain bundle");

        assert_eq!(decode(&bundle, None).unwrap(), b"payload");
    }
}
