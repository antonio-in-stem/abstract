//! Diagnostics: the stable error-identifier catalogue of SPEC chapter 10 and
//! the diagnostic record every phase of the compiler reports.
//!
//! Identifiers are stable across 1.x: an identifier is never reused for a
//! different condition (SPEC §1.4). The first digit groups the error by
//! chapter: `E1xx` files and projects, `E2xx` lexical, `E3xx` schemas,
//! `E4xx` instances, `E5xx` logic, `E6xx` versions, `E7xx` output,
//! `E8xx` command line.

use std::error::Error;
use std::fmt;

/// Every diagnostic identifier defined by SPEC chapter 10.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ErrorId {
    // §10.1 E1xx — files and projects
    E101,
    E102,
    E103,

    // §10.2 E2xx — lexical
    E201,
    E202,
    E203,
    E204,
    E205,
    E206,
    E207,
    E208,
    E209,
    E210,
    E211,

    // §10.3 E3xx — schemas
    E301,
    E302,
    E303,
    E304,
    E305,
    E306,
    E307,
    E308,
    E309,
    E310,
    E311,
    E312,
    E313,
    E314,
    E315,
    E316,
    E317,
    E318,
    E319,
    E320,
    E321,
    E322,
    E323,

    // §10.4 E4xx — instances
    E401,
    E402,
    E403,
    E404,
    E405,
    E406,
    E407,
    E408,
    E409,
    E410,
    E411,
    E412,
    E413,
    E414,
    E415,
    E416,
    E417,
    E418,
    E419,
    E420,
    E421,
    E422,
    E423,
    E424,
    E425,
    E426,
    E427,
    E428,
    E429,
    E430,
    E431,
    E432,
    E433,
    E434,
    E435,
    E436,
    E437,
    E438,
    E439,
    E440,
    E441,
    E442,
    E443,
    E444,
    E445,

    // §10.5 E5xx — logic
    E501,
    E502,
    E503,
    E504,
    E505,
    E506,
    E507,
    E508,
    E509,
    E510,
    E511,
    E512,
    E513,
    E514,
    E515,
    E516,
    E517,
    E518,
    E519,
    E520,
    E521,
    E522,
    E523,
    E524,
    E525,

    // §10.6 E6xx — versions
    E601,
    E602,
    E603,
    E604,
    E605,

    // §10.7 E7xx — output
    E701,
    E702,
    E703,

    // §10.8 E8xx — command line
    E801,
    E802,
    E803,
    E804,
    E805,
    E806,
    E807,
    E808,
    E810,
    E811,
    E812,
    E813,
}

impl ErrorId {
    /// Every catalogue identifier, ascending.
    pub const ALL: &'static [ErrorId] = &[
        ErrorId::E101,
        ErrorId::E102,
        ErrorId::E103,
        ErrorId::E201,
        ErrorId::E202,
        ErrorId::E203,
        ErrorId::E204,
        ErrorId::E205,
        ErrorId::E206,
        ErrorId::E207,
        ErrorId::E208,
        ErrorId::E209,
        ErrorId::E210,
        ErrorId::E211,
        ErrorId::E301,
        ErrorId::E302,
        ErrorId::E303,
        ErrorId::E304,
        ErrorId::E305,
        ErrorId::E306,
        ErrorId::E307,
        ErrorId::E308,
        ErrorId::E309,
        ErrorId::E310,
        ErrorId::E311,
        ErrorId::E312,
        ErrorId::E313,
        ErrorId::E314,
        ErrorId::E315,
        ErrorId::E316,
        ErrorId::E317,
        ErrorId::E318,
        ErrorId::E319,
        ErrorId::E320,
        ErrorId::E321,
        ErrorId::E322,
        ErrorId::E323,
        ErrorId::E401,
        ErrorId::E402,
        ErrorId::E403,
        ErrorId::E404,
        ErrorId::E405,
        ErrorId::E406,
        ErrorId::E407,
        ErrorId::E408,
        ErrorId::E409,
        ErrorId::E410,
        ErrorId::E411,
        ErrorId::E412,
        ErrorId::E413,
        ErrorId::E414,
        ErrorId::E415,
        ErrorId::E416,
        ErrorId::E417,
        ErrorId::E418,
        ErrorId::E419,
        ErrorId::E420,
        ErrorId::E421,
        ErrorId::E422,
        ErrorId::E423,
        ErrorId::E424,
        ErrorId::E425,
        ErrorId::E426,
        ErrorId::E427,
        ErrorId::E428,
        ErrorId::E429,
        ErrorId::E430,
        ErrorId::E431,
        ErrorId::E432,
        ErrorId::E433,
        ErrorId::E434,
        ErrorId::E435,
        ErrorId::E436,
        ErrorId::E437,
        ErrorId::E438,
        ErrorId::E439,
        ErrorId::E440,
        ErrorId::E441,
        ErrorId::E442,
        ErrorId::E443,
        ErrorId::E444,
        ErrorId::E445,
        ErrorId::E501,
        ErrorId::E502,
        ErrorId::E503,
        ErrorId::E504,
        ErrorId::E505,
        ErrorId::E506,
        ErrorId::E507,
        ErrorId::E508,
        ErrorId::E509,
        ErrorId::E510,
        ErrorId::E511,
        ErrorId::E512,
        ErrorId::E513,
        ErrorId::E514,
        ErrorId::E515,
        ErrorId::E516,
        ErrorId::E517,
        ErrorId::E518,
        ErrorId::E519,
        ErrorId::E520,
        ErrorId::E521,
        ErrorId::E522,
        ErrorId::E523,
        ErrorId::E524,
        ErrorId::E525,
        ErrorId::E601,
        ErrorId::E602,
        ErrorId::E603,
        ErrorId::E604,
        ErrorId::E605,
        ErrorId::E701,
        ErrorId::E702,
        ErrorId::E703,
        ErrorId::E801,
        ErrorId::E802,
        ErrorId::E803,
        ErrorId::E804,
        ErrorId::E805,
        ErrorId::E806,
        ErrorId::E807,
        ErrorId::E808,
        ErrorId::E810,
        ErrorId::E811,
        ErrorId::E812,
        ErrorId::E813,
    ];

    /// The identifier as it appears in a diagnostic line, for example `E412`.
    pub fn code(self) -> &'static str {
        match self {
            ErrorId::E101 => "E101",
            ErrorId::E102 => "E102",
            ErrorId::E103 => "E103",
            ErrorId::E201 => "E201",
            ErrorId::E202 => "E202",
            ErrorId::E203 => "E203",
            ErrorId::E204 => "E204",
            ErrorId::E205 => "E205",
            ErrorId::E206 => "E206",
            ErrorId::E207 => "E207",
            ErrorId::E208 => "E208",
            ErrorId::E209 => "E209",
            ErrorId::E210 => "E210",
            ErrorId::E211 => "E211",
            ErrorId::E301 => "E301",
            ErrorId::E302 => "E302",
            ErrorId::E303 => "E303",
            ErrorId::E304 => "E304",
            ErrorId::E305 => "E305",
            ErrorId::E306 => "E306",
            ErrorId::E307 => "E307",
            ErrorId::E308 => "E308",
            ErrorId::E309 => "E309",
            ErrorId::E310 => "E310",
            ErrorId::E311 => "E311",
            ErrorId::E312 => "E312",
            ErrorId::E313 => "E313",
            ErrorId::E314 => "E314",
            ErrorId::E315 => "E315",
            ErrorId::E316 => "E316",
            ErrorId::E317 => "E317",
            ErrorId::E318 => "E318",
            ErrorId::E319 => "E319",
            ErrorId::E320 => "E320",
            ErrorId::E321 => "E321",
            ErrorId::E322 => "E322",
            ErrorId::E323 => "E323",
            ErrorId::E401 => "E401",
            ErrorId::E402 => "E402",
            ErrorId::E403 => "E403",
            ErrorId::E404 => "E404",
            ErrorId::E405 => "E405",
            ErrorId::E406 => "E406",
            ErrorId::E407 => "E407",
            ErrorId::E408 => "E408",
            ErrorId::E409 => "E409",
            ErrorId::E410 => "E410",
            ErrorId::E411 => "E411",
            ErrorId::E412 => "E412",
            ErrorId::E413 => "E413",
            ErrorId::E414 => "E414",
            ErrorId::E415 => "E415",
            ErrorId::E416 => "E416",
            ErrorId::E417 => "E417",
            ErrorId::E418 => "E418",
            ErrorId::E419 => "E419",
            ErrorId::E420 => "E420",
            ErrorId::E421 => "E421",
            ErrorId::E422 => "E422",
            ErrorId::E423 => "E423",
            ErrorId::E424 => "E424",
            ErrorId::E425 => "E425",
            ErrorId::E426 => "E426",
            ErrorId::E427 => "E427",
            ErrorId::E428 => "E428",
            ErrorId::E429 => "E429",
            ErrorId::E430 => "E430",
            ErrorId::E431 => "E431",
            ErrorId::E432 => "E432",
            ErrorId::E433 => "E433",
            ErrorId::E434 => "E434",
            ErrorId::E435 => "E435",
            ErrorId::E436 => "E436",
            ErrorId::E437 => "E437",
            ErrorId::E438 => "E438",
            ErrorId::E439 => "E439",
            ErrorId::E440 => "E440",
            ErrorId::E441 => "E441",
            ErrorId::E442 => "E442",
            ErrorId::E443 => "E443",
            ErrorId::E444 => "E444",
            ErrorId::E445 => "E445",
            ErrorId::E501 => "E501",
            ErrorId::E502 => "E502",
            ErrorId::E503 => "E503",
            ErrorId::E504 => "E504",
            ErrorId::E505 => "E505",
            ErrorId::E506 => "E506",
            ErrorId::E507 => "E507",
            ErrorId::E508 => "E508",
            ErrorId::E509 => "E509",
            ErrorId::E510 => "E510",
            ErrorId::E511 => "E511",
            ErrorId::E512 => "E512",
            ErrorId::E513 => "E513",
            ErrorId::E514 => "E514",
            ErrorId::E515 => "E515",
            ErrorId::E516 => "E516",
            ErrorId::E517 => "E517",
            ErrorId::E518 => "E518",
            ErrorId::E519 => "E519",
            ErrorId::E520 => "E520",
            ErrorId::E521 => "E521",
            ErrorId::E522 => "E522",
            ErrorId::E523 => "E523",
            ErrorId::E524 => "E524",
            ErrorId::E525 => "E525",
            ErrorId::E601 => "E601",
            ErrorId::E602 => "E602",
            ErrorId::E603 => "E603",
            ErrorId::E604 => "E604",
            ErrorId::E605 => "E605",
            ErrorId::E701 => "E701",
            ErrorId::E702 => "E702",
            ErrorId::E703 => "E703",
            ErrorId::E801 => "E801",
            ErrorId::E802 => "E802",
            ErrorId::E803 => "E803",
            ErrorId::E804 => "E804",
            ErrorId::E805 => "E805",
            ErrorId::E806 => "E806",
            ErrorId::E807 => "E807",
            ErrorId::E808 => "E808",
            ErrorId::E810 => "E810",
            ErrorId::E811 => "E811",
            ErrorId::E812 => "E812",
            ErrorId::E813 => "E813",
        }
    }

    /// Parses a catalogue code such as `"E412"`. `"E000"` is not accepted.
    pub fn from_code(code: &str) -> Option<ErrorId> {
        ErrorId::ALL.iter().copied().find(|id| id.code() == code)
    }

    /// The message templates of SPEC chapter 10, in the order the catalogue
    /// lists them. A `{name}` marks a substitution (SPEC §9.8). Several ids
    /// carry alternative templates, one per distinct condition.
    pub fn templates(self) -> &'static [&'static str] {
        match self {
            ErrorId::E101 => &["Cannot read source file '{path}': {reason}."],
            ErrorId::E102 => {
                &["Source file '{path}' is not valid UTF-8 (first bad byte at offset {offset})."]
            }
            ErrorId::E103 => &["No .ab or .abt files were found under '{path}'."],

            ErrorId::E201 => {
                &["Unterminated string literal; strings must open and close on the same line."]
            }
            ErrorId::E202 => &[
                r#"Unknown escape '\{char}' in a string literal; valid escapes are \" \\ \n \r \t."#,
            ],
            ErrorId::E203 => &["Unclosed '{bracket}' opened at {line}:{col}."],
            ErrorId::E204 => &[
                "Mismatched bracket: expected '{expected}' to close '{open}' opened at {line}:{col}, found '{found}'.",
            ],
            ErrorId::E205 => &["Unexpected '{bracket}'."],
            ErrorId::E206 => &[
                "Invalid character '{char}' in identifier '{text}'; identifiers use A-Z a-z 0-9 _ -.",
            ],
            ErrorId::E207 => &["Identifier '{text}' must not end with '-'."],
            ErrorId::E208 => &[
                "Invalid schema name '{text}'; schema names start with a letter and contain letters, digits and '_'.",
            ],
            ErrorId::E209 => &["{subject} exceeds the limit of {limit}."],
            ErrorId::E210 => &["Unexpected {found} here; expected {expected}."],
            ErrorId::E211 => &["'{text}' is not a valid {kind} literal."],

            ErrorId::E301 => &["Schema '{name}' is already declared."],
            ErrorId::E302 => &["Field '{name}' is already declared in this block."],
            ErrorId::E303 => &[
                "Unknown field modifier '{text}'.",
                "Modifier '{text}' is repeated.",
            ],
            ErrorId::E304 => &[
                "Unknown type '{text}'; expected text, int, float, bool, enum, file, image, ref or $(Schema).",
            ],
            ErrorId::E305 => {
                &["Invalid range '{text}'; ranges are 'value' or 'min..max' with min <= max."]
            }
            ErrorId::E306 => &[
                "enum(...) must declare at least one member.",
                "Duplicate enum member '{name}'.",
            ],
            ErrorId::E307 => {
                &["Unsupported image format '{ext}'; supported: png, jpg, gif, bmp, webp."]
            }
            ErrorId::E308 => &[
                "Invalid image size '{text}'; expected WIDTHxHEIGHT where each side is a number or '*'.",
            ],
            ErrorId::E309 => &["Unknown schema '{name}' referenced here."],
            ErrorId::E310 => &["Group '{name}' already has a @tag field '{first}'."],
            ErrorId::E311 => &[
                "@tag is only allowed on a field inside a group; '{name}' is a root field of schema '{schema}'.",
            ],
            ErrorId::E312 => &["A group field cannot declare a default."],
            ErrorId::E313 => &["Default value {value} is invalid for {schema}.{field}: {reason}."],
            ErrorId::E314 => &[
                "'template' is a reserved envelope key and cannot be declared as a field.",
                "'id' may only be declared as 'id: text' or 'id: text(min..max)' with no modifiers and no default; found {found}.",
            ],
            ErrorId::E315 => &["'{name}[][]' is not valid; a field is a list or it is not."],
            ErrorId::E316 => &["Invalid field name '{text}'."],
            ErrorId::E317 => &["{type}(...) requires at least one argument."],
            ErrorId::E318 => &["Constraint '{text}' on {schema}.{field} can never be satisfied."],
            ErrorId::E319 => &["Duplicate extension '{ext}' in {type}(...)."],
            ErrorId::E320 => &[
                "{schema}.{field} declares both @optional and a default; a default already makes the field present.",
            ],
            ErrorId::E321 => &[
                "Schema '{a}' requires '{b}' which requires '{a}'; the structure can never be built.",
            ],
            ErrorId::E322 => &[
                "@tag requires a non-list field of type text, int, bool or enum; '{name}' is {kind}.",
            ],
            ErrorId::E323 => &[
                "Invalid list cardinality '{text}'; write '[]', '[min..]' or '[min..max]' with 0 <= min <= max.",
            ],

            ErrorId::E401 => &["Unknown template '{name}'."],
            ErrorId::E402 => &["Duplicate instance id '{id}'."],
            ErrorId::E403 => &["A statement cannot appear before the first instance header."],
            ErrorId::E404 => &[
                "A clone must appear immediately after the instance header, before any assignment.",
            ],
            ErrorId::E405 => &["Unknown clone target '{id}'."],
            ErrorId::E406 => &["Clone cycle detected: {cycle}."],
            ErrorId::E407 => &[
                "Instance '{id}' is a '{other}'; a clone source must use the same template '{template}'.",
            ],
            ErrorId::E408 => {
                &["Clone path '{path}' does not exist on instance '{id}' in any version."]
            }
            ErrorId::E409 => &["Unknown field '{name}' in {context}."],
            ErrorId::E410 => &["'{name}' is a reserved envelope key and cannot be assigned."],
            ErrorId::E411 => &["Missing required field {context}.{field}."],
            ErrorId::E412 => &["Type mismatch at {context}.{field}: expected {type}, found {found}."],
            ErrorId::E413 => &["Range mismatch at {context}.{field}: {value} is not in {ranges}."],
            ErrorId::E414 => {
                &["Enum mismatch at {context}.{field}: '{value}' is not one of {members}."]
            }
            ErrorId::E415 => &["Wildcard '{prefix}*' matches no member of {members}."],
            ErrorId::E416 => &[
                "Tuple row {n} has {found} values but {field} declares {expected} columns ({columns}).",
            ],
            ErrorId::E417 => &["Duplicate tuple column '{name}'."],
            ErrorId::E418 => {
                &["{context} has no @tag field, so '#{name}' shorthand cannot be used here."]
            }
            ErrorId::E419 => &[
                "Invalid argument '{text}' in '#{name}(...)'; arguments are 'key: value' or a bare boolean flag.",
            ],
            ErrorId::E420 => {
                &["Extension '{ext}' is not allowed at {context}.{field}; allowed: {list}."]
            }
            ErrorId::E421 => {
                &["File not found for {context}.{field}: '{path}' (resolved to 'assets/{rest}')."]
            }
            ErrorId::E422 => {
                &["Image content mismatch at {context}.{field}: '{path}' is {actual}, not {expected}."]
            }
            ErrorId::E423 => &[
                "Image size mismatch at {context}.{field}: '{path}' is {w}x{h}, expected one of {list}.",
            ],
            ErrorId::E424 => &[
                "Asset path '{path}' must be relative to assets/ and must not be absolute or contain '..'.",
            ],
            ErrorId::E425 => &["Unknown variable '${name}'."],
            ErrorId::E426 => &["'$' must be followed by a name, '{' or '$'."],
            ErrorId::E427 => &["Unterminated '${'."],
            ErrorId::E428 => {
                &["Invalid instance id '{text}'; ids are identifiers (A-Z a-z 0-9 _ -)."]
            }
            ErrorId::E429 => &["{path} is assigned twice for version(s) {versions}."],
            ErrorId::E430 => &[
                "This statement can never apply: {field} exists in versions {a}, the statement is annotated {b}.",
            ],
            ErrorId::E431 => &[
                "Unknown reference '{id}' at {context}.{field}.",
                "Reference '{id}' at {context}.{field} does not exist in version {v}.",
            ],
            ErrorId::E432 => {
                &["Reference '{id}' at {context}.{field} is a '{actual}', expected a '{expected}'."]
            }
            ErrorId::E433 => &["A multi-path must name at least one key."],
            ErrorId::E434 => &[
                "Brace patterns expand into several paths and require a list field; {context}.{field} is not a list.",
            ],
            ErrorId::E435 => &["Invalid brace pattern '{text}': {reason}."],
            ErrorId::E436 => &["An instance header contains exactly one '::'."],
            ErrorId::E437 => &[
                "@since/@removed are only allowed on a body statement or at the end of an instance header.",
            ],
            ErrorId::E438 => &[
                "Header tag '@{name}' targets {kind} '{field}'; a header tag assigns one root scalar field. Write a body statement.",
            ],
            ErrorId::E439 => &["'::' starts an instance header, but '{text}' is not a schema name."],
            ErrorId::E440 => &[
                "This statement applies to versions {a}, but instance '{id}' exists only in versions {b}.",
                "Clone source '{source}' exists in versions {a}, but instance '{id}' exists in versions {b}.",
            ],
            ErrorId::E441 => &["Nested lists are not supported.", "Empty list item."],
            ErrorId::E442 => &["Trailing comma; a statement does not continue onto the next line."],
            ErrorId::E443 => &[
                "Cannot assign '{path}': '{prefix}' already holds {kind}.",
                "Cannot assign '{path}': '{prefix}' is a list.",
            ],
            ErrorId::E444 => {
                &["{context}.{field} already has an element whose {tag} is '{key}'."]
            }
            ErrorId::E445 => &["{context}.{field} has {found} elements; {field} declares {expected}."],

            ErrorId::E501 => &["Unknown schema '{name}' in logic block."],
            ErrorId::E502 => &["Schema '{name}' already has a logic block."],
            ErrorId::E503 => &["Unknown path '{path}' in logic for '{schema}'."],
            ErrorId::E504 => &["derive cannot write '{name}'."],
            ErrorId::E505 => &["derive target '{path}' is not a declared field of '{schema}'."],
            ErrorId::E506 => {
                &["derive cannot write through a list; '{path}' crosses list field '{field}'."]
            }
            ErrorId::E507 => &["require must be followed by 'else throw \"message\"'."],
            ErrorId::E508 => &["A throw message must be a quoted string."],
            ErrorId::E509 => &["'{path}' is not a list; for iterates lists and literal lists."],
            ErrorId::E510 => &["Unbound variable '${name}'."],
            ErrorId::E511 => &["Invalid condition '{text}': {reason}."],
            ErrorId::E512 => &["Comparisons cannot be chained; use 'and'."],
            ErrorId::E513 => &["Operator '{op}' cannot compare {left} and {right}."],
            ErrorId::E514 => &["length() requires a list or text value; '{path}' is {kind}."],
            ErrorId::E515 => &["{schema} logic: {message}"],
            ErrorId::E516 => &["Loop variable '${name}' is already bound by an enclosing loop."],
            ErrorId::E517 => &["'else' must follow the closing '}' of an if block."],
            ErrorId::E518 => &[
                "derive? can never apply to '{field}': the schema declares a default, which is filled before logic runs.",
            ],
            ErrorId::E519 => &[
                "Operator '{op}' requires a single value; '{path}' crosses list field '{field}'. Use 'contains'.",
            ],
            ErrorId::E520 => {
                &["A derive target cannot contain the variable '${name}'; write the field name."]
            }
            ErrorId::E521 => &[
                "derive cannot write {context}.{field} in version {v}; the field exists in versions {versions}.",
            ],
            ErrorId::E522 => &["Variable '${name}' is not a scalar; it is {kind}."],
            ErrorId::E523 => &[
                "Logic work limit exceeded at {context}: the logic executed more than 1000000 loop iterations in version {v}.",
            ],
            ErrorId::E524 => &["Invalid arithmetic expression: {reason}."],
            ErrorId::E525 => &["Arithmetic evaluation failed: {reason}."],

            ErrorId::E601 => &["The project already declares a version range."],
            ErrorId::E602 => &[
                "Invalid version range '{min}..{max}'; both are integers >= 1 and min <= max.",
            ],
            ErrorId::E603 => &["Version {n} is outside the project range {min}..{max}."],
            ErrorId::E604 => &["@removed({r}) must be greater than @since({s})."],
            ErrorId::E605 => &["{schema}.{field} exists in no version: {reason}."],

            ErrorId::E701 => &["Value at {context}.{field} cannot be represented in {format}."],
            ErrorId::E702 => &["Public contract profile cannot admit {target}: {reason}."],
            ErrorId::E703 => &["Public contract export rejected: {reason}."],

            ErrorId::E801 => {
                &["Unknown command '{text}'; expected compile, lint, templates or init."]
            }
            ErrorId::E802 => &["Unknown flag '{text}'."],
            ErrorId::E803 => &["Flag '{flag}' requires a value."],
            ErrorId::E804 => &["Unexpected argument '{text}'."],
            ErrorId::E805 => {
                &["Unknown output format '{text}'; expected JSON, YML, YAML or RAW."]
            }
            ErrorId::E806 => &[
                "Input not found: '{path}'.",
                "'{path}' is not an .ab or .abt file.",
                "No input path was given.",
                "'{path}' is inside the data directory '{data}'.",
                "'{path}' contains more than one 'data' directory.",
            ],
            ErrorId::E807 => &[
                "Cannot mix files and directories in one invocation.",
                "Inputs belong to different projects: '{a}' and '{b}'.",
            ],
            ErrorId::E808 => &[
                "Refusing to write '{path}': it is a source file.",
                "Refusing to write '{path}': it is inside the data directory.",
                "Refusing to write '{path}': an '.ab' or '.abt' file inside the discovery root would be read on the next run.",
            ],
            ErrorId::E810 => &["Cannot write '{path}': {reason}."],
            ErrorId::E811 => &["Flag '{flag}' was given twice."],
            ErrorId::E812 => &["Flag '{flag}' requires {expected}; found '{text}'."],
            ErrorId::E813 => &["'{path}' is not empty."],
        }
    }

    /// The first message template. Ids with a single condition have only one.
    pub fn template(self) -> &'static str {
        self.templates()[0]
    }

    /// The exit code a run that reported this identifier as its first
    /// diagnostic must use (SPEC §9.6).
    pub fn exit_code(self) -> i32 {
        match self {
            ErrorId::E810 => 3,
            ErrorId::E801
            | ErrorId::E802
            | ErrorId::E803
            | ErrorId::E804
            | ErrorId::E805
            | ErrorId::E806
            | ErrorId::E807
            | ErrorId::E808
            | ErrorId::E811
            | ErrorId::E812
            | ErrorId::E813 => 2,
            _ => 1,
        }
    }
}

impl fmt::Display for ErrorId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

/// A source position. Lines and columns are 1-based and a column counts
/// Unicode scalar values, not bytes (SPEC §2.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Position {
    pub line: u32,
    pub col: u32,
}

impl Position {
    pub fn new(line: u32, col: u32) -> Self {
        Self { line, col }
    }
}

impl fmt::Display for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.line, self.col)
    }
}

/// A secondary line attached to a diagnostic (SPEC §9.8). A note may carry its
/// own position, for example the first declaration site of a duplicate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Note {
    pub file: Option<String>,
    pub position: Option<Position>,
    pub text: String,
}

impl Note {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            file: None,
            position: None,
            text: text.into(),
        }
    }

    /// Attaches a second source position to this note.
    pub fn at(mut self, file: impl Into<String>, position: Position) -> Self {
        self.file = Some(file.into());
        self.position = Some(position);
        self
    }
}

impl fmt::Display for Note {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (&self.file, self.position) {
            (Some(file), Some(position)) => write!(f, "{file}:{position}: note: {}", self.text),
            (Some(file), None) => write!(f, "{file}: note: {}", self.text),
            _ => write!(f, "note: {}", self.text),
        }
    }
}

/// One reported problem.
///
/// The rendering follows SPEC §9.8: `<path>:<line>:<col>: error[<ID>]:
/// <message>`, degrading to `<path>:<line>`, `<path>` and finally `abstract`
/// as position information is missing. Paths are project-relative with `/`
/// separators; the compiler never prints an absolute or extended-length path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub id: ErrorId,
    pub file: Option<String>,
    pub position: Option<Position>,
    pub message: String,
    pub notes: Vec<Note>,
}

impl Diagnostic {
    /// A diagnostic with no source position.
    pub fn new(id: ErrorId, message: impl Into<String>) -> Self {
        Self {
            id,
            file: None,
            position: None,
            message: message.into(),
            notes: Vec::new(),
        }
    }

    /// A diagnostic naming a file, a line and a column.
    pub fn at(
        id: ErrorId,
        file: impl Into<String>,
        position: Position,
        message: impl Into<String>,
    ) -> Self {
        Self {
            id,
            file: Some(file.into()),
            position: Some(position),
            message: message.into(),
            notes: Vec::new(),
        }
    }

    /// A diagnostic naming a file but no position inside it.
    pub fn in_file(id: ErrorId, file: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            id,
            file: Some(file.into()),
            position: None,
            message: message.into(),
            notes: Vec::new(),
        }
    }

    pub fn with_note(mut self, note: Note) -> Self {
        self.notes.push(note);
        self
    }

    pub fn with_note_text(self, text: impl Into<String>) -> Self {
        self.with_note(Note::new(text))
    }

    /// The exit code this diagnostic implies when it is the first reported.
    pub fn exit_code(&self) -> i32 {
        self.id.exit_code()
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (&self.file, self.position) {
            (Some(file), Some(position)) => {
                write!(f, "{file}:{position}: error[{}]: {}", self.id, self.message)?
            }
            (Some(file), None) => write!(f, "{file}: error[{}]: {}", self.id, self.message)?,
            (None, _) => write!(f, "abstract: error[{}]: {}", self.id, self.message)?,
        }
        for note in &self.notes {
            write!(f, "\n  {note}")?;
        }
        Ok(())
    }
}

/// The diagnostics one compilation reported, in the deterministic order of
/// SPEC §11.1. A compilation that produced no diagnostic returns its document
/// instead, so a `Diagnostics` value handed back as an error is never empty.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Diagnostics {
    items: Vec<Diagnostic>,
    /// Set when reporting stopped at the `--max-errors` limit (SPEC §9.3).
    truncated_at: Option<usize>,
}

impl Diagnostics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn one(diagnostic: Diagnostic) -> Self {
        Self {
            items: vec![diagnostic],
            truncated_at: None,
        }
    }

    pub fn push(&mut self, diagnostic: Diagnostic) {
        self.items.push(diagnostic);
    }

    pub fn extend(&mut self, other: Diagnostics) {
        self.items.extend(other.items);
        if self.truncated_at.is_none() {
            self.truncated_at = other.truncated_at;
        }
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn first(&self) -> Option<&Diagnostic> {
        self.items.first()
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Diagnostic> {
        self.items.iter()
    }

    pub fn as_slice(&self) -> &[Diagnostic] {
        &self.items
    }

    /// Marks the collection as cut short at `limit` diagnostics.
    pub fn set_truncated_at(&mut self, limit: usize) {
        self.truncated_at = Some(limit);
    }

    pub fn truncated_at(&self) -> Option<usize> {
        self.truncated_at
    }

    /// The exit code of a run that reported these diagnostics (SPEC §9.6).
    pub fn exit_code(&self) -> i32 {
        self.items.first().map(Diagnostic::exit_code).unwrap_or(0)
    }
}

impl From<Diagnostic> for Diagnostics {
    fn from(diagnostic: Diagnostic) -> Self {
        Diagnostics::one(diagnostic)
    }
}

impl IntoIterator for Diagnostics {
    type Item = Diagnostic;
    type IntoIter = std::vec::IntoIter<Diagnostic>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.into_iter()
    }
}

impl<'a> IntoIterator for &'a Diagnostics {
    type Item = &'a Diagnostic;
    type IntoIter = std::slice::Iter<'a, Diagnostic>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.iter()
    }
}

impl fmt::Display for Diagnostics {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut first = true;
        for item in &self.items {
            if !first {
                f.write_str("\n")?;
            }
            first = false;
            write!(f, "{item}")?;
        }
        if let Some(limit) = self.truncated_at {
            if !first {
                f.write_str("\n")?;
            }
            write!(
                f,
                "abstract: note: stopping after {limit} errors; more may remain."
            )?;
        }
        Ok(())
    }
}

impl Error for Diagnostics {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_catalogue_id_has_a_unique_code_and_a_template() {
        let mut seen: Vec<&'static str> = Vec::new();
        for id in ErrorId::ALL.iter().copied() {
            let code = id.code();
            assert!(!seen.contains(&code), "duplicate code {code}");
            seen.push(code);
            assert_eq!(ErrorId::from_code(code), Some(id));
            assert!(!id.templates().is_empty());
            assert!(id.template().ends_with('.') || id.template().ends_with('}'));
        }
        assert_eq!(seen.len(), 127);
        assert_eq!(ErrorId::from_code("E000"), None);
        assert_eq!(ErrorId::from_code("E809"), None);
    }

    #[test]
    fn diagnostics_render_in_the_documented_shapes() {
        let full = Diagnostic::at(
            ErrorId::E412,
            "data/items/a.ab",
            Position::new(3, 9),
            "Type mismatch at a.count: expected int, found text.",
        );
        assert_eq!(
            full.to_string(),
            "data/items/a.ab:3:9: error[E412]: Type mismatch at a.count: expected int, found text."
        );

        let with_note = Diagnostic::in_file(ErrorId::E102, "data/a.ab", "Bad bytes.")
            .with_note_text("second line.");
        assert_eq!(
            with_note.to_string(),
            "data/a.ab: error[E102]: Bad bytes.\n  note: second line."
        );

        let bare = Diagnostic::new(ErrorId::E103, "No sources.");
        assert_eq!(bare.to_string(), "abstract: error[E103]: No sources.");
    }

    #[test]
    fn exit_codes_follow_the_specification() {
        assert_eq!(ErrorId::E412.exit_code(), 1);
        assert_eq!(ErrorId::E806.exit_code(), 2);
        assert_eq!(ErrorId::E810.exit_code(), 3);
    }
}
