#!/usr/bin/env bash
set -euo pipefail

# ── Required env vars ──────────────────────────────────────────────
# ANTHROPIC_API_KEY  - Claude API key for Claude Code
# PULSE_API_URL      - Trace service URL (e.g. http://host.docker.internal:3000)
# PULSE_API_KEY      - API key for the trace service

for var in ANTHROPIC_API_KEY PULSE_API_URL PULSE_API_KEY; do
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

assert_match() {
  local label="$1" pattern="$2" actual="$3"
  if echo "$actual" | grep -qE "$pattern"; then
    echo "  PASS: $label"
    PASS=$((PASS + 1))
  else
    echo "  FAIL: $label (value '$actual' does not match pattern '$pattern')"
    FAIL=$((FAIL + 1))
  fi
}

# Helper to query spans from the trace service
query_spans() {
  curl -sf \
    -H "Authorization: Bearer $PULSE_API_KEY" \
    -H "X-Project-Id: e2e-test" \
    "$PULSE_API_URL/v1/spans?$1" 2>&1 || echo '{"spans":[]}'
}

# Helper to extract spans array from response (handles {spans:[]} or [] shapes)
extract_spans() {
  echo "$1" | jq 'if type == "array" then . elif .spans? then .spans elif .data? then .data else [] end'
}

# ── Step 1: Initialize pulse (non-interactive, skip health check) ──
echo "── Step 1: pulse init"
pulse init \
  --api-url "$PULSE_API_URL" \
  --api-key "$PULSE_API_KEY" \
  --project-id "e2e-test" \
  --no-validate

# ── Step 2: Connect hooks ─────────────────────────────────────────
echo "── Step 2: pulse connect"

# Create Claude settings file so connect can find it
mkdir -p ~/.claude
echo '{}' > ~/.claude/settings.json

CONNECT_OUTPUT=$(pulse connect 2>&1)
echo "$CONNECT_OUTPUT"

assert_eq "connect shows 10/10" "true" \
  "$(echo "$CONNECT_OUTPUT" | grep -q '10/10' && echo true || echo false)"

# ── Step 3: Verify status ─────────────────────────────────────────
echo "── Step 3: pulse status"
STATUS_OUTPUT=$(pulse status 2>&1)
echo "$STATUS_OUTPUT"

assert_eq "status shows 10/10" "true" \
  "$(echo "$STATUS_OUTPUT" | grep -q '10/10' && echo true || echo false)"

assert_eq "status shows trace service reachable" "true" \
  "$(echo "$STATUS_OUTPUT" | grep -q 'Trace service reachable' && echo true || echo false)"

# ── Step 4: Run Claude Code with a trivial prompt ─────────────────
echo "── Step 4: Running Claude Code"

# Record time before running so we can filter spans to this session
BEFORE_TS=$(date -u +%Y-%m-%dT%H:%M:%SZ)

# Use a minimal prompt that avoids tool use for speed/cost
# --allowedTools "" prevents any tool calls
CLAUDE_OUTPUT=$(claude -p "Reply with exactly: hello" --allowedTools "" 2>&1 || true)
echo "Claude output: $CLAUDE_OUTPUT"

# Give async spans time to land
sleep 3

# ── Step 5: Verify spans in trace service ─────────────────────────
echo "── Step 5: Verifying spans in trace service"
echo ""

RESPONSE=$(query_spans "limit=50")
ALL_SPANS=$(extract_spans "$RESPONSE")

# Filter to only spans from this test run (after BEFORE_TS)
SESSION_SPANS=$(echo "$ALL_SPANS" | jq --arg ts "$BEFORE_TS" \
  'map(select(.timestamp >= $ts))')
SESSION_COUNT=$(echo "$SESSION_SPANS" | jq 'length')

echo "  Total spans in DB: $(echo "$ALL_SPANS" | jq 'length')"
echo "  Spans from this session: $SESSION_COUNT"
echo ""

# ── 5a: Span count ────────────────────────────────────────────────
echo "  ── 5a: Span count"
# A no-tool-use session should produce exactly 4 spans:
# session_start, user_prompt_submit, stop, session_end
assert_eq "session produced 4 spans" "4" "$SESSION_COUNT"

# ── 5b: All spans share the same sessionId ────────────────────────
echo "  ── 5b: Session consistency"
UNIQUE_SESSIONS=$(echo "$SESSION_SPANS" | jq '[.[].sessionId] | unique | length')
assert_eq "all spans share one sessionId" "1" "$UNIQUE_SESSIONS"

SESSION_ID=$(echo "$SESSION_SPANS" | jq -r '.[0].sessionId')
assert_neq "sessionId is not empty" "" "$SESSION_ID"
assert_neq "sessionId is not null" "null" "$SESSION_ID"

# ── 5c: Required fields present on every span ─────────────────────
echo "  ── 5c: Required fields"
REQUIRED_FIELDS="spanId sessionId source kind eventType status timestamp"
for field in $REQUIRED_FIELDS; do
  MISSING=$(echo "$SESSION_SPANS" | jq --arg f "$field" \
    'map(select(.[$f] == null or .[$f] == "")) | length')
  assert_eq "all spans have $field" "0" "$MISSING"
done

# ── 5d: spanId is a valid UUID on every span ──────────────────────
echo "  ── 5d: UUID validation"
UUID_PATTERN='^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
SPAN_IDS=$(echo "$SESSION_SPANS" | jq -r '.[].spanId')
ALL_VALID="true"
while IFS= read -r sid; do
  if ! echo "$sid" | grep -qE "$UUID_PATTERN"; then
    ALL_VALID="false"
    echo "    invalid spanId: $sid"
  fi
done <<< "$SPAN_IDS"
assert_eq "all spanIds are valid UUIDs" "true" "$ALL_VALID"

# Verify all spanIds are unique
UNIQUE_SPAN_IDS=$(echo "$SESSION_SPANS" | jq '[.[].spanId] | unique | length')
assert_eq "all spanIds are unique" "$SESSION_COUNT" "$UNIQUE_SPAN_IDS"

# ── 5e: Timestamp validation ──────────────────────────────────────
echo "  ── 5e: Timestamp validation"
ISO_PATTERN='^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}'
TIMESTAMPS=$(echo "$SESSION_SPANS" | jq -r '.[].timestamp')
ALL_VALID_TS="true"
while IFS= read -r ts; do
  if ! echo "$ts" | grep -qE "$ISO_PATTERN"; then
    ALL_VALID_TS="false"
    echo "    invalid timestamp: $ts"
  fi
done <<< "$TIMESTAMPS"
assert_eq "all timestamps are valid ISO format" "true" "$ALL_VALID_TS"

# ── 5f: Source field ──────────────────────────────────────────────
echo "  ── 5f: Source validation"
NON_CC=$(echo "$SESSION_SPANS" | jq 'map(select(.source != "claude_code")) | length')
assert_eq "all spans have source=claude_code" "0" "$NON_CC"

# ── 5g: Event types present ───────────────────────────────────────
echo "  ── 5g: Event types"
for et in session_start user_prompt_submit stop session_end; do
  COUNT=$(echo "$SESSION_SPANS" | jq --arg et "$et" \
    'map(select(.eventType == $et)) | length')
  assert_eq "has exactly 1 $et span" "1" "$COUNT"
done

# ── 5h: Kind mappings are correct ─────────────────────────────────
echo "  ── 5h: Kind mappings"
# session_start, session_end, stop → "session"
# user_prompt_submit → "user_prompt"
for et in session_start session_end stop; do
  KIND=$(echo "$SESSION_SPANS" | jq -r --arg et "$et" \
    'map(select(.eventType == $et)) | .[0].kind')
  assert_eq "$et has kind=session" "session" "$KIND"
done

KIND=$(echo "$SESSION_SPANS" | jq -r \
  'map(select(.eventType == "user_prompt_submit")) | .[0].kind')
assert_eq "user_prompt_submit has kind=user_prompt" "user_prompt" "$KIND"

# ── 5i: Status mappings ──────────────────────────────────────────
echo "  ── 5i: Status validation"
NON_SUCCESS=$(echo "$SESSION_SPANS" | jq \
  'map(select(.status != "success")) | length')
assert_eq "all spans have status=success (no failures)" "0" "$NON_SUCCESS"

# ── 5j: Metadata validation ──────────────────────────────────────
echo "  ── 5j: Metadata validation"

# Every span should have cli_version in metadata
MISSING_CLI_VER=$(echo "$SESSION_SPANS" | jq \
  'map(select(.metadata.cli_version == null)) | length')
assert_eq "all spans have metadata.cli_version" "0" "$MISSING_CLI_VER"

# Every span should have project_id in metadata
MISSING_PROJECT=$(echo "$SESSION_SPANS" | jq \
  'map(select(.metadata.project_id == null)) | length')
assert_eq "all spans have metadata.project_id" "0" "$MISSING_PROJECT"

# project_id should be "e2e-test"
PROJECT_ID=$(echo "$SESSION_SPANS" | jq -r '.[0].metadata.project_id')
assert_eq "metadata.project_id is e2e-test" "e2e-test" "$PROJECT_ID"

# ── 5k: user_prompt_submit captured the prompt ────────────────────
echo "  ── 5k: user_prompt_submit content"
PROMPT_VALUE=$(echo "$SESSION_SPANS" | jq -r \
  'map(select(.eventType == "user_prompt_submit")) | .[0].metadata.prompt')
assert_eq "prompt captured in metadata" "Reply with exactly: hello" "$PROMPT_VALUE"

# ── 5l: session_end has a reason ──────────────────────────────────
echo "  ── 5l: session_end content"
END_REASON=$(echo "$SESSION_SPANS" | jq -r \
  'map(select(.eventType == "session_end")) | .[0].metadata.reason')
assert_neq "session_end has a reason" "null" "$END_REASON"
assert_neq "session_end reason is not empty" "" "$END_REASON"

# ── 5m: cwd is set on spans ──────────────────────────────────────
echo "  ── 5m: cwd validation"
HAS_CWD=$(echo "$SESSION_SPANS" | jq \
  'map(select(.cwd != null and .cwd != "")) | length')
assert_gte "at least one span has cwd set" 1 "$HAS_CWD"

# ── Step 6: Test disconnect ───────────────────────────────────────
echo ""
echo "── Step 6: pulse disconnect"
pulse disconnect
HOOKS_AFTER=$(cat ~/.claude/settings.json)
HAS_HOOKS=$(echo "$HOOKS_AFTER" | jq -r 'has("hooks") | tostring')
assert_eq "hooks removed after disconnect" "false" "$HAS_HOOKS"

# ── Summary ───────────────────────────────────────────────────────
echo ""
echo "══════════════════════════════════════"
echo "  Results: $PASS passed, $FAIL failed"
echo "══════════════════════════════════════"

if [ "$FAIL" -gt 0 ]; then
  exit 1
fi
