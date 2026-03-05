# Contributing to HIEF

Thank you for your interest in contributing to HIEF! This guide covers
everything you need to know to get started.

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Getting Started](#getting-started)
- [Development Setup](#development-setup)
- [Development Workflow](#development-workflow)
- [Pull Request Process](#pull-request-process)
- [Coding Standards](#coding-standards)
- [Architecture Overview](#architecture-overview)
- [Reporting Bugs](#reporting-bugs)
- [Suggesting Features](#suggesting-features)

## Code of Conduct

This project follows the [Contributor Covenant Code of Conduct](CODE_OF_CONDUCT.md).
By participating, you are expected to uphold this code.

## Getting Started

1. **Fork** the repository on GitHub
2. **Clone** your fork locally:
   ```sh
   git clone https://github.com/<your-username>/hief.git
   cd hief
   ```
3. **Add upstream** remote:
   ```sh
   git remote add upstream https://github.com/hiranp/hief.git
   ```
4. **Build** the project:
   ```sh
   cargo build
   ```
5. **Run tests** to verify everything works:
   ```sh
   cargo test
   ```

## Development Setup

### Prerequisites

- **Rust** stable toolchain (MSRV 1.85+) — install via [rustup](https://rustup.rs/)
- **Git** (for blame integration and hooks)
- **just** (optional, for [Justfile](Justfile) task runner)

### Building

```sh
cargo build                 # Debug build
cargo build --release       # Release build (optimized)
```

### Running Tests

```sh
cargo test                  # All tests
cargo test test_config      # Specific test
```

### Linting & Formatting

```sh
cargo fmt -- --check        # Check formatting
cargo fmt                   # Auto-format
cargo clippy --all-targets  # Run clippy lints
```

### Using the Justfile

If you have [just](https://github.com/casey/just) installed:

```sh
just build          # cargo build --release
just test           # cargo test
just lint           # cargo clippy
just fmt            # cargo fmt --check
just install        # Build + symlink to ~/bin/hief
```

## Development Workflow

### 1. Create a Branch

```sh
git checkout -b feature/your-feature-name
# or
git checkout -b fix/your-bug-fix
```

Use descriptive branch names with prefixes: `feature/`, `fix/`, `docs/`, `refactor/`.

### 2. Use Intents (for non-trivial changes)

For anything beyond typo fixes or single-line changes, create an intent to
track your work:

```sh
hief graph create --kind feature --title "Add your feature"
hief graph update --id <intent-id> --status in_progress
```

This helps coordinate work when multiple contributors are active.

### 3. Make Your Changes

- Follow the [coding standards](#coding-standards) below
- Write tests for new functionality
- Update documentation if behavior changes
- Keep commits focused and atomic

### 4. Validate Before Submitting

```sh
cargo fmt -- --check        # Formatting
cargo clippy --all-targets  # Lints
cargo test                  # Tests
hief eval run               # Golden set evaluation (if applicable)
```

### 5. Submit a Pull Request

Push your branch and open a PR against `main`. See the
[PR process](#pull-request-process) below.

## Pull Request Process

1. **Fill out the PR template** — describe what changed and why
2. **Link related issues** — reference any issues this addresses
3. **Ensure CI passes** — all checks must be green
4. **Request review** — a maintainer will review your changes
5. **Address feedback** — make requested changes in new commits
6. **Squash and merge** — maintainers will handle the final merge

### PR Checklist

- [ ] Code compiles without warnings (`cargo build`)
- [ ] All tests pass (`cargo test`)
- [ ] Code is formatted (`cargo fmt -- --check`)
- [ ] Clippy passes (`cargo clippy --all-targets`)
- [ ] Documentation is updated (if applicable)
- [ ] CHANGELOG.md is updated (for user-facing changes)
- [ ] Intent is created (for non-trivial changes)

## Coding Standards

### Rust Style

- Follow standard Rust idioms and the [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- Use `rustfmt` for formatting (project config in `.rustfmt.toml`)
- Use `clippy` for linting (project config in `Clippy.toml`)

### Documentation

- **All public functions** must have doc comments (`///`)
- Use `//!` module-level docs for each module
- Include examples in doc comments where helpful

### Error Handling

- Use `thiserror` for error types — all errors implement `std::error::Error`
- **No `.unwrap()`** in production code paths — use `?` or explicit error handling
- No `unsafe` without documented safety invariants

### Naming

- Types: `PascalCase`
- Functions/methods: `snake_case`
- Constants: `SCREAMING_SNAKE_CASE`
- Module files: `snake_case.rs`

### Dependencies

- Minimize new dependencies — HIEF aims for ~15 core dependencies
- Async runtime: **tokio only** (no mixing runtimes)
- Prefer well-maintained, widely-used crates

### Testing

- Unit tests go in the same file as the code (`#[cfg(test)]` module)
- Integration tests go in the `tests/` directory
- Use `tempfile` for tests that need filesystem access

## Architecture Overview

HIEF has three core subsystems:

```
src/
├── index/    # Code indexing + search (tree-sitter, FTS5, ast-grep)
├── graph/    # Intent dependency graph (DAG with status workflow)
├── eval/     # Golden set evaluation engine (scoring + regression)
├── mcp/      # MCP server (tools + resources via rmcp)
├── cli/      # CLI commands (clap)
└── docs/     # Document scaffolding (MiniJinja templates)
```

For full details, see:
- [Architecture Summary](docs/architecture-summary.md) — quick overview
- [Architecture](docs/architecture.md) — full technical details
- [Agent Protocol](docs/agent-protocol.md) — how agents interact with HIEF

## Reporting Bugs

Open a [bug report issue](https://github.com/hiranp/hief/issues/new?template=bug_report.md)
with:

1. **Description** — what happened vs. what you expected
2. **Steps to reproduce** — minimal reproduction steps
3. **Environment** — OS, Rust version, HIEF version (`hief version`)
4. **Logs** — relevant output with `RUST_LOG=debug`

## Suggesting Features

Open a [feature request issue](https://github.com/hiranp/hief/issues/new?template=feature_request.md)
with:

1. **Problem** — what problem does this solve?
2. **Proposed solution** — how would this work?
3. **Alternatives** — what else did you consider?

## Questions?

- Open a [Discussion](https://github.com/hiranp/hief/discussions) on GitHub
- Check existing [issues](https://github.com/hiranp/hief/issues) and [docs](docs/)

---

Thank you for helping make HIEF better! 🎉
