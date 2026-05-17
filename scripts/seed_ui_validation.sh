#!/usr/bin/env bash
set -euo pipefail

ROOT=${1:-$PWD}
cd "$ROOT"

if [[ ! -f Cargo.toml ]]; then
  echo "error: run from repo root or pass repo path as first arg" >&2
  exit 1
fi

echo "Seeding UI validation intents..."

create_intent() {
  local title="$1"
  local out
  out=$(cargo run -- --json graph create --kind feature --title "$title" --priority high)
  echo "$out" | awk -F '"' '/"id"/ {print $4; exit}'
}

INTENT_A=$(create_intent "UI Validation: Block-Unblock")
INTENT_B=$(create_intent "UI Validation: Review Transition")

cargo run -- graph update "$INTENT_A" --status approved >/dev/null
cargo run -- graph update "$INTENT_B" --status approved >/dev/null

cat <<EOF

Seed complete.

Intent IDs:
- $INTENT_A
- $INTENT_B

Validation URLs:
- http://127.0.0.1:3190/ui
- http://127.0.0.1:3190/ui/tasks/$INTENT_A
- http://127.0.0.1:3190/ui/review/$INTENT_A

Next:
1. Start UI: cargo run -- ui --port 3190
2. Open task detail URL for $INTENT_A
3. Verify Block/Unblock controls are visible in review panel
EOF
