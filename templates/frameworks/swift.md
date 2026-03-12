# Swift Framework Rules

> SDD conventions and best practices for Swift (iOS/macOS) projects using HIEF.
> Reference: https://developer.apple.com/xcode/swiftui/ | https://www.swift.org/documentation/concurrency/

## UI Architecture (SwiftUI)
- Use **SwiftUI** for all new UI development
- Follow the **MVVM** (Model-View-ViewModel) pattern or **The Composable Architecture (TCA)** for state-intensive apps
- Keep Views lightweight — logic belongs in ViewModels, Reducers, or domain models
- Use `@State`, `@StateObject`, and `@EnvironmentObject` appropriately for local vs. shared state
- Use `@Observable` (Swift 5.9+, iOS 17+) as a modern replacement for `ObservableObject` + `@Published`

## Swift Concurrency
- Use `async/await` for asynchronous operations; avoid completion handlers or `Combine` unless necessary for legacy interoperability
- Use `Task { ... }` to bridge sync and async boundaries cautiously; prefer `.task {}` view modifier for UI-anchored work
- Define `actor` types for shared mutable state to prevent data races
- Use `@MainActor` for ViewModels and UI-update logic; avoid `DispatchQueue.main.async`
- Use `TaskGroup` / `withThrowingTaskGroup` for structured concurrency across multiple async operations

## Error Handling
- Use `do-catch` blocks with typed `Error` enums conforming to `LocalizedError` for user-facing messages
- Prefer `Result<Success, Failure>` type for functions that cross async/sync or delegate-based boundaries
- Use `fatalError()` only for genuinely unreachable code paths; prefer `preconditionFailure` with a message

## Networking & Data
- Use `URLSession` with `Codable` for network requests and JSON parsing
- Leverage **SwiftData** (iOS 17+) for local persistence with `@Model` and `@Query` macros; fall back to CoreData for iOS 16
- Use `AsyncStream` or `AsyncThrowingStream` for sequences of events (e.g., location updates, WebSocket messages)

## Testing
- Use `XCTest` for unit and UI testing (Xcode 15 and earlier)
- Use the new **Swift Testing** framework (`import Testing`, `@Test`, `#expect`) for expressive, macro-driven tests (Xcode 16+)
- Use **Dependency Injection** (via TCA's `DependencyValues` or a custom DI container) to make ViewModels and Services testable
- Implement snapshot testing (e.g., `swift-snapshot-testing`) to catch visual regressions in UI components
- Use **Instruments** to profile memory, CPU, and hang-related issues before shipping

## Logging & Observability
- Use the `os.Logger` framework for structured, category-scoped logging
- Categorize logs by subsystem and category for easier filtering in Console.app
- Use `Privacy` options (`OSLogPrivacy`) to properly handle sensitive data: `.private` by default, `.public` only when safe
- Use `MetricKit` to capture on-device performance and hang rate data from production

## Privacy & Distribution
- Provide a **Privacy Manifest** (`PrivacyInfo.xcprivacy`) declaring required reason APIs, data types collected, and tracking status
- Sign all builds with Apple Developer IDs; use notarization for macOS distribution outside the App Store
- Review App Store Review Guidelines regularly, especially for apps using AI or user-generated content
