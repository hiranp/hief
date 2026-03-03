# Java Framework Rules

> SDD conventions and best practices for Java projects using HIEF.
> Reference: https://google.github.io/styleguide/javaguide.html | https://openjdk.org/

## Code Style
- Follow Google Java Style Guide; enforce with `google-java-format` in CI
- Max line length: 120 characters
- Use 4-space indentation (never tabs)
- Organise imports: `java.*` → `javax.*` → third-party → project-local; no wildcards

## Modernity (Java 17+)
- Prefer `record` types for immutable data carriers over manual POJOs
- Use `sealed` classes + `instanceof` pattern matching for exhaustive variant handling
- Use `var` for local variables only when the type is obvious from the right-hand side
- Use `switch` expressions (not statements) with arrow syntax (`->`) for exhaustive switches
- Use text blocks (`"""..."""`) for multiline strings (SQL, JSON, templates)

## Nullability
- Annotate all public parameters and return types with `@NonNull` / `@Nullable` (JSR-305 or JSpecify)
- Prefer `Optional<T>` for methods that may return no value — never return `null` from a public API
- Validate constructor parameters with `Objects.requireNonNull` or use `@NonNull` + a null-checking framework (e.g. Lombok `@NonNull`)

## Error Handling
- Use checked exceptions for recoverable conditions the caller *must* handle
- Use unchecked exceptions (`RuntimeException`) for programming errors (bad state, contract violations)
- Never catch `Exception` or `Throwable` without re-throwing or logging at ERROR level
- Wrap third-party exceptions in domain exceptions to avoid leaking implementation details

## Collections & Streams
- Prefer immutable collections: `List.of(...)`, `Map.of(...)`, `Set.of(...)` for constants
- Use `Stream` API for data transformations; avoid imperative loops for non-trivial mappings
- Avoid `null` values in collections — use `Optional` or a sentinel value with documentation

## Concurrency
- Prefer `java.util.concurrent` (e.g. `CompletableFuture`, `ExecutorService`) over raw `Thread`
- Use structured concurrency (`StructuredTaskScope`, Java 21 preview) for parallel subtasks
- Annotate shared mutable fields with `@GuardedBy("lock")` and document the locking strategy
- Prefer `ConcurrentHashMap` over `Collections.synchronizedMap()`

## Dependency Management (Maven / Gradle)
- Use Gradle (Kotlin DSL) for new projects; version catalogs (`libs.versions.toml`) for dependency management
- Pin all dependency versions — no `+` or `latest.release` in production dependencies
- Use `dependencyCheck` plugin to scan for CVEs in CI

## Testing
- Use JUnit 5 (`@Test`, `@ParameterizedTest`, `@ExtendWith`) — not JUnit 4
- Use `AssertJ` for fluent assertions over bare `assertEquals`
- Use `Mockito` for mocking; `WireMock` for HTTP mocking
- Aim for ≥ 80% branch coverage on domain logic; use `JaCoCo` in CI
- Integration tests in a separate `src/integrationTest/` source set — do not mix with unit tests

## Documentation
- All `public` and `protected` members must have `/** Javadoc */` comments
- Include `@param`, `@return`, and `@throws` tags for non-trivial methods
- Use `{@code ...}` for inline code references in Javadoc
