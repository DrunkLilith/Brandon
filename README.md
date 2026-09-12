# Knowledge Platform

Self-hosted encrypted team knowledge base with canonical Markdown, granular permissions and an administration console.

The project is being implemented from the approved [design specification](docs/superpowers/specs/2026-09-12-knowledge-platform-v1-design.md) using the tracked [implementation plan](docs/superpowers/plans/2026-09-12-knowledge-platform-v1.md).

## Development baseline

Requirements:

- Rust 1.98;
- Node.js 24;
- pnpm 11.19;
- PostgreSQL 18.

The server requires `DATABASE_URL`, its canonical HTTPS
`KNOWLEDGE_PUBLIC_ORIGIN`, and a base64-encoded 32-byte
`KNOWLEDGE_MASTER_KEY`. Keep the master key outside PostgreSQL, logs and the
repository. Initialize the first workspace and administrator once:

```powershell
$env:DATABASE_URL = "postgres://knowledge:knowledge@localhost:5432/knowledge"
$env:KNOWLEDGE_PUBLIC_ORIGIN = "https://knowledge.example.com"
$env:KNOWLEDGE_MASTER_KEY = "<base64-encoded 32-byte secret>"
cargo run -p knowledge-server --bin bootstrap -- --email admin@example.com
```

The bootstrap command reads the administrator password from a hidden prompt;
password command-line arguments are deliberately rejected.

When the server is behind a reverse proxy, set
`KNOWLEDGE_TRUSTED_PROXY_CIDRS` to the proxy network (comma-separated CIDRs).
Forwarded client addresses are ignored for all connections outside that list.

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
