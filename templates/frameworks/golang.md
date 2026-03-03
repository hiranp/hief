# Go Framework Rules

> SDD conventions and best practices for Go projects using HIEF.
> Reference: https://google.github.io/styleguide/go/ | https://go.dev/doc/effective_go

## Code Style
- Run `gofmt` and `goimports` on every save — enforce in CI with `gofmt -l .`
- Use `golangci-lint` with at least `errcheck`, `staticcheck`, `govet`, and `unused` enabled
- Prefer short, lowercase names for local variables; longer descriptive names for package-level identifiers
- Group imports: stdlib → external → internal; separated by blank lines (enforced by `goimports`)

## Packages & Modules
- One concept per package; package name is singular and lowercase (e.g. `graph`, `index`, `docs`)
- Avoid `util`, `common`, `helpers` package names — name by what the package *does*
- Use `internal/` to prevent external packages from importing implementation details
- Keep `main.go` thin — parse flags, wire dependencies, call into domain packages

## Error Handling
- Always check errors — never assign to `_` unless you've explicitly reasoned that the error is safe to ignore (with a comment)
- Return errors as the last return value: `func Foo() (Result, error)`
- Wrap errors with context: `fmt.Errorf("doing X: %w", err)` (never use `errors.New` after nesting)
- Define sentinel errors with `var ErrNotFound = errors.New("not found")` for callers to inspect with `errors.Is`
- Define domain error types for structured inspection: `type ValidationError struct { Field string; Message string }`

## Interfaces
- Define interfaces at the point of use (the consuming package), not in the implementing package
- Keep interfaces small — prefer single-method interfaces (`io.Reader`, `io.Writer`)
- Accept interfaces, return concrete types (the "accept interfaces, return structs" principle)
- Use `//go:generate mockgen` or `moq` to generate mocks from interfaces

## Concurrency
- Use `context.Context` as the first parameter in all functions that do I/O or may block
- Cancel contexts promptly — always `defer cancel()` after `context.WithCancel` / `WithTimeout`
- Protect shared mutable state with `sync.Mutex` or `sync.RWMutex`; document which fields a mutex guards
- Prefer channels for coordination; prefer `sync` primitives for simple state protection
- Use `sync.WaitGroup` + `errgroup.Group` (golang.org/x/sync) for fan-out concurrency

## Testing
- Use `testing.T`; table-driven tests for variant coverage:
  ```go
  tests := []struct{ name string; input X; want Y }{ ... }
  for _, tc := range tests { t.Run(tc.name, func(t *testing.T) { ... }) }
  ```
- Use `t.TempDir()` for filesystem tests — cleaned up automatically
- Benchmark with `testing.B`; use `b.ResetTimer()` after expensive setup
- Mock I/O with interface substitution rather than monkey-patching
- Use `testcontainers-go` for integration tests requiring real databases

## Dependency Management
- Use Go modules (`go.mod` / `go.sum`); commit `go.sum`
- Pin direct dependencies to exact minor versions; let patch versions float
- Audit dependencies with `govulncheck` in CI
- Prefer the standard library over third-party packages for non-trivial functionality

## Documentation
- All exported identifiers must have a doc comment beginning with the name: `// Foo does ...`
- Package-level docs in `doc.go` for complex packages
- Use `go doc` output as a quality check before merging
