# Kotlin Framework Rules

> SDD conventions and best practices for Kotlin (Android & Multiplatform) projects using HIEF.
> Reference: https://developer.android.com/jetpack/compose | https://kotlinlang.org/docs/coroutines-overview.html

## UI Architecture (Jetpack Compose — Android)
- Use **Jetpack Compose** for all new Android UI development
- Follow the **Uni-directional Data Flow (UDF)** pattern with ViewModels
- Keep Composables stateless where possible by hoisting state to the caller or ViewModel
- Use `remember` and `rememberSaveable` for local UI state that survives configuration changes
- Use `LaunchedEffect` and `SideEffect` correctly for scoped side-effects within composables

## Kotlin Multiplatform (KMP)
- Use **Compose Multiplatform** for shared UI across Android, iOS, and Desktop targets
- Place shared logic in `commonMain`; platform-specific code in `androidMain`, `iosMain`, etc.
- Use `expect`/`actual` declarations for platform-specific APIs (e.g., filesystem, crypto)
- Use **Ktor Client** for multiplatform HTTP; **SQLDelight** for multiplatform local databases
- Use **kotlinx.serialization** for cross-platform JSON serialization (avoids Gson/Moshi platform limits)

## Server-Side Kotlin (Ktor)
- Use **Ktor** for Kotlin-first HTTP servers with coroutine-native routing and plugins
- Structure applications using `Application.module()` extensions in separate files
- Use `ContentNegotiation` + `kotlinx.serialization` for JSON handling
- Manage configuration with `ApplicationConfig` or `kotlin-dotenv`

## Coroutines & Flow
- Use **Coroutines** for all asynchronous work (network, disk, long-running tasks)
- Use `Flow` and `StateFlow`/`SharedFlow` to represent streams of data and UI state
- Always specify appropriate Coroutine Dispatchers (`Dispatchers.IO`, `Dispatchers.Main`, `Dispatchers.Default`)
- Avoid `GlobalScope`; use `viewModelScope` or `lifecycleScope` to tie work to lifecycle
- Use `supervisorScope` when child failures should not cancel siblings

## Dependency Injection
- Use **Hilt** (Android) or **Koin** (Multiplatform) for dependency injection
- Inject interfaces rather than concrete implementations to facilitate testing
- Use constructor injection whenever possible; avoid field injection

## Functional Programming
- Use **Arrow** (`arrow-kt`) for typed error handling (`Either`, `Option`), resource management, and optics
- Replace nullable chains with `Option` in business logic for explicit absence semantics

## Error Handling
- Use `runCatching` or `try-catch` blocks with custom sealed Exception hierarchies
- Represent UI errors as specific states: `sealed class UiState { data class Error(...) }`
- Log exceptions with `Timber` (Android) or `SLF4J/Logback` (Ktor/JVM)

## Tooling & Performance
- Use `ktlint` or `detekt` for linting and formatting
- Enable **R8** (Android) for code shrinking and obfuscation in production builds
- Profile memory and performance using the **Android Profiler** in Android Studio or Xcode Instruments (iOS)

## Testing
- Use **JUnit 5** + **MockK** for unit tests; `kotest` is a great alternative for expressive, spec-style tests
- Use `ComposeTestRule` for testing Android UI components
- Use `ktor-server-test-host` for testing Ktor routes in isolation
- Use **Turbine** to test `Flow` emissions in a concise, assertion-friendly way
