use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use abstract_lang::{
    bundle, compile_paths, compile_project, known_template_names, CompileOptions, CompiledProject,
};

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Copy, PartialEq)]
enum OutputFormat {
    Json,
    Yml,
    Raw,
}

impl OutputFormat {
    fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "json" => Some(Self::Json),
            "yml" | "yaml" => Some(Self::Yml),
            "raw" => Some(Self::Raw),
            _ => None,
        }
    }

    fn extension(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Yml => "yml",
            Self::Raw => "abraw",
        }
    }

    fn render(self, compiled: &CompiledProject) -> String {
        match self {
            Self::Json => compiled.to_json_string(),
            Self::Yml => compiled.to_yaml_string(),
            Self::Raw => compiled.to_raw_string(),
        }
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("abstract: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() || args[0] == "--help" || args[0] == "-h" || args[0] == "help" {
        print_help();
        return Ok(());
    }
    if args[0] == "--version" || args[0] == "-V" || args[0] == "version" {
        println!("abstract {VERSION}");
        return Ok(());
    }

    let command = args.remove(0);
    match command.as_str() {
        "compile" => {
            let request = parse_compile_request(args)?;
            let compiled = compile_project(&request.input, request.options.clone())?;
            emit_compiled(compiled, request)?;
        }
        "lint" => {
            let options = collect_options(&args);
            let input = take_input(&args)?;
            compile_project(&input, options)?;
            println!("abstract: ok");
        }
        "templates" => {
            let input = take_input(&args)?;
            for name in known_template_names(&input)? {
                println!("{name}");
            }
        }
        "init" => {
            let target = args
                .iter()
                .find(|arg| !arg.starts_with('-'))
                .map(PathBuf::from)
                .ok_or("usage: abstract init <directory>")?;
            init_project(&target)?;
        }
        "bundle" => {
            run_bundle(args)?;
        }
        "unbundle" => {
            run_unbundle(args)?;
        }
        other => {
            if looks_like_path(other) {
                let mut request = parse_direct_request(other, args)?;
                validate_input_paths(&request.paths)?;
                let compiled = compile_paths(&request.paths, request.options.clone())?;
                request.input = request.paths[0].clone();
                emit_compiled(compiled, request)?;
            } else {
                return Err(format!(
                    "unknown command '{other}' (run 'abstract --help' for usage)"
                )
                .into());
            }
        }
    }
    Ok(())
}

struct CompileRequest {
    input: PathBuf,
    paths: Vec<PathBuf>,
    format: OutputFormat,
    write_file: bool,
    out: Option<PathBuf>,
    options: CompileOptions,
}

fn collect_options(args: &[String]) -> CompileOptions {
    CompileOptions {
        allow_unknown_fields: args.iter().any(|arg| arg == "--allow-unknown"),
        skip_asset_checks: args.iter().any(|arg| arg == "--skip-assets"),
    }
}

fn flag_value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|arg| arg == flag)
        .and_then(|index| args.get(index + 1))
        .cloned()
}

/// Arguments consumed by `--flag value` pairs, so positional parsing can
/// ignore them.
fn positional_args(args: &[String]) -> Vec<String> {
    let mut output = Vec::new();
    let mut skip_next = false;
    for arg in args {
        if skip_next {
            skip_next = false;
            continue;
        }
        if arg == "--out" || arg == "--key" {
            skip_next = true;
            continue;
        }
        if arg.starts_with("--") {
            continue;
        }
        output.push(arg.clone());
    }
    output
}

fn parse_compile_request(args: Vec<String>) -> Result<CompileRequest, Box<dyn std::error::Error>> {
    let options = collect_options(&args);
    let out = flag_value(&args, "--out").map(PathBuf::from);
    let positional = positional_args(&args);
    let input = take_input(&positional)?;
    let format = positional
        .iter()
        .find_map(|arg| OutputFormat::parse(arg))
        .unwrap_or(OutputFormat::Json);
    let write_file = positional.iter().any(|arg| is_true(arg)) || out.is_some();
    Ok(CompileRequest {
        input: input.clone(),
        paths: vec![input],
        format,
        write_file,
        out,
        options,
    })
}

fn parse_direct_request(
    first: &str,
    args: Vec<String>,
) -> Result<CompileRequest, Box<dyn std::error::Error>> {
    let options = collect_options(&args);
    let out = flag_value(&args, "--out").map(PathBuf::from);
    let positional = positional_args(&args);
    let format_index = positional
        .iter()
        .position(|arg| OutputFormat::parse(arg).is_some())
        .ok_or("missing output format: expected JSON, YML, or RAW")?;
    let format = OutputFormat::parse(&positional[format_index]).expect("format was checked above");
    let write_file = positional
        .get(format_index + 1)
        .map(|arg| is_true(arg))
        .unwrap_or(false)
        || out.is_some();

    let mut paths = vec![PathBuf::from(first)];
    paths.extend(positional[..format_index].iter().map(PathBuf::from));

    Ok(CompileRequest {
        input: paths[0].clone(),
        paths,
        format,
        write_file,
        out,
        options,
    })
}

fn validate_input_paths(paths: &[PathBuf]) -> Result<(), Box<dyn std::error::Error>> {
    for path in paths {
        if path.exists() {
            continue;
        }

        let cwd = env::current_dir()?;
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        let mut message = format!(
            "Input file not found: '{}'.\n  Current directory: {}\n  Resolved path: {}",
            path.display(),
            cwd.display(),
            cwd.join(path).display()
        );

        if extension == "abt" || extension == "ab" {
            let candidates = find_sibling_candidates(path, extension);
            if !candidates.is_empty() {
                message.push_str(&format!(
                    "\n  Available .{} files here: {}",
                    extension,
                    candidates.join(", ")
                ));
            }
        }

        message.push_str(
            "\n  Fix: run the command from the folder that contains the file, or pass the full/relative path explicitly.",
        );
        return Err(message.into());
    }
    Ok(())
}

fn find_sibling_candidates(path: &Path, extension: &str) -> Vec<String> {
    let directory = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    let Ok(entries) = fs::read_dir(directory) else {
        return Vec::new();
    };

    let mut candidates = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|candidate| {
            candidate.extension().and_then(|value| value.to_str()) == Some(extension)
        })
        .filter_map(|candidate| {
            candidate
                .file_name()
                .and_then(|value| value.to_str())
                .map(str::to_string)
        })
        .collect::<Vec<_>>();
    candidates.sort();
    candidates
}

fn emit_compiled(
    compiled: CompiledProject,
    request: CompileRequest,
) -> Result<(), Box<dyn std::error::Error>> {
    let output = request.format.render(&compiled);
    print!("{output}");
    if request.write_file {
        let output_path = request
            .out
            .clone()
            .unwrap_or_else(|| default_output_path(&request.input, request.format));
        fs::write(&output_path, output)?;
        eprintln!(
            "abstract: wrote {} ({})",
            output_path.display(),
            request.format.extension()
        );
    }
    Ok(())
}

fn default_output_path(input: &Path, format: OutputFormat) -> PathBuf {
    if input.is_dir() {
        return input.join(format!("abstract.{}", format.extension()));
    }
    input.with_extension(format.extension())
}

// ---------------------------------------------------------------------------
// bundle / unbundle
// ---------------------------------------------------------------------------

fn run_bundle(args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    let options = collect_options(&args);
    let key_material = flag_value(&args, "--key");
    let out = flag_value(&args, "--out").map(PathBuf::from);
    let plain = args.iter().any(|arg| arg == "--plain");
    let positional = positional_args(&args);
    let input = positional
        .first()
        .map(PathBuf::from)
        .ok_or("usage: abstract bundle <path> --key <passphrase|hex:...> [--out file.abx] [--plain]")?;

    if !plain && key_material.is_none() {
        return Err(
            "bundle requires --key <passphrase|hex:...> (or pass --plain for an unencrypted debug bundle)"
                .into(),
        );
    }

    let compiled = compile_project(&input, options)?;
    let payload = compiled.to_json_string();
    let key = match (&key_material, plain) {
        (_, true) => None,
        (Some(material), false) => Some(bundle::derive_key(material)?),
        (None, false) => unreachable!("checked above"),
    };
    let bytes = bundle::encode(payload.as_bytes(), key.as_ref());

    let output_path = out.unwrap_or_else(|| {
        if input.is_dir() {
            input.join("abstract.abx")
        } else {
            input.with_extension("abx")
        }
    });
    fs::write(&output_path, &bytes)?;
    eprintln!(
        "abstract: wrote {} ({} bytes, {} item(s), {})",
        output_path.display(),
        bytes.len(),
        compiled.items.len(),
        if plain { "plain" } else { "sealed" }
    );
    Ok(())
}

fn run_unbundle(args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    let key_material = flag_value(&args, "--key");
    let positional = positional_args(&args);
    let input = positional
        .first()
        .map(PathBuf::from)
        .ok_or("usage: abstract unbundle <file.abx> [--key <passphrase|hex:...>]")?;
    let bytes = fs::read(&input)?;
    let key = match key_material {
        Some(material) => Some(bundle::derive_key(&material)?),
        None => None,
    };
    let payload = bundle::decode(&bytes, key.as_ref())?;
    print!("{}", String::from_utf8_lossy(&payload));
    Ok(())
}

// ---------------------------------------------------------------------------
// init
// ---------------------------------------------------------------------------

fn init_project(target: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if target.exists() && fs::read_dir(target)?.next().is_some() {
        return Err(format!(
            "directory '{}' already exists and is not empty",
            target.display()
        )
        .into());
    }
    fs::create_dir_all(target.join("data/templates"))?;
    fs::create_dir_all(target.join("data/items"))?;
    fs::create_dir_all(target.join("assets"))?;

    fs::write(
        target.join("data/templates/Meta.abt"),
        r#"schema Meta {
    id: text(1..40)
    owner: text(1..60)
    version: int(1..999)
    description: text(1..160)
}
"#,
    )?;
    fs::write(
        target.join("data/templates/Item.abt"),
        r#"schema Item {
    id: text(1..40)
    name: text(1..80)
    rarity: enum(common, rare, epic, legendary) = common
    price: float(0..9999) = 0.0
    tradable: bool = true
    release_wave: int(1..99) @optional
    tags[]: enum(starter, seasonal, exclusive) @optional
    icon: image(png) @optional

    stats {
        power: int(0..100) = 0
        agility: int(0..100) = 0
    }
}

logic Item {
    if .rarity == "legendary" {
        require .price >= 100
            else throw "Legendary items must cost at least 100."
    }

    derive? .release_wave = 1
}
"#,
    )?;
    fs::write(
        target.join("pack.ab"),
        r#"Meta :: @id.my_pack, @owner.me, @version.1
    description: "A new Abstract project. Describe your pack here."
"#,
    )?;
    fs::write(
        target.join("data/items/starter_sword.ab"),
        r#"// Your first instance. Compile the project with:
//   abstract compile . JSON
Item :: @id.starter_sword, @rarity.common
    name: Starter Sword
    price: 9.5
    tags: starter
    stats.power: 12
"#,
    )?;
    fs::write(
        target.join("README.md"),
        r#"# Abstract Project

- `pack.ab` describes the pack itself.
- `data/templates/` holds the schemas (`.abt`).
- `data/items/` holds instances (`.ab`).
- `assets/` holds binary files referenced by `file(...)` and `image(...)` fields.

Compile everything:

```
abstract compile . JSON
```

Validate without output:

```
abstract lint .
```
"#,
    )?;
    println!(
        "abstract: created project at {} (compile it with 'abstract compile {} JSON')",
        target.display(),
        target.display()
    );
    Ok(())
}

fn looks_like_path(value: &str) -> bool {
    value.contains('\\')
        || value.contains('/')
        || value.ends_with(".ab")
        || value.ends_with(".abt")
        || std::path::Path::new(value).exists()
}

fn take_input(args: &[String]) -> Result<PathBuf, Box<dyn std::error::Error>> {
    args.iter()
        .find(|arg| {
            !arg.starts_with('-') && OutputFormat::parse(arg).is_none() && !is_bool(arg)
        })
        .map(PathBuf::from)
        .ok_or_else(|| "missing input path".into())
}

fn is_true(value: &str) -> bool {
    value.eq_ignore_ascii_case("true")
}

fn is_bool(value: &str) -> bool {
    value.eq_ignore_ascii_case("true") || value.eq_ignore_ascii_case("false")
}

fn print_help() {
    println!(
        "abstract {VERSION}\n\
\n\
USAGE:\n\
    abstract compile <path> [JSON|YML|RAW] [true] [--out <file>]\n\
    abstract <object.ab> [Template.abt ...] JSON|YML|RAW [true] [--out <file>]\n\
    abstract lint <path>\n\
    abstract templates <path>\n\
    abstract init <directory>\n\
    abstract bundle <path> --key <passphrase|hex:...> [--out <file.abx>] [--plain]\n\
    abstract unbundle <file.abx> [--key <passphrase|hex:...>]\n\
\n\
OPTIONS:\n\
    --skip-assets     Skip on-disk file/image checks (authoring without assets)\n\
    --allow-unknown   Accept instance fields not declared in the schema\n\
    --out <file>      Write output to a specific file\n\
    --key <material>  Bundle key: a passphrase, or hex:<64 hex digits>\n\
    --plain           Write an unencrypted bundle (debugging only)\n\
\n\
The compiler validates .ab instances against .abt schemas and emits JSON,\n\
YAML, or RAW data. 'bundle' seals compiled JSON into a tamper-evident .abx\n\
container for embedding in applications (see the Java runtime)."
    );
}
