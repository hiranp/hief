# Python Framework Rules

> SDD conventions and best practices for Python projects using HIEF.
> Reference: https://peps.python.org/pep-0008/ | https://docs.pydantic.dev/

## Code Style
- Follow PEP 8; use `ruff` for linting and formatting (replaces `flake8` + `black`)
- Max line length: 100 characters
- Use f-strings over `.format()` or `%` formatting
- Prefer `pathlib.Path` over `os.path` for all filesystem operations

## Type Hints
- All function signatures must include parameter and return type hints (PEP 484)
- Use `from __future__ import annotations` for forward references in Python < 3.10
- Use `TypeAlias` (3.10+) or `NewType` to document domain-specific scalar types
- Prefer `X | None` over `Optional[X]` (Python 3.10+)

## Data Validation
- Use `pydantic` (v2) for all data models crossing API or I/O boundaries
- Define Pydantic models with `model_config = ConfigDict(frozen=True)` for immutable data
- Validate environment variables at startup using `pydantic-settings`
- Never use raw `dict` for structured data that lives beyond a single function

## Error Handling
- Never silence exceptions with bare `except:` — always catch specific exception types
- Use custom exception classes inheriting from a domain base (`class HiefError(Exception)`)
- Log errors with `logging.exception()` (captures traceback) rather than `logging.error()`
- Use `contextlib.suppress` only for genuinely ignorable exceptions, documented with a comment

## Async (asyncio)
- Use `asyncio` + `httpx` for async HTTP; avoid mixing sync/async in the same call stack
- Use `async with` for resource management (DB sessions, HTTP clients)
- Use `asyncio.TaskGroup` (3.11+) for structured concurrency rather than bare `asyncio.create_task`
- Profile with `asyncio.get_event_loop().set_debug(True)` before optimising

## Testing
- Use `pytest` with `pytest-asyncio` for async tests
- Use `pytest-cov` to enforce ≥ 80% coverage on new code
- Fixtures in `conftest.py` — never inline test setup in test functions
- Use `tmp_path` fixture (pytest built-in) for filesystem tests
- Mock external I/O with `unittest.mock.AsyncMock` or `respx` (for `httpx`)

## Dependency Management
- Use `uv` for dependency management and virtual environments
- Pin dependencies in `uv.lock`; use version ranges in `pyproject.toml`
- Separate `[project.dependencies]` (runtime) from `[project.optional-dependencies]` (`dev`, `test`)
- Never import from `src/` with relative imports across package boundaries

## Documentation
- Docstrings: Google style for all `public` functions and classes
- Include `Args:`, `Returns:`, and `Raises:` sections
- Generate API docs with `mkdocs` + `mkdocstrings`
