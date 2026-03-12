# Flutter Framework Rules

> SDD conventions and best practices for Flutter (Dart) projects using HIEF.
> Reference: https://docs.flutter.dev/ | https://dart.dev/guides

## Architecture
- Use **Feature-First** folder structure: `features/<name>/{data,domain,presentation}/` rather than a flat type-based layout
- Follow **Clean Architecture** layers: `presentation` (widgets/pages) → `domain` (use cases/entities) → `data` (repositories/sources)
- Keep widgets thin and dumb — logic lives in a ViewModel, Cubit, or Notifier, not inline in `build()`
- Use `go_router` for declarative, deep-link-friendly navigation; define routes as typed constants

## State Management
- Use **Riverpod 2** (with `flutter_riverpod` + code generation via `riverpod_generator`) for dependency injection and state — it is compile-time safe and testable
- Use `AsyncNotifier` / `Notifier` for complex state with lifecycle; `Provider` for simple computed/constant values
- Use **Bloc/Cubit** (`flutter_bloc`) for event-driven state machines where explicit event modeling adds clarity (e.g., multi-step flows, complex UIs)
- Avoid `setState` outside of purely local, ephemeral UI state (e.g., focus, hover)

## Dart Language
- Enable **null safety** (`dart: >=3.0.0`) — use `?`, `!`, `late`, and `required` appropriately
- Prefer Dart 3 features: **patterns**, **records**, **sealed classes** for exhaustive matching
- Use `const` constructors wherever possible to enable compile-time widget tree optimization
- Use **named parameters** with `required` for clarity in widget constructors

## Networking & Data
- Use **Dio** (with interceptors for auth, retry, and logging) or the lighter **http** package for network calls
- Use `Retrofit` + Dio for generated type-safe API clients from OpenAPI specs
- Define domain-layer `Repository` interfaces and implement them in the data layer for testability
- Use **Drift** (SQLite ORM with code generation) or **Isar** for performant local database storage
- Use **Freezed** + `json_serializable` for immutable model classes with `copyWith`, `==`, and JSON support

## Performance
- Use `const` widgets to prevent unnecessary rebuilds
- Profile with Flutter DevTools' Performance and Memory tabs before optimizing
- Use `RepaintBoundary` around expensive, frequently-changing subtrees
- Avoid blocking the main isolate; offload heavy computation to `compute()` or `Isolate.run()`

## Platform Channels & Plugins
- Use existing `pub.dev` plugins before writing custom platform channels
- When writing a plugin, follow the federated plugin structure with separate `_android`, `_ios`, and `_platform_interface` packages
- Test platform-specific code on real devices — simulators miss GPU and performance edge cases

## Testing
- **Unit tests:** Test business logic (use cases, repositories, Cubits, Notifiers) in isolation using `mockito` or `mocktail`
- **Widget tests:** Use `flutter_test` with `WidgetTester` to test widget trees; mock providers/blocs with overrides
- **Integration tests:** Use `integration_test` package to run tests on real devices/emulators for E2E flows
- Run `flutter analyze` and `dart fix --apply` in CI to catch lint and style issues

## Tooling & CI
- Use `flutter pub run build_runner build --delete-conflicting-outputs` whenever you change `@freezed`, `@riverpod`, or `@JsonSerializable` annotated code
- Use `flutter_launcher_icons` and `flutter_native_splash` for consistent branding setup
- Run `flutter test --coverage` and upload to Codecov or similar in CI
- Use **Fastlane** or **Codemagic** for automated App Store / Play Store deployments

## Documentation
- Document all public classes and methods with `///` dartdoc comments
- Use `dart doc` to generate and verify documentation builds cleanly
