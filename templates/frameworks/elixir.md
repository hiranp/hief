# Elixir Framework Rules

> SDD conventions and best practices for Elixir / Phoenix projects using HIEF.
> Reference: https://elixir-lang.org/ | https://www.phoenixframework.org/ | https://hexdocs.pm/

## Project Structure
- Follow Mix project conventions: `lib/`, `test/`, `config/`, `priv/`
- Separate the **boundary** (web, CLI, jobs) from the **core** (business logic, domain): keep Phoenix in a thin `MyAppWeb` context; business logic in `MyApp.*` contexts
- Use **Contexts** to define explicit, documented public APIs between domains — access across contexts only through the context module's public functions
- Name contexts after business domains: `Accounts`, `Billing`, `Notifications`

## OTP & Supervision
- Model long-lived processes as `GenServer`s; use `Agent` for simple state holders
- Organize processes in a supervision tree with appropriate restart strategies (`one_for_one`, `rest_for_one`)
- Use `Task.Supervisor` for supervised, fire-and-forget concurrent work
- Use `Registry` for named process registration and `via_tuple` patterns for dynamic process lookup
- Use **`Oban`** for reliable background jobs with persistence, scheduling, and retry policies

## Phoenix Framework
- Use **Phoenix LiveView** for real-time, interactive UI without writing JavaScript — maintain state in LiveView processes, not the browser
- Use **Components** (`Phoenix.Component`) for reusable, stateless HTML fragments; use `attr` and `slot` macros for typed, documented props
- Use **Contexts** as the data layer interface; never call `Repo` directly from controllers or LiveViews
- Use `Phoenix.Channels` for WebSocket communication when LiveView is not a fit (e.g., bidirectional binary data)
- Use `Plug` for composable HTTP middleware; place shared plugs in `MyAppWeb.Router` pipelines

## Ecto & Database
- Define **Changesets** for every data mutation — they encode validation, transformation, and constraints
- Use `Repo.transaction/1` for multi-step writes that must succeed or fail atomically
- Use Ecto's **`multi`** (`Ecto.Multi`) for composable, auditable transaction pipelines
- Write raw SQL via `Ecto.Adapters.SQL.query!/3` for complex queries that Ecto's query DSL can't express well
- Run `mix ecto.migrate` and `mix ecto.rollback` — never modify the database schema directly in production

## Error Handling
- Embrace the **"let it crash"** philosophy — let unexpected errors propagate and be restarted by supervisors
- Use `{:ok, result}` / `{:error, reason}` tuples for expected failure paths; `with` chains for multi-step pipelines
- Use `Logger` (backed by Erlang's `:logger`) for structured logging with metadata

## Logging & Observability
- Attach metadata with `Logger.metadata/1` at request boundaries for correlation across log lines
- Use **`Telemetry`** events for instrumentation; attach handlers with `Telemetry.attach/4`
- Use **`OpentelemetryExporter`** or **`Datadog`** APM for distributed tracing in production

## Testing
- Use **ExUnit** for all testing; organize tests to mirror the `lib/` directory structure
- Use `Ecto.Adapters.SQL.Sandbox` for DB tests — each test runs in a rolled-back transaction for isolation
- Use **`Mox`** for behaviour-based mocking; define behaviours in your context boundary (`@callback`)
- Use **`Wallaby`** or **`Playwright`** (via `playwright_elixir`) for browser-based E2E testing of LiveView UIs

## Tooling
- Format with `mix format` on every save; enforce in CI with `mix format --check-formatted`
- Use **`Dialyxir`** (`mix dialyzer`) for static type analysis with Erlang's success typings system
- Use **`Credo`** for consistent code style linting
- Use **`ex_doc`** for documentation; write `@moduledoc` and `@doc` for all public modules and functions
- Use **`mix release`** for self-contained, reproducible production deployments; avoid `mix run` in production
