# Custom Template Overrides

Place template files here to override HIEF's embedded defaults.
Templates use `{{variable_name}}` syntax for variable substitution.

## Supported Template Files

| File | Overrides |
|------|-----------|
| `constitution.md` | Project constitution template |
| `spec.md` | Feature specification template |
| `data-model.md` | Data model & contracts template |
| `harness.md` | Harness specification template |
| `playbook.md` | Simulation playbook template |
| `golden.toml` | Golden set evaluation template |

## Variables

Variables are substituted when generating documents. Common variables:

- `{{project_name}}` — Project name (auto-detected from Cargo.toml/package.json/git)
- `{{feature}}` — Feature name (from `--name` flag)
- `{{id}}` — Intent ID (from `--id` flag or auto-generated)
- `{{file_count}}` — Total indexed files (auto-populated from index)
- `{{languages}}` — Indexed languages (auto-populated from index)
- `{{chunk_count}}` — Total indexed chunks (auto-populated from index)

Unresolved variables remain as `{{placeholder}}` for manual editing.

## Example

To customize the constitution template:

```bash
cp $(hief docs show-template constitution) .hief/templates/constitution.md
# Edit .hief/templates/constitution.md
hief docs generate constitution
```
