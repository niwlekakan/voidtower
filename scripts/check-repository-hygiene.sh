#!/usr/bin/env bash
set -euo pipefail

# Keep machine state, internal planning, credentials, and generated artifacts
# out of the public repository. This is intentionally checked in CI because
# .gitignore does not prevent `git add --force` or already-tracked files.

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
repo_root=$(cd -- "$script_dir/.." && pwd)
violations=()
tracked_file=$(mktemp)
trap 'rm -f "$tracked_file"' EXIT
if ! git -C "$repo_root" ls-files -s -z >"$tracked_file"; then
  printf 'Repository hygiene check failed; unable to enumerate tracked paths.\n' >&2
  exit 2
fi

while IFS= read -r -d '' record; do
  path=${record#*$'\t'}
  match_path=${path,,}
  match_basename=${match_path##*/}
  if [[ ${record:0:6} == 120000 ]]; then
    violations+=("$path (tracked symlink)")
    continue
  fi
  sensitive_basename=0
  case "$match_basename" in
    key|token|password|passwd|auth|.netrc|private_key|private-key|id_rsa|id_rsa.*|id_ed25519|id_ed25519.*|id_ecdsa|id_ecdsa.*|id_dsa|id_dsa.*|id_*_sk|*.pem|*.key|*.p12|*.pfx|*.jks)
      sensitive_basename=1
      ;;
    *.json|*.yaml|*.yml|*.txt)
      case "$match_basename" in
        *key*|*token*|*password*|*passwd*|*credential*|*secret*|auth-config.*)
          sensitive_basename=1
          ;;
      esac
      ;;
  esac
  if ((sensitive_basename)); then
    violations+=("$path")
    continue
  fi
  case "$match_path" in
    .claude/*|.codex/*|.opencode/*|.agents/*|.devteam/*|.obsidian/*|*/.claude/*|*/.codex/*|*/.opencode/*|*/.agents/*|*/.devteam/*|*/.obsidian/*) ;;
    dev-data/*|backend/dev-data/*|dev-config/*|*/dev-data/*|*/dev-config/*) ;;
    target/*|*/target/*|node_modules/*|*/node_modules/*|dist/*|*/dist/*|build/*|*/build/*|coverage/*|*/coverage/*) ;;
    docs/adr/*|docs/handoffs/*|docs/operations/*|docs/superpowers/*) ;;
    .env|.env.*|*.env|*/.env|*/.env.*)
      case "$path" in
        .env.example|*/.env.example) continue ;;
      esac
      ;;
    *.db|*.db-shm|*.db-wal|*.sqlite|*.sqlite3|*.migration.lock|*.pem|*.key|*.p12|*.pfx|*.jks) ;;
    id_rsa|id_rsa.*|*/id_rsa|*/id_rsa.*|id_ed25519|id_ed25519.*|*/id_ed25519|*/id_ed25519.*|id_ecdsa|id_ecdsa.*|*/id_ecdsa|*/id_ecdsa.*|id_dsa|id_dsa.*|*/id_dsa|*/id_dsa.*|id_*_sk|*/id_*_sk|private_key|*/private_key|private-key|*/private-key) ;;
    credentials.json|*/credentials.json|*-credentials.json|*/\*-credentials.json|*_credentials.json|*/\*_credentials.json|*credential*.json|*/\*credential*.json|*credential*.yaml|*/\*credential*.yaml|*credential*.yml|*/\*credential*.yml|*credential*.txt|*/\*credential*.txt) ;;
    secrets.json|*/secrets.json|*-secrets.json|*/\*-secrets.json|*_secrets.json|*/\*_secrets.json|*secret*.json|*/\*secret*.json|*secret*.yaml|*/\*secret*.yaml|*secret*.yml|*/\*secret*.yml|*secret*.txt|*/\*secret*.txt) ;;
    api_key.json|*/api_key.json|*-api_key.json|*/\*-api_key.json|*_api_key.json|*/\*_api_key.json|api-key.json|*/api-key.json|*-api-key.json|*/\*-api-key.json|*_api-key.json|*/\*_api-key.json|*key*.json|*/\*key*.json) ;;
    *token*.json|*/\*token*.json|*token*.yaml|*/\*token*.yaml|*token*.yml|*/\*token*.yml|*token*.txt|*/\*token*.txt|*password*.json|*/\*password*.json|*password*.yaml|*/\*password*.yaml|*password*.yml|*/\*password*.yml|*password*.txt|*/\*password*.txt|*passwd*.json|*/\*passwd*.json|auth.json|*/auth.json|auth-config.yaml|*/auth-config.yaml|auth-config.yml|*/auth-config.yml|auth.txt|*/auth.txt|.netrc|*/.netrc|*.netrc) ;;
    bootstrap-token|*/bootstrap-token|.npmrc|*/.npmrc|.pypirc|*/.pypirc) ;;
    *.zip|*.tar.gz|*.tar.xz|*.deb|*.rpm) ;;
    docs/internal/*) continue ;;
    CLAUDE.md|CLAUDE_*.md|AGENTS.md|*/AGENTS.md|HANDOFF.md|*Handoff*.md|*handoff*.md) ;;
    plan.md|future_plan.md|UI_plan.md|*_plan.md|*_manual_test.sh) ;;
    docs/*audit*.md|docs/*codebase-map*.md|docs/*gap-analysis*.md|docs/*edd*.md) ;;
    frontend/.design-sync/NOTES.md) ;;
    *) continue ;;
  esac
  violations+=("$path")
done <"$tracked_file"

if ((${#violations[@]})); then
  printf 'Repository hygiene check failed; forbidden tracked paths:\n' >&2
  printf '  %s\n' "${violations[@]}" >&2
  exit 1
fi

bash "$repo_root/scripts/check-schema-migration-ownership.sh"

printf 'Repository hygiene check passed (%s tracked files checked).\n' \
  "$(tr -cd '\0' <"$tracked_file" | wc -c)"
