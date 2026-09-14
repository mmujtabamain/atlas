#!/bin/bash
set -euo pipefail

clear || true
clear || true
clear || true

RED=$'\033[0;31m'
GREEN=$'\033[0;32m'
YELLOW=$'\033[0;33m'
BLUE=$'\033[0;34m'
GRAY=$'\033[0;90m'
BOLD=$'\033[1m'
RESET=$'\033[0m'

log()     { printf '%s\n' "${BLUE}> $* ${RESET}"; }
step()    { printf '\n%s\n' "${BOLD}${BLUE}==> ${BOLD}$*${RESET}"; }
success() { printf '%s\n' "${GREEN}$* ${RESET}"; }
warn()    { printf '%s\n' "${YELLOW}$* ${RESET}"; }
fail()    { printf '%s\n' "${RED}$* ${RESET}" >&2; }

trap 'fail "Failed at line $LINENO. Aborting — nothing further will run."' ERR

run() {
  log "${GRAY}\$ $*${RESET}"
  "$@"
}

STORE_DIR="$(cd "$(dirname "$0")/.." && pwd)"
CONFIG_URI="file://$STORE_DIR/atlas.hcl"
ENTITIES_DIR="$STORE_DIR/src/entities"
TEMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/atlas-financer-entities.XXXXXX")"
TEMP_DB="$TEMP_DIR/entities.sqlite"
TEMP_ENTITIES="$TEMP_DIR/entities"

cleanup() {
  rm -f "$TEMP_DB" "$TEMP_DB-shm" "$TEMP_DB-wal"
  rm -rf "$TEMP_DIR"
}
trap cleanup EXIT

step "Step 1/4 — Check development tools"
command -v atlas >/dev/null 2>&1 || { fail "Atlas CLI is required: https://atlasgo.io/getting-started#installation"; exit 1; }
command -v sea-orm-cli >/dev/null 2>&1 || { fail "sea-orm-cli 1.1.20 is required: cargo install sea-orm-cli --version 1.1.20 --locked"; exit 1; }
expected_atlas="$(sed -n 's/^atlas=//p' "$STORE_DIR/TOOL_VERSIONS")"
expected_sea_orm="$(sed -n 's/^sea-orm-cli=//p' "$STORE_DIR/TOOL_VERSIONS")"
actual_atlas="$(atlas version 2>/dev/null | sed -n 's/^atlas version v//p')"
actual_sea_orm="$(sea-orm-cli --version 2>/dev/null | sed -n 's/^sea-orm-cli //p')"
[[ "$actual_atlas" == "$expected_atlas" ]] || { fail "Atlas $expected_atlas is required; found ${actual_atlas:-unknown}."; exit 1; }
[[ "$actual_sea_orm" == "$expected_sea_orm" ]] || { fail "sea-orm-cli $expected_sea_orm is required; found ${actual_sea_orm:-unknown}."; exit 1; }
run atlas version
run sea-orm-cli --version

step "Step 2/4 — Build a temporary database at migration head"
cd "$STORE_DIR"
run atlas migrate apply --env local --config "$CONFIG_URI" --url "sqlite://$TEMP_DB"

step "Step 3/4 — Generate SeaORM entities"
run sea-orm-cli generate entity \
  --database-url "sqlite://$TEMP_DB" \
  --output-dir "$TEMP_ENTITIES" \
  --ignore-tables atlas_schema_revisions \
  --with-serde none

step "Step 4/4 — Normalize SQLite integer types and install output"
while IFS= read -r -d '' file; do
  sed -i.bak \
    -e 's/: i32,/: i64,/g' \
    -e 's/Option<i32>/Option<i64>/g' \
    "$file"
  rm -f "$file.bak"
done < <(find "$TEMP_ENTITIES" -type f -name '*.rs' -print0)
rm -rf "$ENTITIES_DIR"
mv "$TEMP_ENTITIES" "$ENTITIES_DIR"
success "Generated deterministic entities in $ENTITIES_DIR. Review and commit the result."
