# Knowledge Platform

Self-hosted encrypted team knowledge base with canonical Markdown, granular permissions and an administration console.

The project is being implemented from the approved [design specification](docs/superpowers/specs/2026-09-12-knowledge-platform-v1-design.md) using the tracked [implementation plan](docs/superpowers/plans/2026-09-12-knowledge-platform-v1.md).

## Development baseline

Requirements:

- Rust 1.98;
- Node.js 24;
- pnpm 11.19.

Run the backend:

```powershell
cargo run -p knowledge-server
```

Run the web application:

```powershell
pnpm --filter @knowledge/web dev
```

Run the current verification suite:

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm --filter @knowledge/web test --run
pnpm --filter @knowledge/web build
```
