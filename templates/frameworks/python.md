# Python Framework Rules

> SDD conventions and best practices for Python projects using HIEF.
> Reference: https://peps.python.org/pep-0008/ | https://docs.pydantic.dev/

## Tooling & Environment
- Use `uv` for lightning-fast dependency management and virtual environments
- Enforce linting and formatting with `ruff` (replaces `black`, `isort`, `flake8`)
- Use `pyproject.toml` as the single source of truth for project metadata
- Always pin dependencies in `uv.lock` for reproducible builds
- Use `pre-commit` hooks to run `ruff format` and `ruff check --fix` before every commit

## Type Hints & Modernity
- Use Python 3.10+ features: `X | Y` for unions, `match` statements for branching
- All functions must have complete type hints (parameters and return type)
- Use `TypeAlias` (3.10+) or `Annotated` (3.9+) to document domain concepts
- Run `mypy` in strict mode or `pyright` (faster, better IDE integration) to catch type errors statically
- **Emerging:** `ty` (from the Ruff team) is a next-generation type checker worth tracking for speed

## Data Validation (Pydantic v2)
- Use `pydantic` for all data crossing boundaries (API, Config, Filesystem)
- Prefer `BaseModel` with `model_config = ConfigDict(frozen=True, extra="forbid")`
- Use `Field(..., description="...")` to document model fields for LLMs/Agents and OpenAPI specs
- Use `pydantic-settings` for environment variable management with full type safety

## Error Handling
- Use custom exception hierarchies: `class ProjectError(Exception)`
- Never catch `Exception` without re-raising or logging with `logging.exception()`
- Use `contextlib.suppress` only for expected, safe-to-ignore cases
- Prefer `raise exc from cause` to preserve traceback chains

## Logging & Observability
- Use `structlog` for structured, context-rich logging in production services
- Use `loguru` for simpler scripts and CLIs that benefit from its ergonomic API
- Avoid bare `print()` in application code; prefer `logging.debug()` for development traces

## Async (asyncio)
- Use `asyncio` for I/O bound tasks; `httpx` for async HTTP requests
- Use `asyncio.TaskGroup` (3.11+) for structured concurrency
- Avoid `loop.run_until_complete` inside library code; propagate `async` up the stack
- Use `anyio` if the project needs to support multiple async backends (asyncio + trio)

## Testing
- Use `pytest` with `pytest-asyncio` for all testing
- Use `pytest-mock` for cleaner mocking than the standard library
- Enforce coverage targets (≥ 80%) using `pytest-cov`
- Use `hypothesis` for property-based testing of complex parsing or data-transform logic
- Run `mypy --strict` or `pyright` as a CI check alongside tests

## Documentation
- Follow **Google Style** or **NumPy Style** docstrings for all public members
- Include `Args:`, `Returns:`, and `Raises:` sections explicitly
- Use `mkdocs-material` for high-quality project documentation with search and versioning
