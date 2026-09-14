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

step "Step 1/3 — Check Atlas CLI"
command -v atlas >/dev/null 2>&1 || { fail "Atlas CLI is required: https://atlasgo.io/getting-started#installation"; exit 1; }
expected_atlas="$(sed -n 's/^atlas=//p' "$STORE_DIR/TOOL_VERSIONS")"
actual_atlas="$(atlas version 2>/dev/null | sed -n 's/^atlas version v//p')"
[[ "$actual_atlas" == "$expected_atlas" ]] || { fail "Atlas $expected_atlas is required; found ${actual_atlas:-unknown}."; exit 1; }
run atlas version

step "Step 2/3 — Validate migration checksums"
cd "$STORE_DIR"
run atlas migrate validate --env local --config "$CONFIG_URI"

step "Step 3/3 — Lint the latest migration"
run atlas migrate lint --env local --latest 1 --config "$CONFIG_URI"
success "Migration checksums and lint passed."
