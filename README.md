<div align="center">

# abstract

A small schema-backed language for defining validated data with a pleasant authoring experience.

`	ext
.abt schemas + .ab instances -> abstract compile -> canonical RAW data
`

</div>

<details>
<summary>Setup & details</summary>

## Why?

Large content-heavy systems often drift into fragile YAML, noisy JSON, or custom validation scripts scattered across build steps. Abstract gives that data a single authored shape: schemas describe the contract, instances declare records, and the compiler validates everything before emitting canonical output.

## Run locally

`ash
cargo test
cargo run -- compile example
cargo run -- lint example
`

## Project layout

- src/: Rust compiler core and CLI.
- 	ests/: compiler behavior tests.
- example/: generic Abstract project.
- docs/: language documentation and AI primer.
- editors/: editor integrations.
- site/: static documentation website.
- docs/ai/: agent-facing repository context.

## CLI examples

`ash
abstract path/to/object.ab path/to/Template.abt JSON
abstract path/to/object.ab path/to/Template.abt YML true
abstract lint path/to/project
abstract templates path/to/project
`

</details>

<div align="center">

## Stack

Rust, static HTML/CSS/JS docs, and VS Code editor support.

<p align="center">
  <a href="https://skillicons.dev">
    <img src="https://skillicons.dev/icons?i=rust,html,css,js,vscode&perline=5&theme=dark" />
  </a>
</p>

</div>
