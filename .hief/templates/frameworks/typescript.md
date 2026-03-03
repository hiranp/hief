# TypeScript Framework Rules

> SDD conventions and best practices for TypeScript projects using HIEF.
> Reference: https://www.typescriptlang.org/docs/ | https://ts.dev/style/

## Compiler Settings
- Always enable `strict: true` in `tsconfig.json` (enables `noImplicitAny`, `strictNullChecks`, etc.)
- Enable `noUncheckedIndexedAccess` to catch unsafe array/object indexing
- Use `moduleResolution: "bundler"` (Vite/ESM) or `"node16"` (Node.js); never `"classic"`
- Set `target` to the minimum runtime you actually support — don't over-target `ES5`

## Types & Interfaces
- Prefer `interface` for object shapes that may be extended; `type` for unions, intersections, and aliases
- Never use `any` — use `unknown` and narrow with type guards, or `as const` for literal types
- Use `satisfies` operator to validate objects against types without widening
- Define domain types in a co-located `types.ts` — avoid a global `types/` barrel file
- Use branded/nominal types for IDs and other primitive-typed domain concepts:
  ```ts
  type IntentId = string & { readonly _brand: "IntentId" };
  ```

## Functions & Modules
- Prefer named exports over default exports (better refactoring and tree-shaking)
- Avoid barrel files (`index.ts` re-exporting everything) in large codebases — they hide circular deps
- Use `const` arrow functions for utilities; `function` declarations for named, hoistable functions
- Mark side-effect-free utilities as `pure` with a JSDoc `/** @pure */` comment

## Async
- Always return `Promise<T>` with an explicit type — never leave async return types as `: Promise<any>`
- Use `Promise.all` / `Promise.allSettled` for concurrent independent promises
- Avoid mixing `async/await` and `.then()/.catch()` chains in the same function
- Handle errors with `try/catch` in async functions; never swallow rejections

## Error Handling
- Define custom error classes extending `Error`:
  ```ts
  class HiefError extends Error {
    constructor(message: string, public readonly code: string) {
      super(message);
      this.name = "HiefError";
    }
  }
  ```
- Use `Result<T, E>` pattern (e.g. via `neverthrow`) for domain errors rather than throwing

## Testing
- Use `vitest` (Vite projects) or `jest` with `ts-jest` for unit tests
- Co-locate test files as `*.test.ts` next to the source file
- Use `@testing-library` for UI component tests
- Mock network calls with `msw` (Mock Service Worker) — never mock `fetch` directly
- Aim for 80%+ branch coverage on domain logic

## Tooling
- `eslint` + `@typescript-eslint` for linting; `prettier` for formatting
- `lefthook` or `husky` for pre-commit hooks running lint + type-check
- `publint` or `arethetypeswrong` before publishing any npm package
