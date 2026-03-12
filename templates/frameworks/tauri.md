# Tauri Framework Rules

> SDD conventions and best practices for Tauri (Rust + TypeScript/JS) desktop apps.
> Reference: https://tauri.app/ | https://v2.tauri.app/

## Architecture & Boundaries
- **Tauri v2:** Favor Tauri v2 features (multi-window, mobile support, scoped capabilities, IPC v2)
- Clearly separate Frontend (UI/State) and Backend (I/O/Computation/System APIs)
- Use the **Command** pattern for all cross-boundary communication (`#[tauri::command]`)
- Use `tauri-specta` + `specta` to automatically generate TypeScript types from Rust command signatures — eliminates manual type duplication and drift

## Tauri Commands
- Return `Result<T, String>` (or a custom serializable error type) from all commands to handle failures gracefully on the frontend
- Validate all data on the Rust side even if already validated in the frontend — the Rust layer is the trust boundary
- Keep commands thin; delegate heavy logic to domain modules that are independently testable
- Use `tokio::spawn` or `tauri::async_runtime::spawn` for long-running tasks; emit progress via Tauri Events

## Events & State
- Use `tauri::State<T>` for shared backend resources (DB pools, configurations, file handles)
- Use Tauri **Events** (`emit`, `listen`) for pushing data from Rust to UI (e.g., progress, notifications, system changes)
- Prefer Events over polling for real-time updates — they are efficient and cancellation-safe
- Version your event payloads; add a `version: u32` field to avoid breaking changes as the app evolves

## Plugin Ecosystem
- Prefer official **Tauri plugins** (`tauri-plugin-fs`, `tauri-plugin-shell`, `tauri-plugin-store`, `tauri-plugin-updater`) over custom reimplementations
- Pin plugin versions in `Cargo.toml` and audit changes on upgrade — plugins run with full native access
- For custom plugins, follow the `tauri-plugin-*` generator pattern for consistent initialization and permission declarations

## Security
- Define fine-grained **Capabilities** (`src-tauri/capabilities/*.json`) — grant only the permissions each window actually needs
- Disable remote URL access (`dangerousRemoteHttpAccess`) unless explicitly required and audited
- Use **Scoped Filesystem Access** to restrict where the app can read/write data
- Sanitize and validate all paths received from the frontend to prevent path traversal attacks
- Run `cargo audit` and `npm audit` as part of your CI pipeline

## Testing
- Unit test Rust commands in isolation by extracting logic into pure functions testable without a Tauri runtime
- Use `tauri::test::mock_builder()` for lightweight integration tests of command handlers
- Use `tauri-action` (GitHub Action) for automated multi-platform builds in CI
- Use `WebdriverIO` + `tauri-driver` or `Playwright` for E2E testing of the fully integrated app

## Build & Distribution
- Sign all binaries: use Apple Developer ID (macOS) and Authenticode (Windows) before distribution
- Use the built-in **`tauri-plugin-updater`** for seamless, cryptographically verified app updates
- Use `tauri bundle` targets per platform; test installers on clean VMs before release
- Embed app version from `Cargo.toml` into the UI using `TAURI_ENV_APP_VERSION` at build time
