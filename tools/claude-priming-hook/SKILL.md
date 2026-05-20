---
name: lookout-companion
description: Use when you produce structured, visual, or long-running output that would be easier for the user to scan in a side view than inline. Catalogs lookout's card types and when each fits, plus heuristics for what is worth pushing vs. what is noise.
---

# lookout-companion

Lookout is a TUI visualizer running at http://127.0.0.1:9477/mcp. The user
is watching it in another pane. Push things they would want to glance at;
keep this chat for prose.

## What to push

PUSH:
- Structured findings (tables, trees, diffs). Anything that reads better
  in 2D than in chat.
- In-flight progress. `show_progress` updated in place.
- Status of multi-step work. `show_status` with key/value fields.
- Subagent dispatch and return. So the user can follow long-horizon work.
- Big outputs that would otherwise dominate the chat (log dumps, build
  output, search results).

DO NOT PUSH:
- Trivial confirmations ("read file X", "ran command Y").
- Your own prose. Responses belong in the conversation.
- Secrets, credentials, `.env` contents.
- Things the user just directly asked a question about. Answer in chat.

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

Push if the user would benefit from seeing it without scrolling back
through this chat. Skip if it belongs in the conversation itself.
