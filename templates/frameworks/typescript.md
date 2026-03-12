# TypeScript Framework Rules

> SDD conventions and best practices for TypeScript projects using HIEF.
> Reference: https://www.typescriptlang.org/docs/ | https://ts.dev/style/

## Compiler Settings
- Always enable `strict: true` in `tsconfig.json` (enables `noImplicitAny`, `strictNullChecks`, etc.)
- Enable `noUncheckedIndexedAccess` to catch unsafe array/object indexing
- Enable `verbatimModuleSyntax` to enforce consistent `import type` usage and avoid runtime surprises
- Use `moduleResolution: "bundler"` (Vite/ESM) or `"node16"` (Node.js); never `"classic"`
- Set `target` to at least `ES2022` for modern features like top-level await and class fields
- Use `paths` aliases in `tsconfig.json` for cleaner imports (`@/` → `src/`)

## Types & Data Validation
- Prefer `interface` for object shapes; `type` for unions/aliases
- **Runtime Validation:** Use `Zod` or `Valibot` for all I/O boundaries (API responses, file logs, user input)
- Never use `any` — use `unknown` and narrow with type guards or `z.infer<T>`
- Use `satisfies` operator to validate objects against types without widening
- Define domain types in a co-located `types.ts` — avoid a global `types/` barrel file

## Functions & Modules
- Prefer named exports over default exports
- Avoid barrel files (`index.ts` re-exporting everything) as they cause circular dependencies and slow bundling
- Mark side-effect-free utilities as `pure` with a JSDoc `/** @pure */` comment
- Use `const` arrow functions for simple utilities; `function` for hoisted/complex ones

## Async
- Always return `Promise<T>` with an explicit type — never `: Promise<any>`
- Use `Promise.all` / `Promise.allSettled` for concurrent independent promises
- Handle errors with `try/catch` in async functions; never swallow rejections

## Error Handling
- Define custom error classes extending `Error` with a `code` field for programmatic handling
- Avoid throwing raw strings — always throw `new Error()` or a subclass
- Consider `neverthrow` or `effect-ts` for Result-based error handling in critical logic paths

## Testing
- Use `vitest` as the primary test runner for its speed and Vite integration
- Co-locate test files as `*.test.ts` next to the source file
- Use `@testing-library` for UI component tests; `msw` for network mocking
- Use `faker` or `zod-mock` to generate realistic test data
- Run type-check as a CI step: `tsc --noEmit`

## Tooling & Performance
- **Modern alternative:** Use `biome` as a single fast tool for linting + formatting (replaces eslint + prettier)
- **Classic stack:** `eslint` + `@typescript-eslint` + `prettier` (still valid, especially for large configs)
- Use `pnpm` for faster, disk-efficient dependency management
- Enable `isolatedModules: true` to ensure compatibility with modern transpilers (esbuild, swc)
- Use `import type` for type-only imports to improve bundling and respect `verbatimModuleSyntax`
- Use `tsx` for running TypeScript files directly in scripts/tooling without a build step
