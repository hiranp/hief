# FastAPI Framework Rules

> SDD conventions and best practices for FastAPI (Python) projects using HIEF.
> Reference: https://fastapi.tiangolo.com/ | https://docs.pydantic.dev/

## API Design
- Use **Type Hints** for all path parameters, query parameters, and request bodies
- Define separate `RequestSchema` and `ResponseSchema` Pydantic models — never reuse internal domain models as API contracts
- Use `JSONResponse` or custom response classes only when standard Pydantic models are insufficient
- Group related endpoints using `APIRouter` with descriptive `prefix`, `tags`, and `dependencies`

## Application Lifespan
- Use the **lifespan context manager** (`@asynccontextmanager`) instead of deprecated `@app.on_event` handlers
- Initialize shared resources (DB pools, HTTP clients, caches) in `startup` and close them in `shutdown`
- Expose the `app` via a factory function (`create_app()`) for testability and flexibility

## Data Validation & Modeling
- Use **Pydantic v2** for all request and response schemas
- Use `Annotated` for dependency injection and complex field validation constraints
- Set `model_config = ConfigDict(frozen=True)` for response models to enforce immutability
- Document fields with `Field(..., description="...", examples=[...])` to generate high-quality OpenAPI specs

## Database & Migrations
- Use **SQLModel** for type-safe ORM models that are also valid Pydantic schemas
- Use **Alembic** for database migrations; keep migrations in version control and run them as part of deployment
- Use async database drivers (`asyncpg`, `aiomysql`) with **SQLAlchemy 2.0** async sessions for non-blocking queries
- Use connection pooling; configure pool size based on concurrency requirements

## Dependency Injection
- Use `Depends()` to extract shared logic (auth, database sessions, common query params)
- Keep dependencies small and composable
- Use **Annotated Dependencies** for better readability and testing:
  ```python
  CurrentUser = Annotated[User, Depends(get_current_user)]
  ```

## Async & Performance
- Use `async def` for endpoints performing I/O (DB, network); use `def` for CPU-bound or legacy sync libraries
- Use `httpx.AsyncClient` for non-blocking external API calls; reuse the client via `app.state`
- Use `BackgroundTasks` for fire-and-forget work (email, webhooks) that doesn't affect the response

## Logging & Observability
- Use `structlog` (or `loguru`) for structured logging; configure JSON output in production
- Add request ID middleware to correlate logs across services
- Instrument with OpenTelemetry for distributed tracing

## Security
- Use FastAPI's built-in `Security` and `OAuth2PasswordBearer` for authentication; consider `fastapi-users` for full auth flows
- Never store secrets in code; use `pydantic-settings` with `.env` file support to load from environment variables
- Implement proper CORS middleware settings — avoid `allow_origins=["*"]` in production
- Validate and sanitize all file uploads; restrict file types and sizes

## Testing
- Use `pytest` with `httpx.AsyncClient` + `ASGITransport` for integration testing of endpoints
- Use `app.dependency_overrides` to mock databases or external services during tests
- Test both success cases and expected error codes (400, 401, 404, 422, etc.)
- Use `pytest-anyio` or `pytest-asyncio` with `asyncio_mode = "auto"` for clean async test setup
