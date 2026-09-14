//! The `abstract` binary. All argument handling lives in [`cli`]; the project
//! scaffold `abstract init` writes lives in [`scaffold`].

#![deny(warnings)]
#![forbid(unsafe_code)]

mod cli;
mod scaffold;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(cli::run(&args));
}
