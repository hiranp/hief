# Rust Framework Rules

> SDD conventions and best practices for Rust projects using HIEF.
> Reference: https://doc.rust-lang.org/stable/book/ | https://rust-lang.github.io/api-guidelines/

## Error Handling
- Use `thiserror` for library error types; `anyhow` for application-layer error propagation
- Never use `.unwrap()` in production paths — use `?`, `.expect("context")`, or explicit match arms
- All custom error types must implement `std::error::Error` and carry enough context to be actionable
- Prefer `Result<(), E>` over `panic!()` for recoverable errors

## Code Structure
- Organize modules in `src/` by domain, not by type (`graph/`, `index/`, `docs/` — not `models/`, `utils/`)
- Keep `lib.rs` / `mod.rs` as pure re-exports; avoid business logic at the crate root
- Use `pub(crate)` liberally — only expose at `pub` what's part of the stable API surface
- Group imports: `std` first, then external crates, then `crate::` — separated by blank lines
- Prefer `impl Trait` in function signatures over explicit lifetime annotations where possible

## Memory & Ownership
- Prefer borrowing (`&str`, `&[T]`) over owning (`String`, `Vec<T>`) in function parameters
- Avoid `clone()` in hot paths — profile first, optimise second
- Use `Arc<T>` for shared state across async tasks; `Rc<T>` is `!Send` and unsafe across threads
- Zero-copy deserialization with `serde` (`&'de str`) where performance matters

## Async
- Async runtime: `tokio` only — never `async-std` or mixing runtimes
- Mark functions `async` only when they actually `await` something
- Use `tokio::spawn` sparingly; prefer structured concurrency with `JoinSet` or `FuturesUnordered`
- Avoid holding non-`Send` types (e.g. `MutexGuard`) across `.await` points

## Safety
- No `unsafe` blocks without a `// SAFETY:` comment explaining why the invariant holds
- Prefer safe abstractions (`Vec`, `slice`, `str`) over raw pointer arithmetic
- Pin `unsafe` surface to the smallest possible scope

## Testing
- Unit tests live in `mod tests { ... }` within the same file as the code under test
- Integration tests live in `tests/` and test public API only
- Use `tempfile::tempdir()` for filesystem tests — never write to the project root in tests
- Prefer `assert_eq!` with descriptive messages over bare `assert!`

## Documentation
- All `pub` items must have `///` doc comments
- Include `# Examples` sections for non-trivial public functions
- Use `#[doc = include_str!("../README.md")]` to keep crate docs in sync with README
