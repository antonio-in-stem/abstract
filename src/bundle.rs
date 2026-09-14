//! ABX bundles: sealed, tamper-evident containers for compiled Abstract data.
//!
//! `abstract bundle` compiles a project and writes an `.abx` file that ships
//! inside an application (for example a Java plugin jar). The payload is the
//! compiled JSON document encrypted with ChaCha20-Poly1305, so the authored
//! data never appears as plain text inside the artifact and any modification
//! is detected when the bundle is opened.
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
//! # Nonce construction (SPEC Appendix D.1 item 5)
//!
//! Every seal derives its 12-byte nonce in [`derive_nonce`] as the first 12
//! bytes of `SHA-256(SHA-256(payload) ‖ key ‖ clock ‖ counter)`, where:
//!
//! - `clock` is the wall clock as nanoseconds since the Unix epoch, 16 bytes
//!   little-endian, or 16 zero bytes when the clock is before the epoch;
//! - `counter` is `SEAL_COUNTER`, a process-local `AtomicU64` incremented once
//!   per seal, 8 bytes little-endian.
//!
//! `SEAL_COUNTER` starts at 0 and is reset by exactly one event: the start of
//! a new process. It is never reset while the process runs, is shared by every
//! thread, and is not persisted anywhere, so two seals in one process always
//! use different counter values even when the clock does not advance. Across
//! processes the counters do repeat, and uniqueness then rests on the clock
//! and on the key and payload digests being mixed in. A key is therefore safe
//! for as many seals as the pair (clock, counter) is unique; re-sealing the
//! same payload with the same key produces a fresh nonce and fresh ciphertext.

use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::crypto::{open, seal, sha256};

static SEAL_COUNTER: AtomicU64 = AtomicU64::new(0);

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

/// Derives a 32-byte bundle key from CLI key material.
///
/// - `hex:<64 hex digits>` uses the exact key bytes.
/// - anything else is hashed with SHA-256, so passphrases of any length work.
pub fn derive_key(material: &str) -> Result<[u8; 32], String> {
    if let Some(hex_part) = material.strip_prefix("hex:") {
        let digits: Vec<char> = hex_part.chars().collect();
        if digits.len() != 64 || !digits.iter().all(|ch| ch.is_ascii_hexdigit()) {
            return Err(
                "hex keys must contain exactly 64 hexadecimal digits after 'hex:'".to_string(),
            );
        }
        let mut key = [0u8; 32];
        for (index, pair) in hex_part.as_bytes().chunks(2).enumerate() {
            let text = std::str::from_utf8(pair).map_err(|_| "invalid hex key".to_string())?;
            key[index] = u8::from_str_radix(text, 16).map_err(|_| "invalid hex key".to_string())?;
        }
        return Ok(key);
    }
    if material.trim().is_empty() {
        return Err("bundle keys must not be empty".to_string());
    }
    Ok(sha256(material.as_bytes()))
}

/// Encodes `payload` into an ABX bundle. When `key` is provided the payload
/// is sealed with ChaCha20-Poly1305; otherwise it is stored as plain bytes
/// (useful for debugging, not for shipping).
pub fn encode(payload: &[u8], key: Option<&[u8; 32]>) -> Vec<u8> {
    let mut header = [0u8; HEADER_LENGTH];
    header[..4].copy_from_slice(&MAGIC);
    header[5] = FORMAT_JSON;

    match key {
        Some(key) => {
            header[4] = FLAG_ENCRYPTED;
            let nonce = derive_nonce(payload, key);
            header[6..18].copy_from_slice(&nonce);
            let sealed_length = payload.len() + TAG_LENGTH;
            header[18..22].copy_from_slice(&(sealed_length as u32).to_le_bytes());
            let sealed = seal(key, &nonce, &header, payload);
            let mut output = header.to_vec();
            output.extend_from_slice(&sealed);
            output
        }
        None => {
            header[18..22].copy_from_slice(&(payload.len() as u32).to_le_bytes());
            let checksum = plain_checksum(&header, payload);
            header[6..18].copy_from_slice(&checksum);
            let mut output = header.to_vec();
            output.extend_from_slice(payload);
            output
        }
    }
}

/// The keyless checksum a plain container carries in bytes 6..18: the first 12
/// bytes of `SHA-256(header ‖ payload)`, computed with bytes 6..18 of the
/// header set to zero.
///
/// It is an integrity check, not an authentication tag: anyone can recompute
/// it, so it does not prove who wrote the container. What it does prove is
/// that the container was written as a plain container. A sealed container
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

/// Builds a unique nonce for this seal operation. Uniqueness comes from the
/// payload digest mixed with the wall clock and a process-local counter, so
/// re-bundling identical data still produces fresh ciphertext even on
/// platforms with coarse clocks.
fn derive_nonce(payload: &[u8], key: &[u8; 32]) -> [u8; 12] {
    let clock = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let counter = SEAL_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut material = Vec::with_capacity(96);
    material.extend_from_slice(&sha256(payload));
    material.extend_from_slice(key);
    material.extend_from_slice(&clock.to_le_bytes());
    material.extend_from_slice(&counter.to_le_bytes());
    let digest = sha256(&material);
    let mut nonce = [0u8; 12];
    nonce.copy_from_slice(&digest[..12]);
    nonce
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
