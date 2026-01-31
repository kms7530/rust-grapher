# rust-grapher

Generate dependency and function-call graphs for Rust projects.

## Installation

Build locally or install from the repository:

```bash
# Build
cargo build --release

# Install locally
cargo install --path .
```

## Usage

Display command help:

```bash
rust-grapher --help
```

Examples:

- Dependency graph (Mermaid):

```bash
rust-grapher deps
rust-grapher deps --depth 2 -o deps.md
rust-grapher deps --workspace-only
```

- Function-call graph (Dot / Mermaid / JSON):

```bash
rust-grapher fn-graph
rust-grapher fn-graph --focus main --depth 3
rust-grapher fn-graph -f dot | dot -Tpng -o call-graph.png
```

## GitHub Actions

This repository includes `.github/workflows/release.yml`, a workflow that builds the project with `cargo build --release`, creates a GitHub Release, and — if the `CARGO_REGISTRY_TOKEN` secret is configured — attempts to publish the package to crates.io.

- `CARGO_REGISTRY_TOKEN` is a crates.io API token. Add it at: Repository → Settings → Secrets and variables → Actions → New repository secret.
- Before publishing, verify locally with:

```bash
cargo publish --dry-run
```

## License

This project is distributed under the MIT License. See the `license` field in `Cargo.toml` for details.

## Contributing

Contributions are welcome — please open an issue to discuss major changes before submitting a pull request.
