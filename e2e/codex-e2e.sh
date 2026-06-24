#!/usr/bin/env bash
set -euo pipefail

for var in OPENAI_API_KEY PULSE_API_URL PULSE_API_KEY; do
  if [ -z "${!var:-}" ]; then
    echo "FAIL: $var is not set"
    exit 1
  fi
done

PASS=0
FAIL=0

assert_eq() {
  local label="$1" expected="$2" actual="$3"
  if [ "$expected" = "$actual" ]; then
    echo "  PASS: $label"
    PASS=$((PASS + 1))
  else
    echo "  FAIL: $label (expected=$expected, actual=$actual)"
    FAIL=$((FAIL + 1))
  fi
}

assert_neq() {
  local label="$1" unexpected="$2" actual="$3"
  if [ "$unexpected" != "$actual" ]; then
    echo "  PASS: $label"
    PASS=$((PASS + 1))
  else
    echo "  FAIL: $label (should not be $unexpected)"
    FAIL=$((FAIL + 1))
  fi
}

assert_gte() {
  local label="$1" min="$2" actual="$3"
  if [ "$actual" -ge "$min" ] 2>/dev/null; then
    echo "  PASS: $label ($actual >= $min)"
    PASS=$((PASS + 1))
  else
    echo "  FAIL: $label (expected >= $min, actual=$actual)"
    FAIL=$((FAIL + 1))
  fi
}

query_spans() {
  curl -sf \
    -H "Authorization: Bearer $PULSE_API_KEY" \
    -H "X-Project-Id: e2e-codex" \
    "$PULSE_API_URL/v1/spans?$1" 2>&1 || echo '{"spans":[]}'
}

extract_spans() {
  echo "$1" | jq 'if type == "array" then . elif .spans? then .spans elif .data? then .data else [] end'
}

export PULSE_DEBUG=1
export PULSE_DEBUG_LOG=/tmp/pulse-debug.log

echo "── Step 1: pulse init"
pulse init \
  --api-url "$PULSE_API_URL" \
  --api-key "$PULSE_API_KEY" \
  --project-id "e2e-codex" \
  --no-validate

echo "── Step 2: pulse install-hooks"
mkdir -p ~/.codex

CONNECT_OUTPUT=$(pulse install-hooks 2>&1)
echo "$CONNECT_OUTPUT"

assert_eq "install-hooks shows Codex 8/8" "true" \
  "$(echo "$CONNECT_OUTPUT" | grep -q 'Codex: hooks installed' && echo "$CONNECT_OUTPUT" | grep -q '8/8 hooks installed' && echo true || echo false)"

echo "── Step 3: pulse status"
STATUS_OUTPUT=$(pulse status 2>&1)
echo "$STATUS_OUTPUT"

assert_eq "status shows Codex connected" "true" \
  "$(echo "$STATUS_OUTPUT" | grep -q 'Codex: connected' && echo true || echo false)"
assert_eq "status shows 8/8 Codex hooks" "true" \
  "$(echo "$STATUS_OUTPUT" | grep -q '8/8 hooks installed' && echo true || echo false)"

echo "── Step 4: Running Codex"
BEFORE_TS=$(date -u +%Y-%m-%dT%H:%M:%SZ)

mkdir -p /workdir

CODEX_OUTPUT=$(cd /workdir && CODEX_API_KEY="$OPENAI_API_KEY" codex exec --skip-git-repo-check --dangerously-bypass-hook-trust \
  "Reply with exactly: hello" 2>&1 || true)
echo "Codex output: $CODEX_OUTPUT"

sleep 5

echo "── Step 5: Verifying spans in trace service"
RESPONSE=$(query_spans "limit=50")
ALL_SPANS=$(extract_spans "$RESPONSE")
SESSION_SPANS=$(echo "$ALL_SPANS" | jq --arg ts "$BEFORE_TS" \
  'def epoch: sub("\\.[0-9]+Z$"; "Z") | fromdateiso8601;
   map(select((.timestamp | epoch) >= ($ts | epoch) and .source == "codex"))')
SESSION_COUNT=$(echo "$SESSION_SPANS" | jq 'length')

echo "  Total spans in DB: $(echo "$ALL_SPANS" | jq 'length')"
echo "  Codex spans from this session: $SESSION_COUNT"

assert_gte "session produced Codex spans" 3 "$SESSION_COUNT"

UNIQUE_SESSIONS=$(echo "$SESSION_SPANS" | jq '[.[].sessionId] | unique | length')
assert_eq "all spans share one sessionId" "1" "$UNIQUE_SESSIONS"

SESSION_ID=$(echo "$SESSION_SPANS" | jq -r '.[0].sessionId // ""')
assert_neq "sessionId is not empty" "" "$SESSION_ID"

REQUIRED_FIELDS="spanId sessionId source kind eventType status timestamp"
for field in $REQUIRED_FIELDS; do
  MISSING=$(echo "$SESSION_SPANS" | jq --arg f "$field" \
    'map(select(.[$f] == null or .[$f] == "")) | length')
  assert_eq "all spans have $field" "0" "$MISSING"
done

NON_CODEX=$(echo "$SESSION_SPANS" | jq 'map(select(.source != "codex")) | length')
assert_eq "all spans have source=codex" "0" "$NON_CODEX"

for et in session_start user_prompt_submit stop; do
  COUNT=$(echo "$SESSION_SPANS" | jq --arg et "$et" \
    'map(select(.eventType == $et)) | length')
  assert_gte "has at least 1 $et span" 1 "$COUNT"
done

PROMPT_VALUE=$(echo "$SESSION_SPANS" | jq -r \
  'map(select(.eventType == "user_prompt_submit")) | .[0].metadata.prompt // ""')
assert_eq "prompt captured in metadata" "Reply with exactly: hello" "$PROMPT_VALUE"

MISSING_CLI_VER=$(echo "$SESSION_SPANS" | jq \
  'map(select(.metadata.cli_version == null)) | length')
assert_eq "all spans have metadata.cli_version" "0" "$MISSING_CLI_VER"

MISSING_PROJECT=$(echo "$SESSION_SPANS" | jq \
  'map(select(.metadata.project_id == null)) | length')
assert_eq "all spans have metadata.project_id" "0" "$MISSING_PROJECT"

PROJECT_ID=$(echo "$SESSION_SPANS" | jq -r '.[0].metadata.project_id')
assert_eq "metadata.project_id is e2e-codex" "e2e-codex" "$PROJECT_ID"

echo ""
echo "══════════════════════════════════════"
echo "  Results: $PASS passed, $FAIL failed"
echo "══════════════════════════════════════"

if [ "$FAIL" -gt 0 ]; then
  exit 1
fi
