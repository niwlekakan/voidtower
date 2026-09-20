# Contributing to VoidTower

VoidTower is open source under AGPL-3.0-or-later. Contributions are welcome.

## Getting Started

1. Fork and clone the repository.
2. Install Rust (stable) and Node.js 20+.
3. Run `cd backend && cargo build` to verify the backend compiles.
4. Run `cd frontend && npm install && npm run build` to verify the frontend builds.
5. Make changes on a feature branch.
6. Submit a pull request.

Before requesting review, run the repository-owned release-candidate evidence
collector from the repository root:

```sh
python3 scripts/release_gate.py --repo . --scope changed --json
```

Use `--scope all --output dev-data/release-gate.json` when a complete
candidate report is required. Read `docs/release-gates.md` for the manifest,
exit-status, redaction, and evidence-boundary contract.

## Guidelines

- No telemetry, analytics, or third-party tracking of any kind.
- No cloud service dependencies.
- New API endpoints must be documented in the OpenAPI spec.
- Dangerous actions must require explicit user confirmation.
- Handle unavailable optional dependencies (Docker, LXC, KVM) gracefully.
- Frontend changes must work on desktop, tablet, and mobile.

## Code Style

- Rust: `cargo fmt` and `cargo clippy --deny warnings` must pass.
- TypeScript: ESLint must pass with zero warnings.
- Shell scripts: POSIX-compatible (`#!/bin/sh`) where possible.

## License

By contributing you agree your contributions are licensed under AGPL-3.0-or-later.
