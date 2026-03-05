# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.5] - 2026-03-05

### Added
- Comprehensive `CONTRIBUTING.md` and `SECURITY.md`.
- `CODE_OF_CONDUCT.md`.
- GitHub Actions CI workflow.
- Issue and Pull Request templates.
- Cognitive memory layer with access tracking and activation-weighted search.
- Vector search capabilities.

### Changed
- Updated `Cargo.toml` with repository metadata.
- Improved version 0.2.5 documentation.
- Enhanced project conventions with structural checks.

### Fixed
- Health-check command to validate golden files.

## [0.2.3] - 2026-03-03

### Added
- `hief upgrade` command for local binary upgrades.
- Literal substring search in scoring engine.
- Golden set specification for project invariants.

### Changed
- Updated documentation templates and enforced mandatory workflow rules.

## [0.2.0] - 2026-02-28

### Added
- Initial public-ready release of HIEF.
- AST-aware indexing for Rust, Python, and TypeScript.
- MCP server implementation with stdio and HTTP transports.
- Intent dependency graph for task coordination.
- Golden set quality evaluation engine.
- VS Code extension companion.
