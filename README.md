# lookout

Streaming visualizer with an MCP interface — a long-running terminal pane any
MCP client can push cards into (tables, charts, trees, diffs, logs, images,
progress, status, questions).

The intent: when an agent is doing fast or noisy work whose results don't
render well as text in the active session, it proxies output to lookout
instead. Glanceable in one terminal, pinnable for current state, no context
inflation in the originating chat.

## Run modes

    cargo run                   # default: server + TUI in one process
    cargo run -- serve          # headless server, no TUI; ideal as a long-running background process
    cargo run -- view           # attach a TUI to a running `serve` over SSE
    cargo run -- view --url http://127.0.0.1:9477   # explicit serve URL

The default mode (no subcommand) keeps today's behavior: one process
runs the MCP server and the TUI together.

`serve` accepts MCP traffic identically but renders no TUI. Run it at
login (LaunchAgent / systemd-user-unit) and attach `view` ad hoc when
you want to watch. Multiple `view` processes can attach concurrently.

To push test cards: `cargo run --example smoke` (against any of the
modes).

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

    Tab               toggle focus region (Pins ↔ Feed)
    j / k or arrows   move focus within active region
    o / Enter         expand focused card / zoom focused pin
    Esc               exit zoom / collapse expand / cancel filter prompt
    x                 remove focused pin (Pins region)
    Alt-1 … Alt-9     remove pin by visible index
    p / P             pin / unpin focused feed card
    g                 toggle feed: ticker (3 lines) ↔ expanded (~14 lines)
    G                 jump focus into the feed (and expand it)
    /                 filter prompt (title substring)
    1–9               toggle session-chip filters
    c                 clear feed (pins remain)
    q / Ctrl-C        quit (graceful drain)

## Claude Code integration

If you use Claude Code, see `tools/claude-priming-hook/` for an
installable hook + skill that keeps Claude reminded to push glanceable
state to lookout as it works.

## License

MIT
