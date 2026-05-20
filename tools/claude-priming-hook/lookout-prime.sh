#!/usr/bin/env sh
# lookout-prime.sh: emits priming context for Claude sessions, reminding
# the agent that lookout is available and worth pushing to.
#
# Hook contract:
#   - stdin: JSON with hook_event_name and event-specific fields.
#   - stdout: JSON {"hookSpecificOutput":{"hookEventName":...,"additionalContext":...}}
#     for events that support context injection (SessionStart, UserPromptSubmit,
#     PostToolUse). PreToolUse and SubagentStop don't support context, so we
#     fold subagent guidance into SessionStart and use PostToolUse(Agent) for
#     the post-return nudge.
set -eu

payload="$(cat || true)"
event="$(printf '%s' "$payload" | jq -r '.hook_event_name // empty' 2>/dev/null || true)"

emit() {
    jq -n --arg hook "$event" --arg ctx "$1" \
        '{hookSpecificOutput: {hookEventName: $hook, additionalContext: $ctx}}' \
        2>/dev/null || true
}

case "$event" in
    SessionStart)
        emit "Lookout is running at http://127.0.0.1:9477/mcp. It is an MCP visualizer TUI that accepts show_text, show_table, show_chart, show_tree, show_diff, show_log, show_image, show_progress, show_status, show_question. Push anything the user would want to glance at without cluttering this chat: structured findings, long outputs, in-flight progress, status of long-running work. See the lookout-companion skill for what counts as interesting and which card type fits when. Call set_session_label once early so this session is distinguishable in lookout. When dispatching a subagent for long-horizon work, push a show_status card naming the delegation first so the user can watch it unfold."
        ;;
    UserPromptSubmit)
        emit "Lookout reminder: push notable structured or visual output to lookout via show_* as you work. See lookout-companion if unsure what to push."
        ;;
    PostToolUse)
        # settings.json's matcher already filters to Agent in production; this
        # check makes the script self-defensive if anyone reuses or rewires it.
        tool="$(printf '%s' "$payload" | jq -r '.tool_name // empty' 2>/dev/null || true)"
        if [ "$tool" = "Agent" ]; then
            emit "A subagent just returned. If their result has anything worth glancing at (findings, structured data, diffs, summaries), push it to lookout as a card before folding it into your reply."
        fi
        ;;
    *) : ;;
esac

exit 0
