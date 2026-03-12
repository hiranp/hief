# Go Framework Rules

> SDD conventions and best practices for Go projects using HIEF.
> Reference: https://google.github.io/styleguide/go/ | https://go.dev/doc/effective_go

## Code Style & Tooling
- Run `gofmt` and `goimports` on every save; enforce in CI with `gofmt -l .` failing on diff
- Use `golangci-lint` with a comprehensive config (enable `revive`, `gocritic`, `errcheck`, `exhaustruct`)
- Group imports: stdlib → external → internal (separated by blank lines)
- Use `go work` for multi-module workspace management if applicable

## Packages & Project Structure
- Follow the "Standard Go Project Layout" (`/cmd`, `/internal`, `/pkg`)
- One concept per package; name should be singular, lowercase, and descriptive
- Avoid `util` or `common` packages — instead, name by what the package *provides*
- Keep `main.go` minimal — focus on wiring dependencies and handling OS signals with `os/signal`

## Error Handling
- Always check returned errors; wrap with context: `fmt.Errorf("reading config: %w", err)`
- Define sentinel errors with `errors.New` and custom error types for structured data
- Use `errors.Is` and `errors.As` for error inspection — never compare error strings
- Return errors as the last result: `func DoWork() (Value, error)`

## Concurrency
- **Context:** Pass `context.Context` as the first argument to all blocking or I/O calls
- Use `errgroup.Group` from `golang.org/x/sync/errgroup` for managing multiple goroutines
- Protect shared state with `sync.Mutex` or `sync.RWMutex`; document what the mutex guards
- Prefer channels for communication; mutexes for simple state protection
- Never leak goroutines — ensure they exit when the context is cancelled
- Use `sync.Once` for lazy initialization of singletons

## Logging & Observability
- Use `log/slog` (Go 1.21+) for structured, leveled logging
- Use `slog.Info`, `slog.Error`, etc., with key-value pairs for context
- Register a global logger at startup; use `slog.With(...)` to create child loggers with shared attributes
- Export OpenTelemetry traces and metrics using the `go.opentelemetry.io/otel` SDK

## Database & API
- Use `pgx` (v5) for PostgreSQL — it outperforms `database/sql` with native Postgres types
- Use `sqlc` to generate type-safe Go from SQL queries — keeps queries as source of truth
- For REST APIs, use `chi` or `gin` for routing; document with OpenAPI/Swagger via `swaggo`
- For gRPC and Protobuf, use `buf` for schema management, linting, and code generation

## Testing
- Use `testing.T` and table-driven tests for broad coverage
- Use `t.TempDir()` for all filesystem-related tests
- Use `testcontainers-go` for spinning up real DBs/services in integration tests
- Mock external dependencies using interfaces and generated mocks (e.g., `mockgen`, `moq`)
- Run `go test -race ./...` in CI to catch concurrency bugs early
- Use `go test -fuzz` for fuzz testing parsing and decoding logic (Go 1.18+)

## Dependency Management
- Use Go modules; always commit `go.sum`
- Run `go mod tidy` to clean up unused dependencies
- Audit dependencies with `govulncheck` to identify security vulnerabilities
- Favor the standard library over third-party packages whenever feasible
