#!/usr/bin/env bash
set -euo pipefail

# Persistent production schema belongs to numbered SQL migrations. The legacy
# adopter is the only Rust module allowed to construct conditional DDL because
# SQLite cannot add a column conditionally in static migration SQL. Keep the
# policy implementation in Python so this gate does not silently pass when an
# optional grep-like utility is absent from the verification environment.

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
repo_root=$(cd -- "$script_dir/.." && pwd)
exec python3 "$script_dir/check_schema_migration_ownership.py" --repo "$repo_root"
