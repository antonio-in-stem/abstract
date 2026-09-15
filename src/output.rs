//! Rendering: the compiled value tree and the three output formats
//! (SPEC chapter 8).
//!
//! JSON is the default; YAML is a block rendering with every mapping key
//! quoted; RAW is a review format with unquoted keys and four-space
//! indentation. All three render the same tree, whose key order is fixed by
//! SPEC §8.3 and never by assignment order.
//!
//! The renderer neither reorders nor omits: the tree it is handed already is
//! the document. Key order (§8.3) and the omission of absent optional fields
//! (§8.9) are decided while the tree is built, so every key that reaches this
//! module is emitted, in the order it arrives.

use std::fmt;

use crate::diagnostics::{Diagnostic, Diagnostics, ErrorId};

/// One value of a compiled document.
///
/// `Object` keeps its keys in emission order (SPEC §8.3), so it is a vector of
/// pairs rather than a map. There is no null: an absent optional field is
/// omitted from its object (SPEC §8.9).
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Text(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    List(Vec<Value>),
    Object(Vec<(String, Value)>),
}

impl Value {
    /// An empty object, ready for keys to be pushed in declaration order.
    pub fn object() -> Value {
        Value::Object(Vec::new())
    }

    /// The `{kind}` substitution of SPEC §9.8 for this value.
    pub fn kind(&self) -> &'static str {
        match self {
            Value::Text(_) => "text",
            Value::Int(_) => "int",
            Value::Float(_) => "float",
            Value::Bool(_) => "bool",
            Value::List(_) => "list",
            Value::Object(_) => "object",
        }
    }

    /// Reads a key of an object value.
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Object(entries) => entries
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value),
            _ => None,
        }
    }
}

/// The output formats of SPEC §9.2. `YML` and `YAML` name the same format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Json,
    Yaml,
    Raw,
}

impl Format {
    /// Parses a FORMAT positional, compared case-insensitively (SPEC §9.2).
    pub fn parse(text: &str) -> Option<Format> {
        match text.to_ascii_lowercase().as_str() {
            "json" => Some(Format::Json),
            "yml" | "yaml" => Some(Format::Yaml),
            "raw" => Some(Format::Raw),
            _ => None,
        }
    }

    /// The conventional file extension for this format.
    pub fn extension(self) -> &'static str {
        match self {
            Format::Json => "json",
            Format::Yaml => "yml",
            Format::Raw => "abraw",
        }
    }

    /// The spelling used in diagnostics and in the `--out` informational line.
    pub fn name(self) -> &'static str {
        match self {
            Format::Json => "JSON",
            Format::Yaml => "YAML",
            Format::Raw => "RAW",
        }
    }
}

impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Renders a document tree in `format`. The result ends with exactly one `LF`
/// and carries no BOM (SPEC §8.4–§8.6).
///
/// There is one failure, and a document the compiler produced cannot reach it:
/// E701 for a float that is not finite (§8.7, rejected already at §3.5 and
/// §4.5). The depth limit of §3.7 is **not** re-checked here: SPEC §3.7 makes
/// validation its single site, so by the time a document is rendered its depth
/// is already known to be within the limit.
pub fn render(document: &Value, format: Format) -> Result<String, Diagnostics> {
    let mut renderer = Renderer::new(format);
    match format {
        Format::Json => {
            renderer.json(document, 0)?;
            renderer.out.push('\n');
        }
        Format::Yaml => renderer.yaml_document(document)?,
        Format::Raw => renderer.raw_document(document)?,
    }
    Ok(renderer.out)
}

/// Renders a float as SPEC §8.7 prescribes: the shortest decimal string that
/// round-trips, in positional notation for `1e-6 <= |v| < 1e21` and in
/// scientific notation otherwise. `None` for a value that is not finite,
/// which the caller reports as E701 naming the position it stands at.
///
/// Rust's own `Display` and `LowerExp` for `f64` already produce the shortest
/// digits that round-trip; what is left is choosing between the two spellings
/// and normalising the exponent.
pub fn render_float(value: f64) -> Option<String> {
    if !value.is_finite() {
        return None;
    }
    if value == 0.0 {
        return Some(if value.is_sign_negative() {
            "-0.0".to_string()
        } else {
            "0.0".to_string()
        });
    }
    let magnitude = value.abs();
    if (1e-6..1e21).contains(&magnitude) {
        let plain = format!("{value}");
        return Some(if plain.contains('.') {
            plain
        } else {
            format!("{plain}.0")
        });
    }
    let scientific = format!("{value:e}");
    // `LowerExp` for `f64` always writes an `e`; a value without one is not
    // finite, and that was handled above.
    let (mantissa, exponent) = scientific.split_once('e')?;
    let mantissa = if mantissa.contains('.') {
        mantissa.to_string()
    } else {
        format!("{mantissa}.0")
    };
    Some(match exponent.strip_prefix('-') {
        Some(digits) => format!("{mantissa}e-{digits}"),
        None => format!("{mantissa}e+{exponent}"),
    })
}

/// Escapes a string as SPEC §8.8 prescribes, including the surrounding double
/// quotes. Every Unicode scalar value has a spelling, so this never fails.
pub fn render_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\u{c}' => out.push_str("\\f"),
            '\r' => out.push_str("\\r"),
            // The remaining C0 controls, U+007F and the C1 controls, `\u00XX`
            // in lowercase hex. This is exactly the set SPEC §3.5 excludes
            // from bare text, so a control character never reaches a consumer
            // unescaped whichever spelling produced it. U+007F is escaped
            // although it belongs to neither control range: YAML excludes it
            // from `c-printable`, and a literal one would make an otherwise
            // valid document unreadable (SPEC §8.8, §11.3). `/` is
            // deliberately absent: SPEC §8.8 never escapes it.
            '\u{0}'..='\u{1f}' | '\u{7f}'..='\u{9f}' => {
                out.push_str(&format!("\\u{:04x}", character as u32));
            }
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// The spelling of a scalar, or `None` for a container and for a float that
/// is not finite (E701).
fn scalar_text(value: &Value) -> Option<String> {
    match value {
        Value::Text(text) => Some(render_string(text)),
        Value::Int(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        Value::Float(number) => render_float(*number),
        Value::List(_) | Value::Object(_) => None,
    }
}

/// The one-line spelling of a value that needs no block form: a scalar, `[]`
/// for an empty list, `{}` for an empty object. Callers reach this only for
/// values that are not a non-empty container, so the two bracket pairs are
/// always the empty ones (SPEC §8.4–§8.6). `None` is E701, as above.
fn flat_text(value: &Value) -> Option<String> {
    match value {
        Value::List(_) => Some("[]".to_string()),
        Value::Object(_) => Some("{}".to_string()),
        other => scalar_text(other),
    }
}

/// True for a value that is not a list and not an object, which is what
/// decides whether a RAW array is written inline (SPEC §8.6).
fn is_scalar(value: &Value) -> bool {
    !matches!(value, Value::List(_) | Value::Object(_))
}

/// The rendering state: the buffer being filled, and the dotted path of the
/// value being written, which names the position in an E701 message.
struct Renderer {
    format: Format,
    out: String,
    path: String,
}

impl Renderer {
    fn new(format: Format) -> Self {
        Self {
            format,
            out: String::new(),
            path: String::new(),
        }
    }

    /// Appends `.key` to the current path and returns the length to restore.
    fn push_key(&mut self, key: &str) -> usize {
        let mark = self.path.len();
        if !self.path.is_empty() {
            self.path.push('.');
        }
        self.path.push_str(key);
        mark
    }

    /// Appends `[index]` to the current path, as SPEC §9.8 spells a list
    /// element, and returns the length to restore.
    fn push_index(&mut self, index: usize) -> usize {
        let mark = self.path.len();
        self.path.push('[');
        self.path.push_str(&index.to_string());
        self.path.push(']');
        mark
    }

    fn pop(&mut self, mark: usize) {
        self.path.truncate(mark);
    }

    fn pad(&mut self, spaces: usize) {
        for _ in 0..spaces {
            self.out.push(' ');
        }
    }

    /// E701. `{context}.{field}` is the dotted path of the value, which the
    /// renderer has tracked. SPEC §9.8 roots `{context}` at the instance id;
    /// the renderer walks the envelope and not one instance, so the path it
    /// can name is rooted at the document (`data[0].price`), and a value at
    /// the very root has no containing object to name at all. E701 is
    /// unreachable in a conforming document and §11.3 excepts it from the
    /// required coverage, so no golden case fixes the spelling.
    fn unrepresentable(&self) -> Diagnostics {
        let position = if self.path.is_empty() {
            "the document root"
        } else {
            self.path.as_str()
        };
        Diagnostics::one(Diagnostic::new(
            ErrorId::E701,
            format!(
                "Value at {position} cannot be represented in {}.",
                self.format.name()
            ),
        ))
    }

    /// JSON (SPEC §8.4). `indent` is the level of the value's own line;
    /// members go one level deeper and the closing bracket returns to it.
    fn json(&mut self, value: &Value, indent: usize) -> Result<(), Diagnostics> {
        match value {
            Value::List(items) if !items.is_empty() => {
                self.out.push_str("[\n");
                for (index, item) in items.iter().enumerate() {
                    self.pad((indent + 1) * 2);
                    let mark = self.push_index(index);
                    self.json(item, indent + 1)?;
                    self.pop(mark);
                    self.out.push_str(separator(index, items.len()));
                }
                self.pad(indent * 2);
                self.out.push(']');
            }
            Value::Object(entries) if !entries.is_empty() => {
                self.out.push_str("{\n");
                for (index, (key, child)) in entries.iter().enumerate() {
                    self.pad((indent + 1) * 2);
                    self.out.push_str(&render_string(key));
                    self.out.push_str(": ");
                    let mark = self.push_key(key);
                    self.json(child, indent + 1)?;
                    self.pop(mark);
                    self.out.push_str(separator(index, entries.len()));
                }
                self.pad(indent * 2);
                self.out.push('}');
            }
            other => {
                let text = flat_text(other).ok_or_else(|| self.unrepresentable())?;
                self.out.push_str(&text);
            }
        }
        Ok(())
    }

    /// YAML (SPEC §8.5). The document is the envelope mapping at indent 0,
    /// with no `---` marker.
    fn yaml_document(&mut self, value: &Value) -> Result<(), Diagnostics> {
        match value {
            Value::Object(entries) if !entries.is_empty() => self.yaml_mapping(entries, 0),
            Value::List(items) if !items.is_empty() => self.yaml_sequence(items, 0),
            other => {
                let text = flat_text(other).ok_or_else(|| self.unrepresentable())?;
                self.out.push_str(&text);
                self.out.push('\n');
                Ok(())
            }
        }
    }

    /// A YAML block mapping whose keys start at column `indent`. `depth` is
    /// the document depth of the values inside it, so that the limit of
    /// SPEC §3.7 falls at the same value in all three formats.
    fn yaml_mapping(
        &mut self,
        entries: &[(String, Value)],
        indent: usize,
    ) -> Result<(), Diagnostics> {
        for (key, child) in entries {
            self.pad(indent);
            self.out.push_str(&render_string(key));
            self.out.push(':');
            let mark = self.push_key(key);
            match child {
                Value::Object(children) if !children.is_empty() => {
                    self.out.push('\n');
                    self.yaml_mapping(children, indent + 2)?;
                }
                Value::List(items) if !items.is_empty() => {
                    self.out.push('\n');
                    self.yaml_sequence(items, indent + 2)?;
                }
                other => {
                    let text = flat_text(other).ok_or_else(|| self.unrepresentable())?;
                    self.out.push(' ');
                    self.out.push_str(&text);
                    self.out.push('\n');
                }
            }
            self.pop(mark);
        }
        Ok(())
    }

    /// A YAML block sequence whose `- ` markers start at column `indent`, and
    /// whose items stand at document depth `depth`.
    ///
    /// A container item is written as the block it would be two columns
    /// further in, and its first line's indentation is then overwritten with
    /// `- `, which is exactly the layout SPEC §8.5 describes: the first key of
    /// an object item shares the item's line and the rest align under it.
    fn yaml_sequence(&mut self, items: &[Value], indent: usize) -> Result<(), Diagnostics> {
        for (index, item) in items.iter().enumerate() {
            let mark = self.push_index(index);
            let start = self.out.len();
            match item {
                Value::Object(children) if !children.is_empty() => {
                    self.yaml_mapping(children, indent + 2)?;
                    self.mark_item(start, indent);
                }
                Value::List(nested) if !nested.is_empty() => {
                    self.yaml_sequence(nested, indent + 2)?;
                    self.mark_item(start, indent);
                }
                other => {
                    let text = flat_text(other).ok_or_else(|| self.unrepresentable())?;
                    self.pad(indent);
                    self.out.push_str("- ");
                    self.out.push_str(&text);
                    self.out.push('\n');
                }
            }
            self.pop(mark);
        }
        Ok(())
    }

    /// Replaces the two spaces at `start + indent` with `- `. Both are ASCII,
    /// so the range is on character boundaries; a buffer that does not have
    /// the two spaces is left alone rather than sliced.
    fn mark_item(&mut self, start: usize, indent: usize) {
        let from = start + indent;
        let to = from + 2;
        if self.out.as_bytes().get(from..to) == Some(b"  ".as_slice()) {
            self.out.replace_range(from..to, "- ");
        }
    }

    /// RAW (SPEC §8.6). The envelope object loses its outer braces: its
    /// entries stand at indentation 0, separated by `,` at end of line.
    fn raw_document(&mut self, value: &Value) -> Result<(), Diagnostics> {
        match value {
            Value::Object(entries) if !entries.is_empty() => {
                for (index, (key, child)) in entries.iter().enumerate() {
                    self.out.push_str(key);
                    self.out.push_str(": ");
                    let mark = self.push_key(key);
                    self.raw(child, 0)?;
                    self.pop(mark);
                    self.out.push_str(separator(index, entries.len()));
                }
                Ok(())
            }
            other => {
                self.raw(other, 0)?;
                self.out.push('\n');
                Ok(())
            }
        }
    }

    /// One RAW value. `indent` counts levels of four spaces.
    fn raw(&mut self, value: &Value, indent: usize) -> Result<(), Diagnostics> {
        match value {
            Value::Object(entries) if !entries.is_empty() => {
                self.out.push_str("{\n");
                for (index, (key, child)) in entries.iter().enumerate() {
                    self.pad((indent + 1) * 4);
                    self.out.push_str(key);
                    self.out.push_str(": ");
                    let mark = self.push_key(key);
                    self.raw(child, indent + 1)?;
                    self.pop(mark);
                    self.out.push_str(separator(index, entries.len()));
                }
                self.pad(indent * 4);
                self.out.push('}');
            }
            Value::List(items) if !items.is_empty() && items.iter().all(is_scalar) => {
                self.out.push('[');
                for (index, item) in items.iter().enumerate() {
                    if index > 0 {
                        self.out.push_str(", ");
                    }
                    let mark = self.push_index(index);
                    // Every item is a scalar, so this recursion writes one
                    // flat spelling; it goes through `raw` so that the item
                    // is counted against the depth limit like any other.
                    self.raw(item, indent)?;
                    self.pop(mark);
                }
                self.out.push(']');
            }
            Value::List(items) if !items.is_empty() => {
                self.out.push_str("[\n");
                for (index, item) in items.iter().enumerate() {
                    self.pad((indent + 1) * 4);
                    let mark = self.push_index(index);
                    self.raw(item, indent + 1)?;
                    self.pop(mark);
                    self.out.push_str(separator(index, items.len()));
                }
                self.pad(indent * 4);
                self.out.push(']');
            }
            other => {
                let text = flat_text(other).ok_or_else(|| self.unrepresentable())?;
                self.out.push_str(&text);
            }
        }
        Ok(())
    }
}

/// What follows a member: `,` at end of line, or nothing on the last one.
fn separator(index: usize, count: usize) -> &'static str {
    if index + 1 == count {
        "\n"
    } else {
        ",\n"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CompiledDocument, Overlay, VersionRange};

    /// The empty document of SPEC §8.1.
    const EMPTY_DOCUMENT_JSON: &str = r#"{
  "abstract": {
    "format": 1,
    "compiler": "1.0.0",
    "versions": {
      "min": 1,
      "max": 1
    }
  },
  "data": [],
  "overlays": []
}
"#;

    /// The JSON example of SPEC §8.4.
    const PRODUCT_JSON: &str = r#"{
  "abstract": {
    "format": 1,
    "compiler": "1.0.0",
    "versions": {
      "min": 1,
      "max": 1
    }
  },
  "data": [
    {
      "template": "Product",
      "id": "atlas",
      "price": 19.5,
      "featured": true,
      "tags": [
        "core",
        "public"
      ]
    }
  ],
  "overlays": []
}
"#;

    /// The YAML example of SPEC §8.5.
    const PRODUCT_YAML: &str = r#""abstract":
  "format": 1
  "compiler": "1.0.0"
  "versions":
    "min": 1
    "max": 1
"data":
  - "template": "Product"
    "id": "atlas"
    "price": 19.5
    "featured": true
    "tags":
      - "core"
      - "public"
"overlays": []
"#;

    /// The RAW example of SPEC §8.6.
    const PRODUCT_RAW: &str = r#"abstract: {
    format: 1,
    compiler: "1.0.0",
    versions: {
        min: 1,
        max: 1
    }
},
data: [
    {
        template: "Product",
        id: "atlas",
        price: 19.5,
        featured: true,
        tags: ["core", "public"],
        capabilities: [
            {
                id: "search",
                availability: "stable"
            }
        ]
    }
],
overlays: []
"#;

    /// The compiled JSON of SPEC §C.6.
    const APPENDIX_C_JSON: &str = r#"{
  "abstract": {
    "format": 1,
    "compiler": "1.0.0",
    "versions": {
      "min": 1,
      "max": 2
    }
  },
  "data": [
    {
      "template": "Pack",
      "id": "spring_2026",
      "title": "Spring 2026",
      "tier": "free"
    },
    {
      "template": "Pack",
      "id": "winter_2026",
      "title": "Winter 2026",
      "tier": "plus",
      "slot_count": 3
    },
    {
      "template": "Option",
      "id": "ember",
      "pack": "winter_2026",
      "title": "Ember",
      "icon": "./textures/ember.png",
      "rarity": "epic",
      "glow": false,
      "owner": {
        "team": "Studio A",
        "contact": "support@example.com"
      },
      "tags": [
        "core_ui",
        "promo"
      ],
      "copy": [
        {
          "key": "en_us",
          "value": "Ember"
        },
        {
          "key": "es_es",
          "value": "Brasa"
        },
        {
          "key": "es_mx",
          "value": "Brasa"
        }
      ]
    },
    {
      "template": "Option",
      "id": "frost",
      "pack": "winter_2026",
      "title": "Frost",
      "icon": "./textures/frost.png",
      "rarity": "rare",
      "glow": false,
      "owner": {
        "team": "Studio A",
        "contact": "support@example.com"
      },
      "tags": [
        "core_ui",
        "core_game"
      ],
      "copy": [
        {
          "key": "en_us",
          "value": "Frost"
        },
        {
          "key": "es_es",
          "value": "Escarcha"
        },
        {
          "key": "es_mx",
          "value": "Escarcha"
        }
      ]
    }
  ],
  "overlays": [
    {
      "versions": {
        "min": 1,
        "max": 1
      },
      "data": [
        {
          "template": "Option",
          "id": "ember",
          "pack": "winter_2026",
          "title": "Ember",
          "icon": "./textures/ember.png",
          "rarity": "epic",
          "tint": 200,
          "owner": {
            "team": "Studio A",
            "contact": "support@example.com"
          },
          "tags": [
            "core_ui",
            "promo"
          ],
          "copy": [
            {
              "key": "en_us",
              "value": "Ember"
            },
            {
              "key": "es_es",
              "value": "Brasa"
            },
            {
              "key": "es_mx",
              "value": "Brasa"
            }
          ]
        },
        {
          "template": "Option",
          "id": "frost",
          "pack": "winter_2026",
          "title": "Frost",
          "icon": "./textures/frost.png",
          "rarity": "rare",
          "tint": 200,
          "owner": {
            "team": "Studio A",
            "contact": "support@example.com"
          },
          "tags": [
            "core_ui",
            "core_game"
          ],
          "copy": [
            {
              "key": "en_us",
              "value": "Frost"
            },
            {
              "key": "es_es",
              "value": "Escarcha"
            },
            {
              "key": "es_mx",
              "value": "Escarcha"
            }
          ]
        }
      ],
      "removed": [
        "spring_2026"
      ]
    }
  ]
}
"#;

    /// The compiled YAML of SPEC §C.7.
    const APPENDIX_C_YAML: &str = r#""abstract":
  "format": 1
  "compiler": "1.0.0"
  "versions":
    "min": 1
    "max": 2
"data":
  - "template": "Pack"
    "id": "spring_2026"
    "title": "Spring 2026"
    "tier": "free"
  - "template": "Pack"
    "id": "winter_2026"
    "title": "Winter 2026"
    "tier": "plus"
    "slot_count": 3
  - "template": "Option"
    "id": "ember"
    "pack": "winter_2026"
    "title": "Ember"
    "icon": "./textures/ember.png"
    "rarity": "epic"
    "glow": false
    "owner":
      "team": "Studio A"
      "contact": "support@example.com"
    "tags":
      - "core_ui"
      - "promo"
    "copy":
      - "key": "en_us"
        "value": "Ember"
      - "key": "es_es"
        "value": "Brasa"
      - "key": "es_mx"
        "value": "Brasa"
  - "template": "Option"
    "id": "frost"
    "pack": "winter_2026"
    "title": "Frost"
    "icon": "./textures/frost.png"
    "rarity": "rare"
    "glow": false
    "owner":
      "team": "Studio A"
      "contact": "support@example.com"
    "tags":
      - "core_ui"
      - "core_game"
    "copy":
      - "key": "en_us"
        "value": "Frost"
      - "key": "es_es"
        "value": "Escarcha"
      - "key": "es_mx"
        "value": "Escarcha"
"overlays":
  - "versions":
      "min": 1
      "max": 1
    "data":
      - "template": "Option"
        "id": "ember"
        "pack": "winter_2026"
        "title": "Ember"
        "icon": "./textures/ember.png"
        "rarity": "epic"
        "tint": 200
        "owner":
          "team": "Studio A"
          "contact": "support@example.com"
        "tags":
          - "core_ui"
          - "promo"
        "copy":
          - "key": "en_us"
            "value": "Ember"
          - "key": "es_es"
            "value": "Brasa"
          - "key": "es_mx"
            "value": "Brasa"
      - "template": "Option"
        "id": "frost"
        "pack": "winter_2026"
        "title": "Frost"
        "icon": "./textures/frost.png"
        "rarity": "rare"
        "tint": 200
        "owner":
          "team": "Studio A"
          "contact": "support@example.com"
        "tags":
          - "core_ui"
          - "core_game"
        "copy":
          - "key": "en_us"
            "value": "Frost"
          - "key": "es_es"
            "value": "Escarcha"
          - "key": "es_mx"
            "value": "Escarcha"
    "removed":
      - "spring_2026"
"#;

    fn text(value: &str) -> Value {
        Value::Text(value.to_string())
    }

    fn object(entries: &[(&str, Value)]) -> Value {
        Value::Object(
            entries
                .iter()
                .map(|(key, value)| ((*key).to_string(), value.clone()))
                .collect(),
        )
    }

    fn list(items: &[Value]) -> Value {
        Value::List(items.to_vec())
    }

    fn text_list(items: &[&str]) -> Value {
        Value::List(items.iter().map(|item| text(item)).collect())
    }

    fn render_ok(value: &Value, format: Format) -> String {
        render(value, format).expect("the document renders")
    }

    /// The `abstract.compiler` normalisation of SPEC §11.2, so that a
    /// rendering can be compared with a specification example whose compiler
    /// is `1.0.0`.
    fn normalise_compiler(rendered: &str) -> String {
        rendered.replace(&format!("\"{}\"", crate::COMPILER_VERSION), "\"1.0.0\"")
    }

    fn rendered(document: &CompiledDocument, format: Format) -> String {
        normalise_compiler(&document.render(format).expect("the document renders"))
    }

    /// The `Product` instance of the §8.4 and §8.5 examples.
    fn product_document() -> CompiledDocument {
        let product = object(&[
            ("template", text("Product")),
            ("id", text("atlas")),
            ("price", Value::Float(19.5)),
            ("featured", Value::Bool(true)),
            ("tags", text_list(&["core", "public"])),
        ]);
        CompiledDocument::new(VersionRange::DEFAULT, vec![product], Vec::new())
    }

    /// The §8.6 example, which adds a list of group values to the same
    /// instance so that RAW has both an inline and a multi-line array.
    fn product_with_capabilities_document() -> CompiledDocument {
        let capabilities = list(&[object(&[
            ("id", text("search")),
            ("availability", text("stable")),
        ])]);
        let product = object(&[
            ("template", text("Product")),
            ("id", text("atlas")),
            ("price", Value::Float(19.5)),
            ("featured", Value::Bool(true)),
            ("tags", text_list(&["core", "public"])),
            ("capabilities", capabilities),
        ]);
        CompiledDocument::new(VersionRange::DEFAULT, vec![product], Vec::new())
    }

    fn owner() -> Value {
        object(&[
            ("team", text("Studio A")),
            ("contact", text("support@example.com")),
        ])
    }

    fn copy_rows(rows: &[(&str, &str)]) -> Value {
        Value::List(
            rows.iter()
                .map(|(key, value)| object(&[("key", text(key)), ("value", text(value))]))
                .collect(),
        )
    }

    /// The compiled document of Appendix C, built by hand: the four base
    /// instances of §C.6 and the single `1..1` overlay that carries `tint`,
    /// drops `glow` and removes `spring_2026`.
    fn appendix_c_document() -> CompiledDocument {
        let spring = object(&[
            ("template", text("Pack")),
            ("id", text("spring_2026")),
            ("title", text("Spring 2026")),
            ("tier", text("free")),
        ]);
        let winter = object(&[
            ("template", text("Pack")),
            ("id", text("winter_2026")),
            ("title", text("Winter 2026")),
            ("tier", text("plus")),
            ("slot_count", Value::Int(3)),
        ]);
        let ember = object(&[
            ("template", text("Option")),
            ("id", text("ember")),
            ("pack", text("winter_2026")),
            ("title", text("Ember")),
            ("icon", text("./textures/ember.png")),
            ("rarity", text("epic")),
            ("glow", Value::Bool(false)),
            ("owner", owner()),
            ("tags", text_list(&["core_ui", "promo"])),
            (
                "copy",
                copy_rows(&[("en_us", "Ember"), ("es_es", "Brasa"), ("es_mx", "Brasa")]),
            ),
        ]);
        let frost = object(&[
            ("template", text("Option")),
            ("id", text("frost")),
            ("pack", text("winter_2026")),
            ("title", text("Frost")),
            ("icon", text("./textures/frost.png")),
            ("rarity", text("rare")),
            ("glow", Value::Bool(false)),
            ("owner", owner()),
            ("tags", text_list(&["core_ui", "core_game"])),
            (
                "copy",
                copy_rows(&[
                    ("en_us", "Frost"),
                    ("es_es", "Escarcha"),
                    ("es_mx", "Escarcha"),
                ]),
            ),
        ]);
        let ember_v1 = object(&[
            ("template", text("Option")),
            ("id", text("ember")),
            ("pack", text("winter_2026")),
            ("title", text("Ember")),
            ("icon", text("./textures/ember.png")),
            ("rarity", text("epic")),
            ("tint", Value::Int(200)),
            ("owner", owner()),
            ("tags", text_list(&["core_ui", "promo"])),
            (
                "copy",
                copy_rows(&[("en_us", "Ember"), ("es_es", "Brasa"), ("es_mx", "Brasa")]),
            ),
        ]);
        let frost_v1 = object(&[
            ("template", text("Option")),
            ("id", text("frost")),
            ("pack", text("winter_2026")),
            ("title", text("Frost")),
            ("icon", text("./textures/frost.png")),
            ("rarity", text("rare")),
            ("tint", Value::Int(200)),
            ("owner", owner()),
            ("tags", text_list(&["core_ui", "core_game"])),
            (
                "copy",
                copy_rows(&[
                    ("en_us", "Frost"),
                    ("es_es", "Escarcha"),
                    ("es_mx", "Escarcha"),
                ]),
            ),
        ]);
        let overlay = Overlay {
            versions: VersionRange::new(1, 1).expect("1..1 is a range"),
            data: vec![ember_v1, frost_v1],
            removed: vec!["spring_2026".to_string()],
        };
        CompiledDocument::new(
            VersionRange::new(1, 2).expect("1..2 is a range"),
            vec![spring, winter, ember, frost],
            vec![overlay],
        )
    }

    /// A chain of objects `levels` values deep, with a scalar at the bottom.
    fn nest(levels: usize) -> Value {
        let mut value = Value::Int(1);
        for _ in 1..levels {
            value = Value::Object(vec![("child".to_string(), value)]);
        }
        value
    }

    #[test]
    fn format_keywords_are_case_insensitive_and_yml_aliases_yaml() {
        assert_eq!(Format::parse("JSON"), Some(Format::Json));
        assert_eq!(Format::parse("yml"), Some(Format::Yaml));
        assert_eq!(Format::parse("YAML"), Some(Format::Yaml));
        assert_eq!(Format::parse("Raw"), Some(Format::Raw));
        assert_eq!(Format::parse("XML"), None);
    }

    #[test]
    fn each_format_names_itself_and_its_extension() {
        assert_eq!(Format::Json.extension(), "json");
        assert_eq!(Format::Yaml.extension(), "yml");
        assert_eq!(Format::Raw.extension(), "abraw");
        assert_eq!(Format::Json.to_string(), "JSON");
        assert_eq!(Format::Yaml.to_string(), "YAML");
        assert_eq!(Format::Raw.to_string(), "RAW");
    }

    #[test]
    fn object_values_keep_insertion_order() {
        let value = Value::Object(vec![
            ("template".to_string(), Value::Text("Item".to_string())),
            ("id".to_string(), Value::Text("torch".to_string())),
        ]);
        assert_eq!(value.get("id"), Some(&Value::Text("torch".to_string())));
        assert_eq!(value.get("missing"), None);
        assert_eq!(value.kind(), "object");
        assert_eq!(Value::object(), Value::Object(Vec::new()));
        assert_eq!(Value::Text(String::new()).get("id"), None);
    }

    #[test]
    fn every_value_reports_the_kind_substitution_of_the_catalogue() {
        assert_eq!(text("x").kind(), "text");
        assert_eq!(Value::Int(1).kind(), "int");
        assert_eq!(Value::Float(1.0).kind(), "float");
        assert_eq!(Value::Bool(true).kind(), "bool");
        assert_eq!(Value::List(Vec::new()).kind(), "list");
    }

    // §8.1 — the document envelope.

    #[test]
    fn the_empty_document_renders_as_the_envelope_example_of_8_1() {
        let document = CompiledDocument::new(VersionRange::DEFAULT, Vec::new(), Vec::new());
        assert_eq!(rendered(&document, Format::Json), EMPTY_DOCUMENT_JSON);
    }

    #[test]
    fn the_envelope_keys_keep_their_specified_order_in_every_format() {
        let document = CompiledDocument::new(VersionRange::DEFAULT, Vec::new(), Vec::new());

        let json = rendered(&document, Format::Json);
        let abstract_at = json.find("\"abstract\"").expect("abstract is present");
        let data_at = json.find("\"data\"").expect("data is present");
        let overlays_at = json.find("\"overlays\"").expect("overlays is present");
        assert!(abstract_at < data_at && data_at < overlays_at);

        let format_at = json.find("\"format\"").expect("format is present");
        let compiler_at = json.find("\"compiler\"").expect("compiler is present");
        let versions_at = json.find("\"versions\"").expect("versions is present");
        assert!(format_at < compiler_at && compiler_at < versions_at);
        assert!(json.contains("\"min\": 1,\n      \"max\": 1"));

        let raw = rendered(&document, Format::Raw);
        assert!(raw.starts_with("abstract: {\n"));
        assert!(raw.contains("\ndata: []"));
        assert!(raw.ends_with("\noverlays: []\n"));
    }

    #[test]
    fn the_document_format_number_is_an_unquoted_one() {
        let document = CompiledDocument::new(VersionRange::DEFAULT, Vec::new(), Vec::new());
        assert!(rendered(&document, Format::Json).contains("\"format\": 1,"));
        assert!(rendered(&document, Format::Yaml).contains("\"format\": 1\n"));
        assert!(rendered(&document, Format::Raw).contains("format: 1,"));
    }

    // §8.2, §8.3 — the instance object and key ordering.

    #[test]
    fn an_instance_object_leads_with_template_and_id() {
        let json = rendered(&product_document(), Format::Json);
        let template_at = json.find("\"template\"").expect("template is present");
        let id_at = json.find("\"id\"").expect("id is present");
        let price_at = json.find("\"price\"").expect("price is present");
        assert!(template_at < id_at && id_at < price_at);
    }

    #[test]
    fn keys_are_emitted_in_the_order_they_arrive_and_are_never_sorted() {
        let value = object(&[
            ("template", text("Product")),
            ("id", text("atlas")),
            ("zeta", Value::Int(1)),
            ("alpha", Value::Int(2)),
        ]);
        assert_eq!(
            render_ok(&value, Format::Json),
            "{\n  \"template\": \"Product\",\n  \"id\": \"atlas\",\n  \"zeta\": 1,\n  \"alpha\": 2\n}\n"
        );
        assert_eq!(
            render_ok(&value, Format::Yaml),
            "\"template\": \"Product\"\n\"id\": \"atlas\"\n\"zeta\": 1\n\"alpha\": 2\n"
        );
        assert_eq!(
            render_ok(&value, Format::Raw),
            "template: \"Product\",\nid: \"atlas\",\nzeta: 1,\nalpha: 2\n"
        );
    }

    #[test]
    fn a_group_value_keeps_the_declaration_order_of_its_own_fields() {
        // §8.3 step 3: a nested object orders its keys by the declaration
        // order of the group, not by the order of the parent.
        let value = object(&[(
            "owner",
            object(&[("team", text("Studio A")), ("contact", text("a@b.c"))]),
        )]);
        assert_eq!(
            render_ok(&value, Format::Yaml),
            "\"owner\":\n  \"team\": \"Studio A\"\n  \"contact\": \"a@b.c\"\n"
        );
    }

    // §8.4 — JSON.

    #[test]
    fn json_matches_the_example_of_8_4() {
        assert_eq!(rendered(&product_document(), Format::Json), PRODUCT_JSON);
    }

    #[test]
    fn json_writes_empty_containers_on_one_line() {
        let value = object(&[
            ("group", Value::object()),
            ("items", Value::List(Vec::new())),
        ]);
        assert_eq!(
            render_ok(&value, Format::Json),
            "{\n  \"group\": {},\n  \"items\": []\n}\n"
        );
    }

    #[test]
    fn json_indents_two_spaces_per_level_and_closes_at_the_parent() {
        let value = object(&[(
            "a",
            list(&[object(&[("b", list(&[Value::Int(1), Value::Int(2)]))])]),
        )]);
        assert_eq!(
            render_ok(&value, Format::Json),
            concat!(
                "{\n",
                "  \"a\": [\n",
                "    {\n",
                "      \"b\": [\n",
                "        1,\n",
                "        2\n",
                "      ]\n",
                "    }\n",
                "  ]\n",
                "}\n"
            )
        );
    }

    #[test]
    fn json_escapes_keys_as_strings() {
        let value = object(&[("a\"b", Value::Int(1))]);
        assert_eq!(render_ok(&value, Format::Json), "{\n  \"a\\\"b\": 1\n}\n");
    }

    // §8.5 — YAML.

    #[test]
    fn yaml_matches_the_example_of_8_5() {
        assert_eq!(rendered(&product_document(), Format::Yaml), PRODUCT_YAML);
    }

    #[test]
    fn yaml_quotes_every_mapping_key_however_it_is_spelled() {
        let value = object(&[
            ("1", Value::Int(1)),
            ("yes", Value::Bool(true)),
            ("no", Value::Bool(false)),
            ("on", text("dusk")),
            ("null", Value::Int(0)),
        ]);
        assert_eq!(
            render_ok(&value, Format::Yaml),
            "\"1\": 1\n\"yes\": true\n\"no\": false\n\"on\": \"dusk\"\n\"null\": 0\n"
        );
    }

    #[test]
    fn yaml_writes_empty_containers_on_the_keys_line_and_after_a_dash() {
        let value = object(&[
            ("group", Value::object()),
            ("items", Value::List(Vec::new())),
            (
                "rows",
                list(&[Value::object(), Value::List(Vec::new()), Value::Int(1)]),
            ),
        ]);
        assert_eq!(
            render_ok(&value, Format::Yaml),
            concat!(
                "\"group\": {}\n",
                "\"items\": []\n",
                "\"rows\":\n",
                "  - {}\n",
                "  - []\n",
                "  - 1\n"
            )
        );
    }

    #[test]
    fn a_yaml_object_item_puts_its_first_key_on_the_dash_line() {
        let value = object(&[(
            "rows",
            list(&[
                object(&[("key", text("en_us")), ("value", text("Ember"))]),
                object(&[("key", text("es_es")), ("value", text("Brasa"))]),
            ]),
        )]);
        assert_eq!(
            render_ok(&value, Format::Yaml),
            concat!(
                "\"rows\":\n",
                "  - \"key\": \"en_us\"\n",
                "    \"value\": \"Ember\"\n",
                "  - \"key\": \"es_es\"\n",
                "    \"value\": \"Brasa\"\n"
            )
        );
    }

    #[test]
    fn a_yaml_sequence_item_that_is_itself_a_sequence_shares_the_dash_line() {
        // A list of lists cannot be authored in Abstract (§4.4), so this is
        // the block form the §8.5 rules imply rather than one the
        // specification prints; it is here so the shape is pinned.
        let value = object(&[("rows", list(&[text_list(&["a", "b"])]))]);
        assert_eq!(
            render_ok(&value, Format::Yaml),
            "\"rows\":\n  - - \"a\"\n    - \"b\"\n"
        );
    }

    #[test]
    fn yaml_carries_no_document_marker() {
        let yaml = rendered(&appendix_c_document(), Format::Yaml);
        assert!(!yaml.contains("---"));
        assert!(yaml.starts_with("\"abstract\":\n"));
    }

    // §8.6 — RAW.

    #[test]
    fn raw_matches_the_example_of_8_6() {
        assert_eq!(
            rendered(&product_with_capabilities_document(), Format::Raw),
            PRODUCT_RAW
        );
    }

    #[test]
    fn raw_inlines_scalar_arrays_and_breaks_arrays_that_hold_containers() {
        let value = object(&[
            ("scalars", text_list(&["a", "b"])),
            (
                "mixed",
                list(&[Value::Int(1), object(&[("k", Value::Int(2))])]),
            ),
            ("nested", list(&[Value::List(Vec::new())])),
            ("empty", Value::List(Vec::new())),
        ]);
        assert_eq!(
            render_ok(&value, Format::Raw),
            concat!(
                "scalars: [\"a\", \"b\"],\n",
                "mixed: [\n",
                "    1,\n",
                "    {\n",
                "        k: 2\n",
                "    }\n",
                "],\n",
                "nested: [\n",
                "    []\n",
                "],\n",
                "empty: []\n"
            )
        );
    }

    #[test]
    fn a_raw_object_is_always_multi_line_and_an_empty_one_is_braces() {
        let value = object(&[
            ("one", object(&[("k", Value::Int(1))])),
            ("none", Value::object()),
        ]);
        assert_eq!(
            render_ok(&value, Format::Raw),
            "one: {\n    k: 1\n},\nnone: {}\n"
        );
    }

    #[test]
    fn raw_writes_the_envelope_without_braces_and_leaves_keys_bare() {
        let raw = rendered(&appendix_c_document(), Format::Raw);
        assert!(raw.starts_with("abstract: {\n    format: 1,\n    compiler: \"1.0.0\",\n"));
        assert!(!raw.contains("\"abstract\""));
        assert!(raw.contains("\n},\ndata: [\n"));
        assert!(raw.contains("        tags: [\"core_ui\", \"promo\"],\n"));
        assert!(raw.contains("        removed: [\"spring_2026\"]\n"));
        assert!(raw.ends_with("\n]\n"));
    }

    // §8.7 — number formatting.

    #[test]
    fn integers_are_base_ten_with_no_leading_zeros_and_no_plus() {
        let value = object(&[
            ("zero", Value::Int(0)),
            ("negative", Value::Int(-7)),
            ("low", Value::Int(i64::MIN)),
            ("high", Value::Int(i64::MAX)),
        ]);
        assert_eq!(
            render_ok(&value, Format::Yaml),
            concat!(
                "\"zero\": 0\n",
                "\"negative\": -7\n",
                "\"low\": -9223372036854775808\n",
                "\"high\": 9223372036854775807\n"
            )
        );
    }

    #[test]
    fn floats_take_the_spelling_of_8_7_and_round_trip() {
        let cases: [(f64, &str); 16] = [
            (0.0, "0.0"),
            (-0.0, "-0.0"),
            (3.0, "3.0"),
            (-3.0, "-3.0"),
            (19.5, "19.5"),
            (0.1, "0.1"),
            (1e-6, "0.000001"),
            (1e-5, "0.00001"),
            (1e20, "100000000000000000000.0"),
            (1e21, "1.0e+21"),
            (1e-7, "1.0e-7"),
            (1.5e-7, "1.5e-7"),
            (-2.5e-9, "-2.5e-9"),
            (f64::MAX, "1.7976931348623157e+308"),
            (5e-324, "5.0e-324"),
            (1.0 / 3.0, "0.3333333333333333"),
        ];
        for (value, expected) in cases {
            let text = render_float(value).expect("a finite float has a spelling");
            assert_eq!(text, expected, "rendering {value:?}");
            let parsed: f64 = text.parse().expect("the rendering parses back");
            assert_eq!(
                parsed.to_bits(),
                value.to_bits(),
                "{text} does not round-trip"
            );
        }
    }

    #[test]
    fn a_float_is_always_distinguishable_from_an_integer() {
        let value = object(&[("count", Value::Int(3)), ("ratio", Value::Float(3.0))]);
        assert_eq!(
            render_ok(&value, Format::Json),
            "{\n  \"count\": 3,\n  \"ratio\": 3.0\n}\n"
        );
    }

    #[test]
    fn a_scientific_exponent_carries_an_explicit_sign_and_no_leading_zeros() {
        for (value, expected) in [(1e21, "e+21"), (1e-7, "e-7"), (1e100, "e+100")] {
            let text = render_float(value).expect("a finite float has a spelling");
            assert!(
                text.ends_with(expected),
                "{text} should end with {expected}"
            );
            assert!(text.contains('.'), "{text} needs a fractional digit");
        }
    }

    #[test]
    fn a_value_that_is_not_finite_has_no_spelling() {
        assert_eq!(render_float(f64::NAN), None);
        assert_eq!(render_float(f64::INFINITY), None);
        assert_eq!(render_float(f64::NEG_INFINITY), None);
    }

    // §8.8 — string escaping.

    #[test]
    fn strings_use_exactly_the_escapes_of_8_8() {
        assert_eq!(render_string("plain"), "\"plain\"");
        assert_eq!(render_string("a\"b"), "\"a\\\"b\"");
        assert_eq!(render_string("a\\b"), "\"a\\\\b\"");
        assert_eq!(render_string("a\u{8}b"), "\"a\\bb\"");
        assert_eq!(render_string("a\tb"), "\"a\\tb\"");
        assert_eq!(render_string("a\nb"), "\"a\\nb\"");
        assert_eq!(render_string("a\u{c}b"), "\"a\\fb\"");
        assert_eq!(render_string("a\rb"), "\"a\\rb\"");
    }

    #[test]
    fn other_control_characters_are_four_digit_lowercase_hex() {
        assert_eq!(render_string("\u{0}"), "\"\\u0000\"");
        assert_eq!(render_string("\u{1}"), "\"\\u0001\"");
        assert_eq!(render_string("\u{b}"), "\"\\u000b\"");
        assert_eq!(render_string("\u{1f}"), "\"\\u001f\"");
        assert_eq!(render_string("\u{80}"), "\"\\u0080\"");
        assert_eq!(render_string("\u{9f}"), "\"\\u009f\"");
    }

    #[test]
    fn the_escaped_set_is_every_control_scalar_including_u_007f() {
        // SPEC §8.8 escapes exactly the set §3.5 excludes from bare text: the
        // C0 range, U+007F and the C1 range. U+007F is in it although it
        // belongs to neither control range, because YAML leaves it out of
        // `c-printable` and a literal one would make the `.yml` rendering
        // unparseable, which §11.3 requires the golden runner to catch.
        assert_eq!(render_string("\u{7f}"), "\"\\u007f\"");
        assert_eq!(render_string("\u{0}"), "\"\\u0000\"");
        assert_eq!(render_string("\u{1f}"), "\"\\u001f\"");
        assert_eq!(render_string("\u{85}"), "\"\\u0085\"");
        assert_eq!(render_string("\u{9f}"), "\"\\u009f\"");
        // U+0080 through U+009F are escaped; the next scalar value is not.
        assert_eq!(render_string("\u{a0}"), "\"\u{a0}\"");
    }

    #[test]
    fn non_ascii_text_and_slashes_are_emitted_literally() {
        assert_eq!(render_string("café"), "\"café\"");
        assert_eq!(render_string("日本語"), "\"日本語\"");
        assert_eq!(render_string("\u{1f600}"), "\"\u{1f600}\"");
        assert_eq!(render_string("a/b"), "\"a/b\"");
        assert_eq!(
            render_string("//cdn.example.com/x.png"),
            "\"//cdn.example.com/x.png\""
        );
        // U+00A0 is outside the C1 range the table names.
        assert_eq!(render_string("\u{a0}"), "\"\u{a0}\"");
    }

    #[test]
    fn every_format_uses_the_same_string_escaping() {
        let value = object(&[("note", text("a\"b\\c\nd\te"))]);
        let escaped = "\"a\\\"b\\\\c\\nd\\te\"";
        assert!(render_ok(&value, Format::Json).contains(escaped));
        assert!(render_ok(&value, Format::Yaml).contains(escaped));
        assert!(render_ok(&value, Format::Raw).contains(escaped));
    }

    // §8.9 — absent optional fields.

    #[test]
    fn nothing_that_is_absent_becomes_a_null_and_an_empty_list_stays_present() {
        // `spring_2026` omits the optional `slot_count`; `winter_2026` has it.
        let document = appendix_c_document();
        for format in [Format::Json, Format::Yaml, Format::Raw] {
            let output = rendered(&document, format);
            assert!(!output.contains("null"), "{format} emitted a null");
            assert!(!output.contains('~'), "{format} emitted a tilde");
            assert_eq!(
                output.matches("slot_count").count(),
                1,
                "{format} should carry slot_count once"
            );
        }

        let with_empty_list = object(&[("tags", Value::List(Vec::new()))]);
        assert_eq!(
            render_ok(&with_empty_list, Format::Json),
            "{\n  \"tags\": []\n}\n"
        );
        assert_eq!(render_ok(&with_empty_list, Format::Yaml), "\"tags\": []\n");
        assert_eq!(render_ok(&with_empty_list, Format::Raw), "tags: []\n");
    }

    // Appendix C.

    #[test]
    fn appendix_c_renders_as_the_json_of_c_6() {
        assert_eq!(
            rendered(&appendix_c_document(), Format::Json),
            APPENDIX_C_JSON
        );
    }

    #[test]
    fn appendix_c_renders_as_the_yaml_of_c_7() {
        assert_eq!(
            rendered(&appendix_c_document(), Format::Yaml),
            APPENDIX_C_YAML
        );
    }

    #[test]
    fn appendix_c_carries_the_same_scalars_in_json_and_yaml() {
        // §11.3 cross-format equivalence, checked on the values a reader
        // would compare after coercing every mapping key to a string.
        let json = rendered(&appendix_c_document(), Format::Json);
        let yaml = rendered(&appendix_c_document(), Format::Yaml);
        for token in [
            "\"spring_2026\"",
            "\"winter_2026\"",
            "\"support@example.com\"",
            "\"./textures/ember.png\"",
            "false",
            "200",
            "\"free\"",
            "\"plus\"",
        ] {
            assert_eq!(
                json.matches(token).count(),
                yaml.matches(token).count(),
                "{token} appears a different number of times"
            );
        }
    }

    // Shared shape rules of §8.4–§8.6.

    #[test]
    fn every_rendering_ends_with_one_lf_and_has_no_bom_or_trailing_space() {
        let document = appendix_c_document();
        for format in [Format::Json, Format::Yaml, Format::Raw] {
            let output = document.render(format).expect("the document renders");
            assert!(output.ends_with('\n'), "{format} must end with LF");
            assert!(!output.ends_with("\n\n"), "{format} must end with one LF");
            assert!(!output.contains('\r'), "{format} must not contain CR");
            assert!(
                !output.starts_with('\u{feff}'),
                "{format} must carry no BOM"
            );
            for line in output.lines() {
                assert_eq!(
                    line.trim_end(),
                    line,
                    "{format}: {line:?} has trailing space"
                );
            }
        }
    }

    #[test]
    fn a_document_with_no_instances_still_renders_in_every_format() {
        let document = CompiledDocument::new(VersionRange::DEFAULT, Vec::new(), Vec::new());
        assert_eq!(rendered(&document, Format::Yaml).lines().count(), 8);
        assert_eq!(rendered(&document, Format::Raw).lines().count(), 10);
        assert!(!document.to_json_string().is_empty());
        assert!(!document.to_yaml_string().is_empty());
        assert!(!document.to_raw_string().is_empty());
    }

    #[test]
    fn a_top_level_value_that_is_not_a_mapping_still_renders() {
        // The envelope is always an object; these are the degenerate shapes
        // the three renderers must survive rather than shapes a compiler
        // emits.
        assert_eq!(render_ok(&Value::Int(1), Format::Json), "1\n");
        assert_eq!(render_ok(&Value::Int(1), Format::Yaml), "1\n");
        assert_eq!(render_ok(&Value::Int(1), Format::Raw), "1\n");
        assert_eq!(render_ok(&Value::object(), Format::Json), "{}\n");
        assert_eq!(render_ok(&Value::object(), Format::Yaml), "{}\n");
        assert_eq!(render_ok(&Value::object(), Format::Raw), "{}\n");
        assert_eq!(render_ok(&text_list(&["a"]), Format::Yaml), "- \"a\"\n");
        assert_eq!(render_ok(&text_list(&["a"]), Format::Raw), "[\"a\"]\n");
    }

    // Failures.

    #[test]
    fn a_non_finite_float_is_e701_naming_its_position_and_the_format() {
        let value = object(&[(
            "data",
            list(&[object(&[("price", Value::Float(f64::INFINITY))])]),
        )]);
        for format in [Format::Json, Format::Yaml, Format::Raw] {
            let error = render(&value, format).expect_err("a non-finite float cannot be rendered");
            assert_eq!(error.len(), 1);
            let first = error.first().expect("one diagnostic");
            assert_eq!(first.id, ErrorId::E701);
            assert_eq!(
                first.message,
                format!(
                    "Value at data[0].price cannot be represented in {}.",
                    format.name()
                )
            );
        }
    }

    #[test]
    fn a_non_finite_float_inside_an_inline_raw_array_is_still_e701() {
        let value = object(&[("ratios", list(&[Value::Float(f64::NAN)]))]);
        let error = render(&value, Format::Raw).expect_err("a non-finite float cannot be rendered");
        assert_eq!(
            error.first().map(|item| item.message.clone()),
            Some("Value at ratios[0] cannot be represented in RAW.".to_string())
        );
    }

    #[test]
    fn a_non_finite_float_at_the_root_names_the_root() {
        let error = render(&Value::Float(f64::NAN), Format::Json)
            .expect_err("a non-finite float cannot be rendered");
        assert_eq!(
            error.first().map(|item| item.message.clone()),
            Some("Value at the document root cannot be represented in JSON.".to_string())
        );
    }

    /// SPEC §3.7 makes validation the single site of the depth limit, so the
    /// renderer has no check of its own: a tree that reached it is one
    /// validation already accepted, and it renders.
    #[test]
    fn the_renderer_has_no_depth_check_of_its_own() {
        for format in [Format::Json, Format::Yaml, Format::Raw] {
            assert!(
                render(&nest(66), format).is_ok(),
                "{format} must render the deepest tree validation can pass"
            );
        }
        let mut value = Value::Int(1);
        for _ in 1..=66 {
            value = Value::List(vec![value]);
        }
        for format in [Format::Json, Format::Yaml, Format::Raw] {
            assert!(render(&value, format).is_ok(), "{format} renders lists too");
        }
    }
}
