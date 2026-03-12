# SvelteKit Framework Rules

> SDD conventions and best practices for SvelteKit (Svelte 5) projects using HIEF.
> Reference: https://kit.svelte.dev/ | https://svelte.dev/docs

## Architecture (SvelteKit)
- Use **file-based routing** in `src/routes/`; `+page.svelte` for pages, `+layout.svelte` for shared UI, `+server.ts` for API endpoints
- Default to **Server-Side Rendering (SSR)**; use `export const prerender = true` for static pages and `export const ssr = false` only when necessary for auth-gated pages
- Place shared utilities and components in `$lib` (`src/lib/`); import them with the `$lib/` alias (never relative paths from routes)
- Organise feature-level code within `$lib/features/<name>/` for large applications

## Svelte 5 Runes
- Use **Runes** for all state and reactivity in Svelte 5 — avoid the legacy `store` API in new code:
  - `$state` for mutable reactive state
  - `$derived` for computed values (replaces `$:` reactive declarations)
  - `$effect` for side effects (replaces `onMount`/`afterUpdate` in simple cases)
  - `$props` for component props (replaces `export let`)
- Use `$state.snapshot()` to take a plain (non-reactive) snapshot of state for serialization or debugging

## Data Loading
- Use `+page.server.ts` **load functions** for server-side data fetching (database, auth-gated APIs)
- Use `+page.ts` load functions for data that can be fetched on both client and server (public APIs)
- Always type the return value: `export const load = async (): Promise<{ user: User }> => { ... }`
- Use **Form Actions** (`+page.server.ts` `actions`) for all data mutations — they degrade gracefully without JS and integrate with SvelteKit's `enhance` progressive enhancement

## TypeScript
- Enable `strict: true` in `tsconfig.json`; SvelteKit generates types in `.svelte-kit/types/` — import `$types` from the generated path
- Use `Zod` for validating form data in server actions and API endpoints
- Define shared domain types in `$lib/types.ts` or co-locate with the feature

## Styling
- Use **scoped `<style>` blocks** in `.svelte` files for component-level CSS (styles are auto-scoped)
- Use CSS Custom Properties for design tokens shared across components
- Integrate **UnoCSS** or **Tailwind CSS** for utility-class layers; configure via `vite.config.ts`

## State Management
- Prefer **rune-based state in components** for local state; pass data down via props and emit events via `$props` callbacks
- For global state shared across routes, use **Svelte 5 rune stores** in `$lib/stores/` (plain `$state` exported from a module)
- Use SvelteKit's `$page` store for URL, params, and route data; `$navigating` for transition state

## Security
- Validate and sanitize all form action inputs server-side — client-side validation is UX only
- Never expose secrets in `+page.ts` load functions (they run on the client); use `+page.server.ts` instead
- Use SvelteKit's built-in CSRF protection (enabled by default for form actions)
- Set secure HTTP headers via `hooks.server.ts` using a library like `sveltekit-security-headers`

## Testing
- Use **Vitest** for unit testing utilities, stores, and pure logic
- Use `@testing-library/svelte` for component testing with user-centric assertions
- Use **Playwright** for E2E testing of full user flows, form submissions, and navigation
- Run type-check as a CI step: `svelte-check --tsconfig ./tsconfig.json`

## Tooling
- Use `eslint` with `eslint-plugin-svelte` and `prettier-plugin-svelte` for linting and formatting
- Use `vite-plugin-inspect` to debug build transforms during development
- Use `adapter-auto` in development; swap to `adapter-node`, `adapter-static`, or `adapter-vercel` for deployment
