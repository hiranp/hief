# Tauri Framework Rules

> SDD conventions and best practices for Tauri (Rust + TypeScript/JS) desktop apps.
> Reference: https://tauri.app/v2/guide/ | https://tauri.app/v2/reference/

## Architecture Boundary
- **Frontend** (`src/` or `ui/`): UI rendering, user events, display state — no business logic
- **Backend** (`src-tauri/`): File I/O, database, system calls, heavy computation — no DOM concerns
- All cross-boundary calls must go through typed Tauri `invoke` commands — never use `eval()` or bypass the IPC layer

## Tauri Commands
- Define commands in `src-tauri/src/commands/` — one file per domain area
- All commands must return `Result<T, String>` (or a custom serialisable error type)
- Use `#[tauri::command]` on public functions only; mark helpers `pub(crate)` or private
- Register all commands in `tauri::Builder::invoke_handler(tauri::generate_handler![...])`
- Validate all inputs server-side even if validated on the frontend — never trust IPC payloads

## State Management
- Use `tauri::State<T>` for shared app state (e.g. DB connection pools, config)
- Wrap mutable shared state in `Arc<Mutex<T>>` or `Arc<RwLock<T>>`; prefer `RwLock` for read-heavy state
- Emit events to the frontend with `AppHandle::emit_all` rather than return values for long-running operations
- Never store large blobs (images, binaries) in Tauri state — write to disk and pass paths

## Frontend (TypeScript/Svelte/Vue/React)
- Use the official `@tauri-apps/api` package for all Tauri API calls
- Avoid direct `window.__TAURI__` access — use typed wrappers from `@tauri-apps/api`
- Keep frontend state management minimal; Tauri backend is the source of truth for persisted data
- Use `vite` for bundling; do not eject or customise the build pipeline without documenting why

## Security
- Set `withWebview` + `devtools` only in `#[cfg(debug_assertions)]`
- Define a strict `allowlist` / `permissions` in `tauri.conf.json` — deny by default
- Sanitise all paths received from the frontend before filesystem operations
- Use `tauri::path::AppLocalDataDir` for user data, never hardcode paths

## Testing
- Unit test Rust commands with `#[tokio::test]` and mock external I/O with trait objects
- Integration test the full IPC surface using `tauri::test::mock_builder()` (Tauri v2)
- E2E tests with `WebdriverIO` + `tauri-driver` for critical user flows (launch, load, save)
- Run `cargo clippy --all-targets` and `cargo audit` in CI

## Build & Distribution
- Version follows SemVer — update `src-tauri/Cargo.toml` and `package.json` in lockstep
- Code-sign releases for macOS and Windows before publishing
- Use Tauri's built-in updater (`tauri-plugin-updater`) rather than rolling a custom update mechanism
