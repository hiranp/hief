# HIEF Documentation Scaffolding Guide

HIEF includes a powerful documentation scaffolding engine designed to support **Spec-Driven Development (SDD)** and **Harness-Driven Development (HDD)**. This guide explains how to use the `hief docs` commands and how the template engine works.

## Core Commands

### `hief docs init`
Initializes a standard documentation structure in your project root:
- `docs/specs/`: For feature specifications and constitutions.
- `docs/harness/`: For test harnesses and simulation playbooks.
- `docs/context/`: For session-based context logs.

### `hief docs list`
Lists all available embedded templates, their required variables, and default output paths.

### `hief docs generate <template_id>`
Generates a new document from a template. 
- Use `--name <value>` to satisfy the primary variable (e.g., the feature name).
- Use `--var KEY=VALUE` for additional custom variables.
- Use `--force` to overwrite existing files.

```sh
hief docs generate spec --name "user-auth" --var actor="Developer" --auto-populate
```

## Template Engine

HIEF uses **MiniJinja** (a lightweight Jinja2 implementation in Rust) to render templates.

### Variable Substitution
Templates use the `{{ var_name }}` syntax. 
- If a variable is provided via CLI or auto-population, its value is injected.
- If a variable is **unresolved**, HIEF preserves the `{{ var_name }}` literal in the output as a placeholder for the user or an AI agent to fill in.

### Auto-Population
When running `docs generate` with `--auto-populate`, HIEF automatically discovers and injects project context:
- `project_name`: Detected from `Cargo.toml`, `package.json`, or the git remote.
- `file_count`: Total number of tracked files.
- `chunk_count`: Total number of indexed code chunks.
- `languages`: List of languages detected in the project.

## Customization

### Template Overrides
You can override any embedded template by placing a file with the same ID in `.hief/templates/`.
For example, to customize the `spec` template, create:
`.hief/templates/spec.md`

HIEF will automatically detect this file and use it instead of the default.

### Custom LLM Prompt
Running `hief docs init` generates a `SDD_LLM_PROMPT.md` in your templates directory. This is a meta-prompt you can copy and paste to your AI assistant to help it draft high-quality specifications that follow HIEF's conventions.

---
*For more information on the architecture, see the [Architecture Summary](architecture-summary.md).*
