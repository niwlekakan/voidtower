#!/usr/bin/env bash
set -euo pipefail

# Keep machine state, internal planning, credentials, and generated artifacts
# out of the public repository. This is intentionally checked in CI because
# .gitignore does not prevent `git add --force` or already-tracked files.

violations=()

while IFS= read -r path; do
  case "$path" in
    .claude/*|.codex/*|.opencode/*|.agents/*|.devteam/*|.obsidian/*|*/.claude/*|*/.codex/*|*/.opencode/*|*/.agents/*|*/.devteam/*|*/.obsidian/*) ;;
    dev-data/*|backend/dev-data/*|dev-config/*|*/dev-data/*|*/dev-config/*) ;;
    target/*|*/target/*|node_modules/*|*/node_modules/*|dist/*|*/dist/*|build/*|*/build/*|coverage/*|*/coverage/*) ;;
    docs/adr/*|docs/handoffs/*|docs/internal/*|docs/operations/*|docs/superpowers/*) ;;
    CLAUDE.md|CLAUDE_*.md|AGENTS.md|*/AGENTS.md|HANDOFF.md|*Handoff*.md|*handoff*.md) ;;
    plan.md|future_plan.md|UI_plan.md|*_plan.md|*_manual_test.sh) ;;
    docs/*audit*.md|docs/*codebase-map*.md|docs/*gap-analysis*.md|docs/*edd*.md) ;;
    frontend/.design-sync/NOTES.md) ;;
    .env|.env.*|*/.env|*/.env.*)
      case "$path" in
        .env.example|*/.env.example) continue ;;
      esac
      ;;
    *.db|*.db-shm|*.db-wal|*.sqlite|*.sqlite3|*.migration.lock|*.pem|*.key|*.p12|*.pfx|*.jks) ;;
    credentials.json|*/credentials.json|*-credentials.json|*/\*-credentials.json) ;;
    secrets.json|*/secrets.json|*-secrets.json|*/\*-secrets.json) ;;
    bootstrap-token|*/bootstrap-token|.npmrc|*/.npmrc|.pypirc|*/.pypirc) ;;
    *.zip|*.tar.gz|*.tar.xz|*.deb|*.rpm) ;;
    *) continue ;;
  esac
  violations+=("$path")
done < <(git ls-files)

if ((${#violations[@]})); then
  printf 'Repository hygiene check failed; forbidden tracked paths:\n' >&2
  printf '  %s\n' "${violations[@]}" >&2
  exit 1
fi

bash scripts/check-schema-migration-ownership.sh

printf 'Repository hygiene check passed (%s tracked files checked).\n' "$(git ls-files | wc -l)"
