# Django Framework Rules

> SDD conventions and best practices for Django 5.x projects using HIEF.
> Reference: https://docs.djangoproject.com/ | https://www.django-rest-framework.org/

## Project Structure
- Use a **Django apps** layout: each app owns its own `models.py`, `views.py`, `serializers.py`, `urls.py`, and `tests/`
- Keep `settings.py` split: `base.py` (shared) → `local.py` / `production.py` / `test.py` via `DJANGO_SETTINGS_MODULE`
- Use `django-environ` or `python-decouple` to load secrets from environment variables — never hardcode `SECRET_KEY` or `DATABASE_URL`
- Place shared utilities in an `apps/common/` or `apps/core/` app to avoid cross-app circular imports

## Models & Database
- Use **Django ORM** with descriptive `verbose_name` and `db_index=True` on frequently filtered fields
- Add `__str__`, `get_absolute_url`, and a `Meta` class to every model
- Always use **`select_related`** (FK/OneToOne) and **`prefetch_related`** (M2M/reverse FK) to avoid N+1 queries
- Use `django-model-utils` (`TimeStampedModel`, `StatusModel`) for common model mixins
- Run `EXPLAIN ANALYZE` on slow queries; add indexes via `Meta.indexes` or `AddIndex` migrations

## Migrations
- Commit all migrations to version control; never `squashmigrations` without team coordination
- Name migrations descriptively: `0002_add_user_profile_bio.py`
- Review auto-generated migrations before applying — check for unexpected `ALTER TABLE` statements on large tables
- Use `--fake-initial` only on first deployment of a model that maps to a pre-existing table

## Views & URLs
- Follow **Class-Based Views (CBVs)** for CRUD and API endpoints; **Function-Based Views (FBVs)** for simple, one-off views
- Register URLs using `path()` with typed converters (`<int:pk>`, `<slug:slug>`); use `include()` per app
- Keep business logic out of views — delegate to service functions or domain model methods

## Django REST Framework (DRF)
- Use **DRF** with `ModelSerializer` as the default; override fields or `validate_<field>` for custom behavior
- Use **`@action`** decorators on ViewSets for non-CRUD operations
- Apply **permission classes** per view (`IsAuthenticated`, custom `BasePermission`) — never rely solely on URL-level auth
- Use **`django-filter`** for filterable list endpoints; **`drf-spectacular`** for OpenAPI schema generation

## Async Views (Django 5.x)
- Use `async def` views for endpoints that perform network I/O or use `asyncio`-native libraries
- Use `sync_to_async` / `async_to_sync` from `asgiref` to bridge ORM and async code correctly
- Deploy with an ASGI server (`uvicorn`, `daphne`) when using async views or channels

## Authentication & Security
- Use **`django-allauth`** for social auth and email verification flows
- Use **`djangorestframework-simplejwt`** for JWT-based API authentication
- Enable HTTPS-only cookies: `SESSION_COOKIE_SECURE = True`, `CSRF_COOKIE_SECURE = True`
- Set `ALLOWED_HOSTS`, `SECURE_HSTS_SECONDS`, `X_FRAME_OPTIONS = "DENY"` in production settings

## Logging & Observability
- Configure Django's `LOGGING` dict to use `structlog`-compatible handlers in production
- Use `django-silk` or `django-debug-toolbar` for query profiling in development
- Export metrics with `django-prometheus` for production observability

## Testing
- Use **`pytest-django`** with `@pytest.mark.django_db` for all tests; avoid `TestCase` unless necessary
- Use `RequestFactory` for unit testing views without a database; `APIClient` for DRF endpoint tests
- Use `factory_boy` for test data fixtures over raw `Model.objects.create()`
- Use `coverage run -m pytest` + `coverage report` to enforce a ≥ 80% coverage gate
- Run **`mypy-django`** (`django-stubs`) in strict mode alongside tests for type safety

## Tooling
- Format with `black` and lint with `ruff` (replaces `flake8`/`isort`)
- Use `pre-commit` hooks for `black`, `ruff`, and `mypy` to enforce standards before commit
- Use `celery` + `django-celery-beat` for task queues and scheduled background jobs
