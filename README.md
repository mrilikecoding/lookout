# lookout

Streaming visualizer with an MCP interface — a long-running terminal pane any
MCP client can push cards into (tables, charts, trees, diffs, logs, images,
progress, status, questions).

The intent: when an agent is doing fast or noisy work whose results don't
render well as text in the active session, it proxies output to lookout
instead. Glanceable in one terminal, pinnable for current state, no context
inflation in the originating chat.

## Run

    cargo run                   # listens on http://127.0.0.1:9477/mcp
    cargo run -- --port 9999    # different port
    cargo run --example smoke   # push one of every card type to a running server

## Connect from an MCP client

Add to your client's MCP config (example fragment):

    {
      "mcpServers": {
        "lookout": { "type": "http", "url": "http://127.0.0.1:9477/mcp" }
      }
    }

Lookout exposes 13 tools: `show_text`, `show_table`, `show_chart`, `show_tree`,
`show_diff`, `show_log`, `show_image`, `show_progress`, `show_status`,
`show_question`, plus controls `unpin`, `clear_feed`, `set_session_label`.

## Keybindings

    j / k or arrows   move focus
    o / Enter         expand / collapse focused card
    Esc               collapse / cancel filter prompt
    p / P             pin / unpin focused card
    /                 filter prompt (title substring)
    1–9               toggle session-chip filters
    c                 clear feed (pins remain)
    q / Ctrl-C        quit (graceful drain)

## License

MIT
