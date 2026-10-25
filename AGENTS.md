# Agent instructions

## Project

This project renders protein structures into line-only SVG artwork. The web app uses
Nuxt/Vue/UnoCSS with a client-side Rust/WASM engine. The original Python implementation
is kept in `legacy/python/` as a regression reference. See `README.md` and
`docs/web-plan.md`. Generated renders, WASM build artifacts and local structure bundles
are intentionally ignored by Git; do not force-add them. Never send user structures
to a server for parsing or rendering.

## Git workflow

- Use Git throughout each task. Start by checking `git status` and reviewing any
  existing changes; preserve unrelated user work.
- Commit regularly at meaningful, working checkpoints (for example, a completed
  fix with its tests), rather than leaving an entire multi-step task uncommitted.
  Keep commits small, coherent, and focused on one logical change; do not create
  arbitrary or broken intermediate commits merely to increase commit frequency.
- Use Conventional Commits: `<type>: <subject>`, or
  `<type>(<jira-id>): <subject>` when a relevant Jira issue is known. Types include
  `feat`, `fix`, `docs`, `refactor`, `perf`, `test`, `build`, `ci`, `style`, and `chore`.
- Use a lowercase, imperative subject without a trailing period; keep the subject
  line at most 120 characters. Add a body explaining why when useful.
  Examples: `fix: preserve chain ids when loading structures` and
  `docs: explain render output options`.
- Before committing, review `git diff`, run relevant checks, stage only intended
  files or hunks, and review `git diff --cached`. Never include secrets, generated
  output, or unrelated user changes.
- If checks fail or cannot run, report that clearly; do not claim successful
  validation. Fix task-related failures before making a normal completion commit.
- Do not amend existing commits, rewrite history, discard changes, or push to a
  remote unless the user explicitly requests it. Explicit user instructions to
  avoid commits take precedence over this default workflow.
- End each task with a concise summary of changes, checks, commits, and any
  remaining uncommitted work.

## Validation

Run the unit tests after changing Python behavior:

```bash
uv run --with shapely python -m unittest discover -s legacy/python -v
```

An existing environment can also be used with `.venv/bin/python -m unittest discover -s legacy/python -v`
when it contains the required dependencies. Some structure-loading tests require
optional scientific dependencies; report skipped tests as well as failures.
