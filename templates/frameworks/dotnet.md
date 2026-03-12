# .NET Framework Rules

> SDD conventions and best practices for C# / .NET 8+ projects using HIEF.
> Reference: https://learn.microsoft.com/en-us/dotnet/ | https://learn.microsoft.com/en-us/dotnet/csharp/

## Language & Modernity (C# 12+ / .NET 8+)
- Enable **Nullable Reference Types** (`<Nullable>enable</Nullable>`) in all projects — treat warnings as errors
- Use **Primary Constructors** (C# 12) for concise service and record declarations
- Use **Records** (`record`, `record struct`) for immutable value objects and DTOs
- Use **Collection Expressions** (`[1, 2, 3]`) for concise collection initialization
- Use **Pattern Matching** (`switch` expressions, `is`, `when`) for exhaustive branching over discriminated unions
- Use **Required Members** (`required`) to enforce initialization at the call site

## Project Structure
- Follow the **Vertical Slice Architecture**: organize by feature (`Features/Orders/`, `Features/Users/`) rather than by type (`Controllers/`, `Services/`)
- Each slice owns its request, handler, validator, and response — keeps related code co-located
- Use a shared `Common/` or `Infrastructure/` layer only for cross-cutting concerns (logging, persistence, auth)

## API Design (ASP.NET Core)
- Prefer **Minimal APIs** for new greenfield services; use **Controllers** in large, team-oriented codebases where convention is helpful
- Use **`IResult`** and `Results.Ok(...)`, `Results.Problem(...)` for consistent, typed HTTP responses
- Use **`IOptions<T>`** with `ConfigureOptions<T>` for strongly-typed, validated configuration
- Use **Output Caching** and **Response Caching** middleware for public endpoints
- Document APIs with `Swashbuckle.AspNetCore` (Swagger) or `Microsoft.AspNetCore.OpenApi` (built-in, .NET 9)

## .NET Aspire (Cloud-Native)
- Use **.NET Aspire** for orchestrating multi-service local development (API + DB + cache + messaging)
- Define the app model in the `AppHost` project; use `AddProject`, `AddRedis`, `AddPostgres` for wired service discovery
- Aspire's service defaults propagate OpenTelemetry, health checks, and resilience automatically — don't reinvent these

## Dependency Injection
- Register all services via `IServiceCollection` in `Program.cs` or modular extension methods
- Use the correctscope: `Singleton` for stateless services, `Scoped` for request-scoped work (EF DbContext), `Transient` for lightweight stateless factories
- Use `Scrutor` for assembly scanning and decorator registration

## Data Access (EF Core / Dapper)
- Use **EF Core 8** with compiled models for high-performance, large-schema applications
- Define migrations as code; never modify production schema manually
- For read-heavy or complex queries, drop down to **Dapper** with parameterized SQL — avoid `string.Format` in SQL
- Use the **Repository + Unit of Work** pattern or **Query/Command handlers** (MediatR) to decouple business logic from persistence

## Error Handling
- Use **`Result<T, TError>`** (via `ErrorOr`, `FluentResults`, or `OneOf`) for domain errors instead of exceptions-as-control-flow
- Handle infrastructure failures (DB, HTTP) with exceptions; surface them as `ProblemDetails` using `IProblemDetailsService`
- Use `GlobalExceptionHandler` middleware for consistent, centralized error responses

## Testing
- Use **xUnit** with **FluentAssertions** for expressive, readable tests
- Use **NSubstitute** or **Moq** for mocking; prefer `NSubstitute` for its cleaner API
- Use **Testcontainers for .NET** to spin up real databases or message brokers in integration tests
- Use **WebApplicationFactory<T>** for testing Minimal API / MVC endpoints end-to-end
- Enforce coverage with **Coverlet** via `dotnet test --collect:"XPlat Code Coverage"`

## Tooling & CI
- Run `dotnet format --verify-no-changes` in CI to enforce formatting
- Use **Roslyn Analyzers** (including `Microsoft.CodeAnalysis.NetAnalyzers`) with `<AnalysisMode>All</AnalysisMode>` in CI
- Use `dotnet-outdated` to track dependency freshness
- Build native binaries with **Native AOT** (`<PublishAot>true</PublishAot>`) for CLI tools and serverless functions where startup time matters
