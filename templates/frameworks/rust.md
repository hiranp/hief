# Rust Framework Rules

> SDD conventions and best practices for Rust projects using HIEF.
> Reference: https://doc.rust-lang.org/stable/book/ | https://rust-lang.github.io/api-guidelines/

## Error Handling
- Use `thiserror` for library error types; `anyhow` for application-layer error propagation
- Never use `.unwrap()` in production paths — use `?`, `.expect("invariant: <reason>")`, or explicit match arms
- All custom error types must implement `std::error::Error` and carry enough context to be actionable
- Prefer `Result<(), E>` over `panic!()` for recoverable errors
- Use `tracing::instrument` to automatically capture function arguments and error context in logs
- Use `color-eyre` for user-facing CLI error reports with rich backtraces

## Code Structure
- Organize modules in `src/` by domain, not by type (`graph/`, `index/`, `docs/` — not `models/`, `utils/`)
- Keep `lib.rs` / `mod.rs` as pure re-exports; avoid business logic at the crate root
- Use `pub(crate)` liberally — only expose at `pub` what's part of the stable API surface
- Group imports: `std` first, then external crates, then `crate::` — separated by blank lines
- For CLI tools, use `clap` (v4+) with the `derive` feature for type-safe argument parsing
- In Cargo workspaces, use `[workspace.dependencies]` to centralize version pins across crates

## Logging & Observability
- Use the `tracing` crate for all logging — avoid `println!` or the `log` crate directly
- Level strategy: `error!` for actionable failures, `warn!` for unexpected but recoverable states, `info!` for major milestones, `debug!` for flow tracing
- Use structured fields: `info!(user_id = %user.id, "User logged in")` rather than string formatting
- Use `tracing-subscriber` with `EnvFilter` to allow log-level control via `RUST_LOG`

## Memory & Ownership
- Prefer borrowing (`&str`, `&[T]`) over owning (`String`, `Vec<T>`) in function parameters
- Avoid `clone()` in hot paths — profile first with `cargo flamegraph`, optimise second
- Use `Arc<T>` for shared state across async tasks; `Rc<T>` is `!Send` and unsafe across threads
- Zero-copy deserialization with `serde` (`&'de str`) where performance matters

## Async
- Async runtime: `tokio` only — never `async-std` or mixing runtimes
- Mark functions `async` only when they actually `await` something
- Use `tokio::spawn` sparingly; prefer structured concurrency with `JoinSet` or `FuturesUnordered`
- Avoid holding non-`Send` types (e.g. `MutexGuard`) across `.await` points
- Use `tokio::time::timeout` to bound all external I/O calls

## Safety
- No `unsafe` blocks without a `// SAFETY:` comment explaining why the invariant holds
- Prefer safe abstractions (`Vec`, `slice`, `str`) over raw pointer arithmetic
- Pin `unsafe` surface to the smallest possible scope

## Tooling & CI
- Run `cargo clippy -- -D warnings` in CI to enforce lint cleanliness
- Use `cargo-nextest` for faster, parallel test execution and improved test output
- Use `cargo-deny` to audit dependencies for known vulnerabilities, license issues, and duplicates
- Format with `rustfmt` using `edition = "2021"` (or later) in `rustfmt.toml`
- Use `cargo doc --no-deps` to verify docs compile cleanly without broken intra-doc links

## Testing
- Unit tests live in `mod tests { ... }` within the same file as the code under test
- Integration tests live in `tests/` and test public API only
- Use `tempfile::tempdir()` for filesystem tests — never write to the project root in tests
- Prefer `assert_eq!` with descriptive messages over bare `assert!`
- Use `proptest` or `quickcheck` for property-based testing of complex logic
- Use `mockall` or trait-based mocking to isolate external dependencies in unit tests

## Documentation
- All `pub` items must have `///` doc comments
- Include `# Examples` sections for non-trivial public functions
- Use `#[doc = include_str!("../README.md")]` to keep crate docs in sync with README
- Run `cargo test --doc` to verify all doc examples compile and pass
