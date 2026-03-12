# Vue Framework Rules

> SDD conventions and best practices for Vue.js projects (Vue 3) using HIEF.
> Reference: https://vuejs.org/ | https://pinia.vuejs.org/

## Component Design
- Use **Script Setup** (`<script setup>`) for all new components (Vue 3 Composition API)
- Prefer **SFC** (Single File Components) with `<template>`, `<script>`, and `<style>` sections
- Use `PascalCase` for component filenames and references in templates
- Keep components focused; use **Props** for data down and **Emits** for events up
- Use `defineModel()` (Vue 3.4+) for two-way binding instead of manual prop + emit boilerplate

## State Management
- Use **Pinia** for global state management; avoid Vuex in new projects
- Distinguish between **Local State** (`ref`, `reactive` in components) and **Global State** (Pinia stores)
- Use `computed` for all derived state to ensure reactivity and performance
- Pinia actions handle both sync and async operations; keep getters pure and projection
- Use `storeToRefs()` when destructuring Pinia stores to preserve reactivity

## TypeScript Integration
- Define props with `defineProps<Props>()` and emits with `defineEmits<Emits>()` for full type safety
- Use `interface` for domain models; co-locate types if they are component-specific
- Use `Zod` for runtime validation of external data before populating stores

## Composables
- Extract reusable stateful logic into composables in a `composables/` directory (e.g., `useAuth`, `useTheme`)
- Prefer composables from **VueUse** (`@vueuse/core`) for common operations (scroll, storage, media queries, timers) before writing your own
- Always clean up side effects in composables using `onUnmounted` or `watchEffect`'s return value

## Routing (Vue Router)
- Use `vue-router` for navigation; define routes in a centralized `router/index.ts`
- Use **Route Guards** (`beforeEach`, `meta` fields) for authentication and authorization logic
- Implement **Lazy Loading** for route components: `component: () => import('./views/Home.vue')`
- Use typed routes with `unplugin-typed-router` or `vue-router`'s typed route helpers

## Styling & Assets
- Use scoped CSS (`<style scoped>`) to prevent style leakage between components
- Use CSS Variables for consistent design system tokens; integrate with a Vite PostCSS pipeline
- Optimize images using Vite's asset handling or `vite-imagetools`
- Consider **UnoCSS** or **Tailwind** as a utility-first layer for design tokens

## Nuxt 3 (Full-Stack)
- Use **Nuxt 3** when you need SSR, SSG, or a full-stack Vue framework with file-based routing
- Use `useFetch` / `useAsyncData` for server-aware data fetching with caching and hydration
- Use Nuxt Server Routes (`/server/api/`) for lightweight backend endpoints

## Testing
- Use `Vitest` for unit tests and `@vue/test-utils` for component testing
- Use `Playwright` or `Cypress` for E2E testing of critical paths
- Mock Pinia store state and actions using Pinia's testing utilities (`createTestingPinia`)
