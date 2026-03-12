# React Framework Rules

> SDD conventions and best practices for React (web) projects using HIEF.
> Reference: https://react.dev/ | https://react-typescript-cheatsheet.netlify.app/

## Component Design
- Prefer **Functional Components** with Hooks over Class Components
- Keep components small and focused (Single Responsibility Principle)
- Use `PascalCase` for component files and function names; `camelCase` for props and hooks
- Favor composition over deep prop drilling — use **Context API** for low-frequency state, **Zustand** or **Jotai** for high-frequency global state

## TypeScript Integration
- Define prop types using `interface` or `type` (e.g., `interface ButtonProps { ... }`)
- Avoid `React.FC`; prefer explicit return types: `export function MyComponent(props: Props): React.ReactElement`
- Use `Zod` or `satisfies` to validate complex state or API data before rendering

## Server State (TanStack Query)
- Use **TanStack Query v5** (`@tanstack/react-query`) for all server state management — avoid manual `useEffect` + `useState` for data fetching
- Define query keys as typed tuples in a central `queryKeys.ts` file for cache management
- Use `useMutation` for write operations and invalidate related query caches on success
- Use `queryClient.prefetchQuery` in route loaders for zero-loading-state navigation

## Routing
- Use **React Router v7** (framework mode) or **TanStack Router** for fully type-safe routing
- Co-locate route components near their feature directory; avoid a flat top-level `pages/` folder
- Use `<Outlet />` and nested routing for shared layouts

## Hooks & State Management
- **Rule of Hooks:** Never call hooks inside loops, conditions, or nested functions
- Use `useMemo` and `useCallback` only when performance profiling indicates a need (avoid premature optimization)
- For complex state logic, use `useReducer` to consolidate state transitions
- Use custom hooks (e.g., `useAuth`, `useFetch`) to encapsulate reusable logic

## React 19+
- Use the **React Compiler** (formerly React Forget) to automatically optimize memoization — avoid manual `useMemo`/`useCallback` in new code
- Use `useActionState` and `useFormStatus` for form state management with Server Actions
- Use `use(Promise)` hook to suspend on async resources in Client Components

## Rendering & Performance
- Use `key` props correctly in lists (avoid array indices as keys)
- Implement **Code Splitting** using `React.lazy` and `Suspense` for large application modules
- Use **React DevTools Profiler** to identify unnecessary re-renders before adding memoization

## Error Handling
- Use **Error Boundaries** to catch and handle rendering errors at the component level
- Provide meaningful fallback UI for failed components
- Use TanStack Query's `onError`/`throwOnError` for consistent server error handling

## Testing
- Use `Vitest` + `@testing-library/react` for unit and integration tests
- Focus on testing user behavior (e.g., "clicking button triggers action") rather than implementation details
- Use `msw` (Mock Service Worker) for mocking API calls in component tests
- Use `Playwright` for E2E testing of critical user flows
