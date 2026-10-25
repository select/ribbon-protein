# Ribbon studio

A client-side protein-to-pen-plot studio: Nuxt, Vue and UnoCSS around a Rust/WebAssembly renderer. The web implementation is in progress; see the [implementation plan](docs/web-plan.md) for architecture, scope and acceptance checks.

## Layout

- `app/` — browser UI, molecular parser and worker integration.
- `crates/ribbon-core/` — Rust ribbon geometry and line-only SVG calculation.
- `public/templates/bierdeckel.svg` — one extracted 93.14 mm coaster guide, preview-only.
- `legacy/python/` — original Python renderer, batch tools and regression tests.
- `docs/` — design and migration documentation.
- `output/` — ignored local renders; existing output has not been moved.

## Python reference

The original implementation remains usable. From the repository root:

```bash
uv run legacy/python/render.py input.cif -o output/protein.svg
.venv/bin/python -m unittest discover -s legacy/python -v
```

See [the original renderer documentation](legacy/python/README.md) for options. Prefix the script paths in its older commands with `legacy/python/` when running from the repository root. Keep running batch commands from the repository root so their relative input/output paths continue to resolve.

The Python implementation is independent of the original Go `fogleman/ribbon` library. It is retained as a reference, not a web server dependency.
