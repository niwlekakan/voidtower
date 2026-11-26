# Contributing to VoidTower

VoidTower is open source under AGPL-3.0-or-later. Contributions are welcome.

## Getting Started

1. Fork and clone the repository.
2. Install Rust (stable) and Node.js 20+.
3. Run `cd backend && cargo build` to verify the backend compiles.
4. Run `cd frontend && npm install && npm run build` to verify the frontend builds.
5. Make changes on a feature branch.
6. Submit a pull request.

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

VoidTower is licensed under AGPL-3.0-or-later. See [LICENSE](LICENSE) for the full text
and [LICENSING.md](LICENSING.md) for the commercial licensing terms.

Contributions are accepted only under the terms of [CLA.md](CLA.md). Include the
following trailer in every commit, with your real name and contributing email:

```
Contribution-License-Agreement: 1.0
Signed-off-by: Full Name <email@example.com>
```

A pull request without this trailer cannot be merged. The CLA is what allows the project
to keep offering both the AGPL edition and commercial licenses; accepting a contribution
without it would permanently prevent relicensing that code.

