#!/usr/bin/env sh
# Installs the Abstract CLI on Linux and macOS.
#
# From a repository checkout (builds with cargo):
#   sh scripts/install.sh
#
# The binary is placed in ~/.local/bin (created if needed).

set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
project_dir=$(dirname -- "$script_dir")
install_dir="${HOME}/.local/bin"
target="${install_dir}/abstract"

if [ ! -f "${project_dir}/Cargo.toml" ]; then
    echo "error: run this script from the repository: sh scripts/install.sh" >&2
    exit 1
fi

if ! command -v cargo >/dev/null 2>&1; then
    echo "error: cargo was not found. Install Rust from https://rustup.rs" >&2
    exit 1
fi

echo "Building abstract (release)..."
(cd "$project_dir" && cargo build --release)

mkdir -p "$install_dir"
cp "${project_dir}/target/release/abstract" "$target"
chmod +x "$target"
echo "Installed to $target"

case ":${PATH}:" in
    *":${install_dir}:"*)
        echo "Run: abstract --version"
        ;;
    *)
        echo "Note: ${install_dir} is not on your PATH."
        echo "Add this line to your shell profile:"
        echo "  export PATH=\"\$HOME/.local/bin:\$PATH\""
        ;;
esac
