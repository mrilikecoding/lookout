---
name: lookout-companion
description: Use when you produce structured, visual, or long-running output that would be easier for the user to scan in a side view than inline. Catalogs lookout's card types and when each fits, plus heuristics for what is worth pushing vs. what is noise.
---

# lookout-companion

Lookout is a TUI visualizer running at http://127.0.0.1:9477/mcp. The user
is watching it in another pane. Push things they would want to glance at;
keep this chat for prose.

## Triggers (push on every occurrence)

These rules override judgment. Don't ask "is this interesting?"; when a
trigger fires, push.

1. **Git operations.** Before any commit, merge, branch, or push: push a
   `show_status` card with the action you're about to take and current
   branch state (ahead/behind, dirty files). Update the same card after
   the operation completes.
2. **Tests and builds.** Whenever you run a test suite or build command,
   push the results as `show_log` (raw output) or `show_status` (summary:
   pass/fail counts, duration). Reuse the same `pin` slot to update in
   place rather than spamming new cards.
3. **Structured output.** Whenever you would produce a table, tree, diff,
   or list of more than ~5 items in chat, push it as the matching
   `show_*` card. The chat gets a one-line summary; lookout gets the
   structured view.
4. **Subagent dispatch.** Before you fire a subagent: push a `show_status`
   naming what you're delegating and to whom. After the subagent returns:
   push their findings as a card before integrating into your reply.
5. **Multi-step tasks.** When starting a task with more than ~3 substeps:
   push a `show_status` listing the steps and their state. Update it in
   place as you advance.

## Don't push

- Trivial confirmations ("read file X", "ran ls").
- Your own prose. Conversation stays in chat.
- Secrets, credentials, `.env` contents.
- The user's direct question; answer in chat first.

## Card types

- `show_text`: text/markdown/code blob. Use for prose findings or code
  snippets the user might want pinned.
- `show_table`: rows x columns of comparable data. Use for call sites,
  test failures, PR lists, file enumerations with attributes.
- `show_chart`: time-series or bar chart. Use for metrics over time.
- `show_tree`: hierarchical structure. Use for file trees, AST snippets,
  dependency graphs.
- `show_diff`: code/text diff. Use for changes the user should scan
  before approving.
- `show_log`: appendable log lines. Use for streaming build/test output.
- `show_image`: PNG/JPG from a local path. Use for screenshots,
  generated charts, design assets.
- `show_progress`: single value 0..1 with optional label. Use for
  long-running work with a known fraction complete.
- `show_status`: multi-field key/value status. Use for "current state
  of the long-running thing" (e.g., deploy stage, test pass/fail counts,
  active session info).
- `show_question`: glanceable yes/no for the user. Use sparingly, only
  when a decision unblocks you.

## Conventions

- Call `set_session_label` once near session start with a short human
  label that names the work (e.g., "auth refactor", "lookout hook
  design"). This makes parallel sessions distinguishable in lookout.
- Reuse `card_id` to update an existing card rather than spamming new
  ones. Progress and status cards in particular should update in place.
- Push summaries, not firehoses. 14 callers belong in one `show_table`,
  not 14 separate cards.
- Do not try to anticipate which card the user will pin. Just make sure
  something pin-worthy exists.

## When in doubt

Rules first. Judgment only for cases the rules don't cover.
