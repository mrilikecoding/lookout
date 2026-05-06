//! MCP tool handlers.  LookoutServer is the single rmcp `ServerHandler` impl;
//! all `show_*` tools funnel through `push_card`.

use std::sync::Arc;

use rmcp::{
    ErrorData,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::CallToolResult,
    schemars, tool, tool_handler, tool_router,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use chrono::DateTime;

use crate::{
    card::{Card, CardId, CardKind, CommonArgs, LogEntry, SessionId, StatusField, StatusStyle, TextFormat, Trend},
    state::Command,
};

// ── Args ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ShowTextArgs {
    /// The text content to display.
    pub content: String,
    /// Rendering hint: "plain", "markdown", or "code". Defaults to "plain".
    pub format: Option<String>,
    /// Programming language for code blocks (only meaningful when format = "code").
    pub language: Option<String>,
    /// Optional card title.
    pub title: Option<String>,
    /// Session ID to target; defaults to the connection session.
    pub session: Option<String>,
    /// Pin-slot name to anchor this card.
    pub pin: Option<String>,
    /// Freeform note attached to the card.
    pub note: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ShowLogArgs {
    /// Pre-structured log entries.
    #[serde(default)]
    pub entries: Option<Vec<RawLogEntry>>,
    /// Freeform text blob: split into one entry per line.
    #[serde(default)]
    pub text: Option<String>,
    /// Optional card title.
    #[serde(default)]
    pub title: Option<String>,
    /// Session ID to target; defaults to the connection session.
    #[serde(default)]
    pub session: Option<String>,
    /// Pin-slot name to anchor this card.
    #[serde(default)]
    pub pin: Option<String>,
    /// Freeform note attached to the card.
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct RawLogEntry {
    /// ISO 8601 timestamp.
    #[serde(default)]
    pub ts: Option<String>,
    /// Log level (e.g., "info", "warn", "error").
    #[serde(default)]
    pub level: Option<String>,
    /// Source identifier.
    #[serde(default)]
    pub source: Option<String>,
    /// Log message.
    pub msg: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ShowStatusArgs {
    /// Key-value fields to display.
    pub fields: Vec<RawStatusField>,
    /// Optional card title.
    #[serde(default)]
    pub title: Option<String>,
    /// Session ID to target; defaults to the connection session.
    #[serde(default)]
    pub session: Option<String>,
    /// Pin-slot name to anchor this card.
    #[serde(default)]
    pub pin: Option<String>,
    /// Freeform note attached to the card.
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct RawStatusField {
    /// Label for the field.
    pub label: String,
    /// Value to display.
    pub value: String,
    /// Trend indicator: "up", "down", or "flat".
    #[serde(default)]
    pub trend: Option<String>,
    /// Style hint: "good", "warn", or "bad".
    #[serde(default)]
    pub style: Option<String>,
}

// ── LookoutServer ─────────────────────────────────────────────────────────────

/// The rmcp `ServerHandler` that dispatches MCP tool calls into the state task.
#[derive(Clone)]
pub struct LookoutServer {
    pub(crate) cmds: mpsc::Sender<Command>,
    pub(crate) default_session: Arc<dyn Fn() -> SessionId + Send + Sync>,
    tool_router: ToolRouter<Self>,
}

impl std::fmt::Debug for LookoutServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LookoutServer").finish_non_exhaustive()
    }
}

impl LookoutServer {
    pub fn new(
        cmds: mpsc::Sender<Command>,
        default_session: Arc<dyn Fn() -> SessionId + Send + Sync>,
    ) -> Self {
        Self {
            cmds,
            default_session,
            tool_router: Self::tool_router(),
        }
    }

    /// Build a card, send it to the state task, and return it.
    pub(crate) async fn push_card(
        &self,
        common: CommonArgs,
        kind: CardKind,
    ) -> std::result::Result<Card, crate::error::Error> {
        let card = Card::build(common, (self.default_session)(), kind);
        self.cmds
            .send(Command::PushCard(card.clone()))
            .await
            .map_err(|_| crate::error::Error::Internal("state task has shut down".into()))?;
        Ok(card)
    }
}

// ── Tool implementations ──────────────────────────────────────────────────────

#[tool_router]
impl LookoutServer {
    /// Display a block of text in the lookout feed.
    #[tool(description = "Display text content in the lookout feed.")]
    async fn show_text(
        &self,
        Parameters(args): Parameters<ShowTextArgs>,
    ) -> std::result::Result<CallToolResult, ErrorData> {
        let format = match args.format.as_deref().unwrap_or("plain") {
            "plain" => TextFormat::Plain,
            "markdown" => TextFormat::Markdown,
            "code" => TextFormat::Code,
            other => {
                return Err(ErrorData::invalid_params(
                    format!(
                        "unknown format {:?}: expected \"plain\", \"markdown\", or \"code\"",
                        other
                    ),
                    None,
                ));
            }
        };

        let common = CommonArgs {
            title: args.title,
            session: args.session,
            pin: args.pin,
            note: args.note,
        };
        let kind = CardKind::Text {
            content: args.content,
            format,
            language: args.language,
        };

        let card = self
            .push_card(common, kind)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        let CardId(uuid) = card.id;
        Ok(CallToolResult::success(vec![rmcp::model::Content::text(
            format!("ok:{uuid}"),
        )]))
    }

    /// Push a log card. Pass either `entries` (structured) or `text` (one entry per line).
    #[tool(description = "Push a log card. Pass either `entries` (structured) or `text` (one entry per line).")]
    async fn show_log(
        &self,
        Parameters(args): Parameters<ShowLogArgs>,
    ) -> std::result::Result<CallToolResult, ErrorData> {
        let entries: Vec<LogEntry> = match (args.entries, args.text) {
            (Some(es), _) => es
                .into_iter()
                .map(|e| LogEntry {
                    ts: e.ts.and_then(|s| DateTime::parse_from_rfc3339(&s).ok().map(|d| d.to_utc())),
                    level: e.level,
                    source: e.source,
                    msg: e.msg,
                })
                .collect(),
            (None, Some(text)) => text
                .lines()
                .map(|l| LogEntry {
                    ts: None,
                    level: None,
                    source: None,
                    msg: l.to_string(),
                })
                .collect(),
            (None, None) => {
                return Err(ErrorData::invalid_params(
                    "must provide `entries` or `text`",
                    None,
                ));
            }
        };
        let common = CommonArgs {
            title: args.title,
            session: args.session,
            pin: args.pin,
            note: args.note,
        };
        let card = self
            .push_card(common, CardKind::Log { entries })
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        let CardId(uuid) = card.id;
        Ok(CallToolResult::success(vec![rmcp::model::Content::text(
            format!("ok:{uuid}"),
        )]))
    }

    /// Push a status card: a compact key-value grid with optional trend arrows and severity styles.
    #[tool(description = "Push a status card: a compact key-value grid with optional trend arrows and severity styles.")]
    async fn show_status(
        &self,
        Parameters(args): Parameters<ShowStatusArgs>,
    ) -> std::result::Result<CallToolResult, ErrorData> {
        fn parse_trend(s: Option<String>) -> std::result::Result<Option<Trend>, String> {
            match s.as_deref() {
                None => Ok(None),
                Some("up") => Ok(Some(Trend::Up)),
                Some("down") => Ok(Some(Trend::Down)),
                Some("flat") => Ok(Some(Trend::Flat)),
                Some(other) => Err(format!("unknown trend '{other}'")),
            }
        }
        fn parse_style(s: Option<String>) -> std::result::Result<Option<StatusStyle>, String> {
            match s.as_deref() {
                None => Ok(None),
                Some("good") => Ok(Some(StatusStyle::Good)),
                Some("warn") => Ok(Some(StatusStyle::Warn)),
                Some("bad") => Ok(Some(StatusStyle::Bad)),
                Some(other) => Err(format!("unknown style '{other}'")),
            }
        }
        let mut fields = Vec::with_capacity(args.fields.len());
        for f in args.fields {
            let trend = parse_trend(f.trend).map_err(|m| ErrorData::invalid_params(m, None))?;
            let style = parse_style(f.style).map_err(|m| ErrorData::invalid_params(m, None))?;
            fields.push(StatusField {
                label: f.label,
                value: f.value,
                trend,
                style,
            });
        }
        let common = CommonArgs {
            title: args.title,
            session: args.session,
            pin: args.pin,
            note: args.note,
        };
        let card = self
            .push_card(common, CardKind::Status { fields })
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        let CardId(uuid) = card.id;
        Ok(CallToolResult::success(vec![rmcp::model::Content::text(
            format!("ok:{uuid}"),
        )]))
    }
}

// ── ServerHandler wiring ──────────────────────────────────────────────────────

#[tool_handler(router = self.tool_router)]
impl rmcp::ServerHandler for LookoutServer {}
