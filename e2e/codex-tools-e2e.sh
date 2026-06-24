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
    -H "X-Project-Id: e2e-codex-tools" \
    "$PULSE_API_URL/v1/spans?$1" 2>&1 || echo '{"spans":[]}'
}

extract_spans() {
  echo "$1" | jq 'if type == "array" then . elif .spans? then .spans elif .data? then .data else [] end'
}

export PULSE_DEBUG=1
export PULSE_DEBUG_LOG=/tmp/pulse-debug.log

echo "── Setup: pulse init + connect"
pulse init \
  --api-url "$PULSE_API_URL" \
  --api-key "$PULSE_API_KEY" \
  --project-id "e2e-codex-tools" \
  --no-validate

mkdir -p ~/.codex
pulse install-hooks

mkdir -p /workdir
echo "Hello from the Codex e2e test file." > /workdir/codex-test.txt

echo ""
echo "── Step 1: Running Codex (tool calls)"
BEFORE_TS=$(date -u +%Y-%m-%dT%H:%M:%SZ)

CODEX_OUTPUT=$(cd /workdir && CODEX_API_KEY="$OPENAI_API_KEY" codex exec --skip-git-repo-check --dangerously-bypass-hook-trust \
  "Do these 2 things in order:
1. Read /workdir/codex-test.txt.
2. Run the shell command: echo CODEX_TOOL_TEST_OK.
After both, reply with DONE." 2>&1 || true)
echo "Codex output (last 40 lines):"
echo "$CODEX_OUTPUT" | tail -40

sleep 10

echo ""
echo "── Step 2: Querying spans"
RESPONSE=$(query_spans "limit=200")
ALL_SPANS=$(extract_spans "$RESPONSE")
SESSION_SPANS=$(echo "$ALL_SPANS" | jq --arg ts "$BEFORE_TS" \
  'def epoch: sub("\\.[0-9]+Z$"; "Z") | fromdateiso8601;
   map(select((.timestamp | epoch) >= ($ts | epoch) and .source == "codex"))')
SESSION_COUNT=$(echo "$SESSION_SPANS" | jq 'length')

echo "  Codex spans from this session: $SESSION_COUNT"
assert_gte "Codex produced spans" 3 "$SESSION_COUNT"

echo ""
echo "── Step 3: Event type coverage"
EVENT_TYPES=$(echo "$SESSION_SPANS" | jq -r '[.[].eventType] | unique | sort | .[]')
for et in $EVENT_TYPES; do
  COUNT=$(echo "$SESSION_SPANS" | jq --arg et "$et" 'map(select(.eventType == $et)) | length')
  echo "    $et: $COUNT"
done

for et in session_start user_prompt_submit stop; do
  COUNT=$(echo "$SESSION_SPANS" | jq --arg et "$et" 'map(select(.eventType == $et)) | length')
  assert_gte "has at least 1 $et" 1 "$COUNT"
done

for et in pre_tool_use post_tool_use; do
  COUNT=$(echo "$SESSION_SPANS" | jq --arg et "$et" 'map(select(.eventType == $et)) | length')
  assert_gte "has at least 1 $et" 1 "$COUNT"
done

echo ""
echo "── Step 4: Tool span field audit"
PRE_TOOL=$(echo "$SESSION_SPANS" | jq 'map(select(.eventType == "pre_tool_use")) | .[0]')
if [ "$PRE_TOOL" != "null" ]; then
  echo "  pre_tool_use sample:"
  echo "    toolName:  $(echo "$PRE_TOOL" | jq -r '.toolName // "NULL"')"
  echo "    toolInput: $(echo "$PRE_TOOL" | jq -c '.toolInput // "NULL"' | head -c 200)"

  assert_neq "pre_tool_use has toolName" "NULL" "$(echo "$PRE_TOOL" | jq -r '.toolName // "NULL"')"
  assert_neq "pre_tool_use has toolInput" "NULL" "$(echo "$PRE_TOOL" | jq -r '.toolInput // "NULL"')"
fi

POST_TOOL=$(echo "$SESSION_SPANS" | jq 'map(select(.eventType == "post_tool_use")) | .[0]')
if [ "$POST_TOOL" != "null" ]; then
  echo ""
  echo "  post_tool_use sample:"
  echo "    toolName:     $(echo "$POST_TOOL" | jq -r '.toolName // "NULL"')"
  echo "    toolResponse: $(echo "$POST_TOOL" | jq -c '.toolResponse // "NULL"' | head -c 200)"

  assert_neq "post_tool_use has toolName" "NULL" "$(echo "$POST_TOOL" | jq -r '.toolName // "NULL"')"
  assert_neq "post_tool_use has toolResponse" "NULL" "$(echo "$POST_TOOL" | jq -r '.toolResponse // "NULL"')"
fi

echo ""
echo "── Step 5: Raw payloads from Codex"
if [ -f "$PULSE_DEBUG_LOG" ]; then
  cat "$PULSE_DEBUG_LOG"
else
  echo "  No debug log found"
fi

echo ""
echo "══════════════════════════════════════"
echo "  Results: $PASS passed, $FAIL failed"
echo "══════════════════════════════════════"

if [ "$FAIL" -gt 0 ]; then
  exit 1
fi
