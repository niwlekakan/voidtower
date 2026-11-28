# Licensing

VoidTower is distributed under the **GNU Affero General Public License, version 3 or
later** (`AGPL-3.0-or-later`). The full license text is in [`LICENSE`](LICENSE).

The same terms are declared in the package metadata:

| Manifest | Field |
|---|---|
| `backend/Cargo.toml` | `license = "AGPL-3.0-or-later"` |
| `frontend/src-tauri/Cargo.toml` | `license = "AGPL-3.0-or-later"` |
| `frontend/package.json` | `"license": "AGPL-3.0-or-later"` |
| `mobile/package.json` | `"license": "AGPL-3.0-or-later"` |

## What the AGPL allows

- Run the software for any purpose, including commercially.
- Study, modify, and redistribute it.
- Redistribute modified versions under the same license.

Two obligations matter in practice:

1. **Source on distribution.** If you distribute the software or a modified version,
   you must make the corresponding source available under the AGPL.
2. **Source on network use (section 13).** If you modify the software and let other
   people interact with it over a network, you must offer those users the corresponding
   source of your modified version.

There is no warranty, to the extent permitted by law.

## Commercial licensing

The copyright holder also licenses VoidTower under separate commercial terms, without
the AGPL's source-disclosure obligations. This is intended for:

- embedding VoidTower in a proprietary product or distribution;
- shipping it inside an appliance, or operating it as a managed service, without
  publishing your modifications;
- organisations whose procurement policy prohibits AGPL software.

Commercial terms are negotiated individually. Contact the copyright holder listed in
[`NOTICE`](NOTICE).

Nothing in this document grants a commercial license. Only a signed agreement does.

## Contributions

Contributions are accepted only under the terms in [`CLA.md`](CLA.md). The CLA grants
the project the rights needed to keep offering both the AGPL edition and commercial
licenses. A pull request without a signed CLA cannot be merged, because accepting it
would permanently prevent the project from being relicensed.

## Trademarks

The VoidTower name and logo are not licensed under the AGPL. Redistributing a modified
version requires removing or replacing them, and not implying endorsement by the
project.

## Third-party components

VoidTower depends on third-party Rust crates and npm packages under permissive licenses,
principally MIT, Apache-2.0, and BSD-3-Clause. The audited direct dependencies of the
backend carry no copyleft obligations. The complete dependency tree, including
transitive dependencies, is recorded in `backend/Cargo.lock` and
`frontend/package-lock.json`; each component remains under its own license and its
notices must be preserved on redistribution.

The application catalog in `app-vault/` contains deployment definitions for third-party
software. That software is pulled from its own upstream at deploy time and remains under
its own license.
