use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use abstract_lang::{
    compile_paths, compile_project, known_template_names, lint_project, CompileOptions,
    CompiledProject,
};

#[derive(Clone, Copy)]
enum OutputFormat {
    Json,
    Yml,
}

impl OutputFormat {
    fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "json" => Some(Self::Json),
            "yml" | "yaml" => Some(Self::Yml),
            _ => None,
        }
    }

    fn extension(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Yml => "yml",
        }
    }

    fn render(self, compiled: &CompiledProject) -> String {
        match self {
            Self::Json => compiled.to_json_string(),
            Self::Yml => compiled.to_yaml_string(),
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
    if args.is_empty() || args[0] == "--help" || args[0] == "-h" {
        print_help();
        return Ok(());
    }
    if args[0] == "--version" || args[0] == "-V" {
        println!("abstract 0.1.0");
        return Ok(());
    }

    let command = args.remove(0);
    match command.as_str() {
        "compile" => {
            let request = parse_compile_request(args)?;
            let compiled = compile_project(&request.input, CompileOptions::default())?;
            emit_compiled(compiled, request)?;
        }
        "lint" => {
            let input = take_input(&args)?;
            lint_project(&input)?;
            println!("abstract: ok");
        }
        "templates" => {
            let input = take_input(&args)?;
            for name in known_template_names(&input)? {
                println!("{name}");
            }
        }
        other => {
            if looks_like_path(other) {
                let mut request = parse_direct_request(other, args)?;
                validate_input_paths(&request.paths)?;
                let compiled = compile_paths(&request.paths, CompileOptions::default())?;
                request.input = request.paths[0].clone();
                emit_compiled(compiled, request)?;
            } else {
                return Err(format!("unknown command '{other}'").into());
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
}

fn parse_compile_request(args: Vec<String>) -> Result<CompileRequest, Box<dyn std::error::Error>> {
    let input = take_input(&args)?;
    let format = args
        .iter()
        .find_map(|arg| OutputFormat::parse(arg))
        .unwrap_or(OutputFormat::Json);
    let write_file = args.iter().any(|arg| is_true(arg));
    Ok(CompileRequest {
        input: input.clone(),
        paths: vec![input],
        format,
        write_file,
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

fn parse_direct_request(
    first: &str,
    args: Vec<String>,
) -> Result<CompileRequest, Box<dyn std::error::Error>> {
    let format_index = args
        .iter()
        .position(|arg| OutputFormat::parse(arg).is_some())
        .ok_or("missing output format: expected JSON or YML")?;
    let format = OutputFormat::parse(&args[format_index]).expect("format was checked above");
    let write_file = args
        .get(format_index + 1)
        .map(|arg| is_true(arg))
        .unwrap_or(false);

    let mut paths = vec![PathBuf::from(first)];
    paths.extend(args[..format_index].iter().map(PathBuf::from));
    if paths.is_empty() {
        return Err("missing input path".into());
    }

    Ok(CompileRequest {
        input: paths[0].clone(),
        paths,
        format,
        write_file,
    })
}

fn emit_compiled(
    compiled: CompiledProject,
    request: CompileRequest,
) -> Result<(), Box<dyn std::error::Error>> {
    let output = request.format.render(&compiled);
    print!("{output}");
    if request.write_file {
        let output_path = default_output_path(&request.input, request.format);
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

fn looks_like_path(value: &str) -> bool {
    value.contains('\\')
        || value.contains('/')
        || value.ends_with(".ab")
        || value.ends_with(".abt")
        || std::path::Path::new(value).exists()
}

fn take_input(args: &[String]) -> Result<PathBuf, Box<dyn std::error::Error>> {
    args.iter()
        .find(|arg| !arg.starts_with('-') && OutputFormat::parse(arg).is_none() && !is_bool(arg))
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
        "abstract 0.1.0\n\nUSAGE:\n    abstract <object.ab> <Template.abt> JSON|YML [true|false]\n    abstract compile <path> [JSON|YML] [true|false]\n    abstract lint <path>\n    abstract templates <path>\n\nThe compiler validates .ab instances against .abt schemas and emits JSON or YAML.\nWhen the final boolean is true, it also writes a sibling .json or .yml file."
    );
}
