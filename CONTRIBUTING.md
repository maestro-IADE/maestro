# Contributing to Maestro

Thanks for your interest. Maestro is an early preview, so the most useful contributions right now are bug reports and feedback in [Issues](https://github.com/maestro-IADE/maestro/issues). Small fixes are welcome too; for anything larger, please open an issue first so we can agree on the direction.

## Setup

You need [Rust](https://rustup.rs/), [Node.js](https://nodejs.org/) 22 or later, and the [Tauri prerequisites](https://tauri.app/start/prerequisites/) for your OS.

```bash
npm install
npm run tauri dev
```

## Checks

CI runs these on Windows, macOS, and Linux for every pull request. Run them before you push:

```bash
npm run build                 # type-check and build the frontend (cargo needs dist/)
npm test                      # frontend checks
cd src-tauri && cargo test    # backend tests
```

## Commit messages and pull request titles

We use [Conventional Commits](https://www.conventionalcommits.org/). Release notes are generated from them.

```
<type>(<optional scope>): <summary in the imperative, lower case, no period>
```

| Type | Use for |
|---|---|
| `feat` | a new feature users can see |
| `fix` | a bug fix |
| `docs` | documentation only |
| `refactor` | code change that neither fixes a bug nor adds a feature |
| `perf` | performance |
| `test` | tests only |
| `build` | build system, dependencies, packaging |
| `ci` | GitHub Actions workflows |
| `chore` | anything else (release commits, tooling) |

Scopes are optional; use the area you touched, such as `canvas`, `chat`, `pr`, `guard`, `worktree`, `cli`, `ui`.

Examples:

```
feat(canvas): send an instruction to the selected block
fix(pr): keep the card pending while a check is still running
docs: add the release process
```

A breaking change adds `!` after the type (`feat!: …`) and explains it in the body.

## Branches

Name branches after the type: `feat/short-name`, `fix/short-name`, `docs/short-name`, `ci/short-name`.

## Pull requests

- One topic per pull request.
- The **pull request title** must follow the commit format above; CI checks it.
- Pull requests are **squash-merged**, so the title becomes the single commit on `main`.
- Fill in the template: what changed, how you checked it, and which docs you updated.

## Docs

Feature and architecture notes live in [`docs/`](docs/README.md). They are written in Korean, one topic per file, with frontmatter and fixed sections (see `docs/README.md`). A change that alters behaviour updates its doc in the same pull request.

## Releases

Maintainers release by bumping the version and pushing a tag; see [`docs/reference/release.md`](docs/reference/release.md) (Korean).

## License

By contributing, you agree that your contributions are licensed under the project's [FSL-1.1-MIT license](LICENSE.md).
