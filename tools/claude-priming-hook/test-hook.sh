#!/usr/bin/env sh
# Plain shell test suite for lookout-prime.sh. No bats required.
set -eu

HOOK="$(dirname "$0")/lookout-prime.sh"
FAIL=0

assert_contains() {
    needle="$1"
    haystack="$2"
    label="$3"
    case "$haystack" in
        *"$needle"*) echo "PASS: $label" ;;
        *) echo "FAIL: $label (expected to contain '$needle', got: $haystack)"; FAIL=1 ;;
    esac
}

# --- SessionStart ---
out="$(printf '{"hook_event_name":"SessionStart"}' | "$HOOK")"
assert_contains "127.0.0.1:9477/mcp" "$out" "SessionStart mentions lookout URL"
assert_contains "lookout-companion" "$out" "SessionStart references the skill"
assert_contains "set_session_label" "$out" "SessionStart mentions session label convention"
assert_contains "subagent" "$out" "SessionStart mentions subagent dispatch guidance"


# --- UserPromptSubmit ---
out="$(printf '{"hook_event_name":"UserPromptSubmit"}' | "$HOOK")"
assert_contains "lookout" "$out" "UserPromptSubmit mentions lookout"
assert_contains "show_" "$out" "UserPromptSubmit mentions show_* tools"
assert_contains "lookout-companion" "$out" "UserPromptSubmit references the skill"


# --- PostToolUse (subagent return) ---
out="$(printf '{"hook_event_name":"PostToolUse","tool_name":"Agent"}' | "$HOOK")"
assert_contains "subagent" "$out" "PostToolUse mentions subagent"
assert_contains "lookout" "$out" "PostToolUse mentions lookout"
assert_contains "worth glancing at" "$out" "PostToolUse frames the heuristic"

# --- Safety: unknown event ---
out="$(printf '{"hook_event_name":"NotARealEvent"}' | "$HOOK")"
if [ -z "$out" ]; then
    echo "PASS: unknown event emits nothing"
else
    echo "FAIL: unknown event should be silent (got: $out)"
    FAIL=1
fi

# --- Safety: empty stdin ---
if printf '' | "$HOOK" >/dev/null 2>&1; then
    echo "PASS: empty stdin exits 0"
else
    echo "FAIL: empty stdin should exit 0"
    FAIL=1
fi

# --- Safety: malformed JSON stdin ---
if printf 'not json' | "$HOOK" >/dev/null 2>&1; then
    echo "PASS: malformed stdin exits 0"
else
    echo "FAIL: malformed stdin should exit 0"
    FAIL=1
fi

exit "$FAIL"
