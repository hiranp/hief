# Next.js Framework Rules

> SDD conventions and best practices for Next.js projects (App Router) using HIEF.
> Reference: https://nextjs.org/docs | https://nextjs.org/docs/app/building-your-application/rendering/server-components

## Architecture (App Router)
- Use the `app/` directory for routing; `components/` for reusable UI; `lib/` for shared utilities
- **Server Components (RSC):** Default to Server Components; use `"use client"` only when interactivity (hooks, event listeners) is required
- Fetch data in Server Components and pass as props — reduces client bundle size and eliminates client waterfalls
- Use `layout.tsx` for shared persistent UI; `page.tsx` for unique route content; `template.tsx` when fresh mount per navigation is needed

## Data Fetching & Mutations
- Use the built-in `fetch` extended by Next.js for caching and revalidation (`next: { revalidate: 60 }`, `cache: "force-cache"`, `cache: "no-store"`)
- **Server Actions:** Use Server Actions for all data mutations — keep them in `actions.ts` or co-locate with the feature
- Handle loading states with `loading.tsx` and errors with `error.tsx` at the route segment level
- Use `unstable_cache` for caching expensive data-fetching functions outside of `fetch`

## Partial Pre-rendering (PPR)
- Enable **PPR** (`experimental.ppr = true`) to serve a static shell instantly and stream dynamic content
- Wrap dynamic content in `<Suspense>` boundaries with meaningful `fallback` UI to opt into streaming
- Avoid `cookies()`, `headers()`, or uncached DB calls in the static shell of a PPR route

## Performance & Optimization
- Use the `<Image />` component for automatic image optimization (WebP, AVIF, lazy loading)
- Use `next/font` for optimized, self-hosted web fonts that eliminate layout shift
- Use `next/og` for dynamically generated Open Graph images in `opengraph-image.tsx`
- Use Dynamic Imports (`next/dynamic`) for heavy Client Components to reduce initial JS payload
- Analyze your bundle with `@next/bundle-analyzer` before shipping to production

## Security
- Use `"use server"` carefully; Server Actions must validate user authorization and sanitize input independently
- Never expose environment variables to the client unless prefixed with `NEXT_PUBLIC_`
- Sanitize user-generated content to prevent XSS in client-side rendering
- Implement security headers via `next.config.ts` (`headers()`) or middleware

## Middleware
- Use `middleware.ts` at the project root for cross-cutting concerns: auth guards, locale redirects, A/B flags
- Keep middleware lean and fast — avoid database calls; use Edge-compatible logic only
- Use the `matcher` config to restrict middleware to relevant routes

## Tooling
- Use `ESLint` with the Next.js core web vitals config (`next/core-web-vitals`)
- Use `Turbopack` (via `next dev --turbo`) for significantly faster local development builds
- Use `Zod` for schema validation in both API routes and Server Actions

## Testing
- Use `Vitest` for unit tests of utilities and pure logic; `Playwright` for E2E testing of critical user flows
- Use `@testing-library/react` for testing Client Components in isolation
- Mock Next.js-specific APIs (`cookies`, `headers`, `redirect`) with `next/navigation` test utilities
