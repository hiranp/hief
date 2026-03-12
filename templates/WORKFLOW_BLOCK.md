## HIEF Mandatory Workflow

1. Run `hief docs init` to scaffold/update templates and workflow rules.
2. Generate docs via templates only:
    - `hief docs generate spec --name <feature>`
    - `hief docs generate data-model`
    - `hief docs generate harness --name <feature>`
3. Read and obey `docs/specs/constitution.md`.
4. Ground content in local code context (`search_code`, structural search, and source files).
5. Replace all `{{placeholder}}` values before review.

Reference URLs:
- https://github.com/hiranp/hief/tree/main/templates
- https://aws.amazon.com/startups/prompt-library/kiro-project-init?lang=en-US
- https://github.com/github/spec-kit?tab=readme-ov-file#-specify-cli-reference
- https://docs.tessl.io/introduction-to-tessl/quickstart-skills-docs-rules
