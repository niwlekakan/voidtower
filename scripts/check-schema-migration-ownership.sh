#!/usr/bin/env bash
set -euo pipefail

# Persistent production schema belongs to numbered SQL migrations. The legacy
# adopter is the only Rust module allowed to construct conditional DDL because
# SQLite cannot add a column conditionally in static migration SQL.

violations=()
mapfile -t violations < <(
  rg -nUP \
    'sqlx::(?:query|query_as|raw_sql)\(\s*(?:r#*)?"\s*(?:CREATE\s+(?:UNIQUE\s+)?INDEX|CREATE\s+TABLE|ALTER\s+TABLE|DROP\s+TABLE|DROP\s+INDEX)' \
    backend/src \
    --glob '*.rs' \
    --glob '!db/legacy.rs' \
    || true
)

if ((${#violations[@]})); then
  printf 'Schema migration ownership check failed; production DDL found outside migrations/legacy adopter:\n' >&2
  printf '  %s\n' "${violations[@]}" >&2
  exit 1
fi

if [[ ! -f backend/migrations/0001_current_baseline.sql ]]; then
  printf 'Schema migration ownership check failed; baseline migration is missing.\n' >&2
  exit 1
fi

if [[ ! -f backend/migrations/0002_operation_contracts.sql ]]; then
  printf 'Schema migration ownership check failed; operation-contract migration is missing.\n' >&2
  exit 1
fi

printf 'Schema migration ownership check passed.\n'
