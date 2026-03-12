# Java Framework Rules

> SDD conventions and best practices for Java projects using HIEF.
> Reference: https://google.github.io/styleguide/javaguide.html | https://openjdk.org/

## Code Style & Tooling
- Follow Google Java Style Guide; enforce with `google-java-format` or `palantir-java-format`
- Max line length: 120 characters; 4-space indentation
- Use `Lombok` for reducing boilerplate in legacy POJOs, but prefer **Records** for new data carriers
- Organize imports logically; no wildcards allowed
- Run `checkstyle` in CI against your style config to catch deviations early

## Modernity (Java 17+)
- Use **Records** for immutable data transfer objects (DTOs)
- Use **Sealed Classes** and **Pattern Matching** (`instanceof`, `switch`) to ensure exhaustiveness
- Use **Text Blocks** (`"""..."""`) for multiline SQL, JSON, or HTML templates
- Use **`var`** only when the type is clearly visible from the assignment (e.g., `var list = new ArrayList<String>()`)
- **GraalVM Native Image:** Build with `native-image` via the GraalVM toolchain to produce fast-start,
  low-memory native binaries — ideal for CLI tools and serverless functions

## Framework Patterns (Spring Boot / Quarkus)
- Use constructor injection instead of field `@Autowired` for better testability and immutability
- Apply `@Validated` (from `jakarta.validation`) at controller/service boundaries for declarative constraint enforcement
- Keep controllers thin; delegate business logic to `@Service` components
- Use **Spring Boot Actuator** for health, metrics, and info endpoints in production services
- For Jakarta EE / MicroProfile stacks, prefer Quarkus or OpenLiberty for cloud-native deployment

## Error Handling
- Use custom exception hierarchies extending `RuntimeException` for most domain errors
- Use `@ControllerAdvice` (Spring) or `ExceptionMapper` (Jakarta) for centralized, consistent API error responses
- Never swallow exceptions; always log with enough context to reproduce the issue
- Use `Optional<T>` for values that may be absent — do not return `null` from public methods

## Concurrency
- Use **Structured Concurrency** (JEP 428 preview, Java 21) and **Virtual Threads** (`Executors.newVirtualThreadPerTaskExecutor()`) for high-throughput I/O
- Avoid raw thread management; use `CompletableFuture` for async pipelines or reactive (Project Reactor) when needed
- Use `synchronized` sparingly; prefer `java.util.concurrent` classes for shared state

## Testing
- Use **JUnit 5** and **AssertJ** for expressive, readable tests
- Use **Testcontainers** for integration testing with real databases or message brokers
- Use **Mockito** for mocking dependencies in unit tests; prefer `@InjectMocks` with constructor injection
- Run tests with a dedicated profile (e.g., `test`) to avoid polluting development data
- Aim for ≥ 80% line coverage measured by JaCoCo; enforce via CI build gate
