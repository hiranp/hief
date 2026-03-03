# HIEF Templates

Example SDD (Spec-Driven Development) documents for use with `hief docs generate`.

Inspired by:
- [AWS Kiro](https://aws.amazon.com/startups/prompt-library/kiro-project-init?lang=en-US)
- [GitHub Spec-Kit](https://github.com/github/spec-kit)
- [Tessl](https://docs.tessl.io/introduction-to-tessl/quickstart-skills-docs-rules)

## Structure

```
templates/
  README.md                      — This file
  specs/
    example-constitution.md      — Project governing principles example
    example-spec.md              — Feature spec example
    example-data-model.md        — Data model and API contracts example
  harness/
    example-harness.md           — HDD test harness example
    example-playbook.md          — Multi-agent simulation playbook example
  golden/
    example-golden.toml          — TOML evaluation case example
```

## Usage

Use these as starting points. Generate a fresh template with:

```bash
hief docs generate spec --name <feature>
hief docs generate constitution
hief docs generate data-model
```

Then fill placeholders manually or pass `.hief/templates/SDD_LLM_PROMPT.md` to your AI assistant.
