# Contributing

Thanks for your interest in HIEF! Please follow the workflow described in
`AGENTS.md` when submitting changes. In short:

1. Create an intent via `hief intent create` and record it in git.
2. Work on your changes in a feature branch.
3. Run `hief eval run` to validate behavior before opening a PR.
4. Ensure code is formatted (`cargo fmt`) and passes `cargo clippy`.
5. Open a pull request; a human reviewer must mark the intent `approved` before
   merging.
