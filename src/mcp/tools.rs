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
use serde_json::Value as JsonValue;
use tokio::sync::mpsc;

use chrono::DateTime;

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};

use crate::{
    card::{Card, CardId, CardKind, ChartKind, ChartSeries, CommonArgs, ImageSource, LogEntry, SessionId, StatusField, StatusStyle, TextFormat, TreeNode, Trend},
    imagepaths::ImagePathAllowlist,
    state::Command,
};

// ── Deserializers ────────────────────────────────────────────────────────────

/// Accept either a JSON number or a numeric string (some MCP clients
/// stringify scalars). Used by tools whose JSON Schema declares `number`
/// fields but in practice may receive `"0"` from typed-XML-parameter clients.
fn deserialize_lenient_f64<'de, D>(d: D) -> Result<f64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;
    let v = serde_json::Value::deserialize(d)?;
    coerce_f64(&v).ok_or_else(|| Error::custom(format!("expected number, got {v}")))
}

fn deserialize_lenient_opt_f64<'de, D>(d: D) -> Result<Option<f64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;
    let v = Option::<serde_json::Value>::deserialize(d)?;
    match v {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(val) => coerce_f64(&val)
            .map(Some)
            .ok_or_else(|| Error::custom(format!("expected number or null, got {val}"))),
    }
}

fn coerce_f64(v: &serde_json::Value) -> Option<f64> {
    match v {
        serde_json::Value::Number(n) => n.as_f64(),
        serde_json::Value::String(s) => s.parse::<f64>().ok(),
        _ => None,
    }
}

#[cfg(test)]
mod coerce_tests {
    use super::*;

    #[test]
    fn coerces_json_number() {
        assert_eq!(coerce_f64(&serde_json::json!(0.5)), Some(0.5));
        assert_eq!(coerce_f64(&serde_json::json!(0)), Some(0.0));
    }

    #[test]
    fn coerces_numeric_string() {
        assert_eq!(coerce_f64(&serde_json::json!("0")), Some(0.0));
        assert_eq!(coerce_f64(&serde_json::json!("3.14")), Some(3.14));
        assert_eq!(coerce_f64(&serde_json::json!("-2")), Some(-2.0));
    }

    #[test]
    fn rejects_non_numeric_string() {
        assert_eq!(coerce_f64(&serde_json::json!("abc")), None);
    }

    #[test]
    fn rejects_other_json_kinds() {
        assert_eq!(coerce_f64(&serde_json::json!(null)), None);
        assert_eq!(coerce_f64(&serde_json::json!(true)), None);
        assert_eq!(coerce_f64(&serde_json::json!([1])), None);
    }
}

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
pub struct ShowQuestionArgs {
    /// The question to ask.
    pub question: String,
    /// Multiple-choice options.
    #[serde(default)]
    pub options: Vec<String>,
    /// Optional context or explanation.
    #[serde(default)]
    pub context: Option<String>,
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

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ShowTableArgs {
    /// Either a list of row objects (column names inferred from union of keys, in first-seen order)...
    #[serde(default)]
    pub rows: Option<Vec<serde_json::Map<String, JsonValue>>>,
    /// ...or a CSV blob.
    #[serde(default)]
    pub csv: Option<String>,
    /// Optional explicit column order. Required if neither side has columns.
    #[serde(default)]
    pub columns: Option<Vec<String>>,
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

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ShowChartArgs {
    /// Chart kind: "line", "bar", "scatter", "sparkline", or "hist".
    pub kind: String,
    /// Series data.
    pub series: Vec<RawChartSeries>,
    /// X-axis label.
    #[serde(default)]
    pub x_label: Option<String>,
    /// Y-axis label.
    #[serde(default)]
    pub y_label: Option<String>,
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

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RawChartSeries {
    /// Series name.
    pub name: String,
    /// Either [[x,y], ...] points or a flat values array (x = index).
    #[serde(default)]
    pub points: Option<Vec<(f64, f64)>>,
    /// Flat values array; x-axis is the index.
    #[serde(default)]
    pub values: Option<Vec<f64>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ShowTreeArgs {
    /// Any JSON value, auto-rendered as a tree structure.
    #[serde(default)]
    pub data: Option<JsonValue>,
    /// Explicit hierarchical labels (alternative to `data`).
    #[serde(default)]
    pub nodes: Option<Vec<RawTreeNode>>,
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

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RawTreeNode {
    /// Label for this node.
    pub label: String,
    /// Child nodes.
    #[serde(default)]
    pub children: Vec<RawTreeNode>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ShowDiffArgs {
    /// The before string.
    pub before: String,
    /// The after string.
    pub after: String,
    /// Programming language for syntax highlighting.
    #[serde(default)]
    pub language: Option<String>,
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

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ShowImageArgs {
    /// Filesystem path to the image (resolved against the allowlist).
    #[serde(default)]
    pub path: Option<String>,
    /// Base64-encoded image bytes (alternative to `path`).
    #[serde(default)]
    pub base64: Option<String>,
    /// MIME type override (e.g. "image/png"). Auto-detected when omitted.
    #[serde(default)]
    pub mime: Option<String>,
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

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ShowProgressArgs {
    /// Unique identifier for this progress series; subsequent pushes with the same `id` replace the prior progress card in its pin slot.
    pub id: String,
    /// Short label describing what is being tracked.
    pub label: String,
    /// Current progress value (arbitrary numeric units).
    #[serde(deserialize_with = "deserialize_lenient_f64")]
    pub current: f64,
    /// Optional total value; if unset, progress is interpreted as an absolute quantity.
    #[serde(default, deserialize_with = "deserialize_lenient_opt_f64")]
    pub total: Option<f64>,
    /// Optional status text (e.g., "uploading", "complete").
    #[serde(default)]
    pub status: Option<String>,
    /// Optional card title.
    #[serde(default)]
    pub title: Option<String>,
    /// Session ID to target; defaults to the connection session.
    #[serde(default)]
    pub session: Option<String>,
    /// Pin-slot name to anchor this card. If unset, defaults to `progress:<id>`.
    #[serde(default)]
    pub pin: Option<String>,
    /// Freeform note attached to the card.
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UnpinArgs {
    /// Pin-slot name to release.
    pub slot: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ClearFeedArgs {}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SetSessionLabelArgs {
    /// Session ID to label.
    pub session: String,
    /// Display label for the session.
    pub label: String,
    /// Optional color slot (0..=15).
    #[serde(default)]
    pub color: Option<u8>,
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
    pub(crate) image_paths: ImagePathAllowlist,
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
        image_paths: ImagePathAllowlist,
    ) -> Self {
        Self {
            cmds,
            default_session,
            image_paths,
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
            .try_send(Command::PushCard(card.clone()))
            .map_err(|e| match e {
                tokio::sync::mpsc::error::TrySendError::Full(_) => crate::error::Error::Overloaded,
                tokio::sync::mpsc::error::TrySendError::Closed(_) => {
                    crate::error::Error::Internal("state task closed".into())
                }
            })?;
        Ok(card)
    }
}

// ── Tree helpers ──────────────────────────────────────────────────────

fn raw_to_node(r: RawTreeNode) -> TreeNode {
    TreeNode {
        label: r.label,
        children: r.children.into_iter().map(raw_to_node).collect(),
    }
}

fn json_to_node(label: &str, v: &JsonValue) -> TreeNode {
    match v {
        JsonValue::Object(map) => TreeNode {
            label: format!("{label} {{…}}"),
            children: map.iter().map(|(k, vv)| json_to_node(k, vv)).collect(),
        },
        JsonValue::Array(items) => TreeNode {
            label: format!("{label} [{}]", items.len()),
            children: items
                .iter()
                .enumerate()
                .map(|(i, vv)| json_to_node(&i.to_string(), vv))
                .collect(),
        },
        leaf => TreeNode {
            label: format!("{label}: {leaf}"),
            children: Vec::new(),
        },
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

    /// Push a question card — used when the agent wants to surface a decision point. Read-only here; the answer happens in the originating session.
    #[tool(description = "Push a question card — used when the agent wants to surface a decision point. Read-only here; the answer happens in the originating session.")]
    pub async fn show_question(
        &self,
        Parameters(args): Parameters<ShowQuestionArgs>,
    ) -> std::result::Result<CallToolResult, ErrorData> {
        let kind = CardKind::Question {
            question: args.question,
            options: args.options,
            context: args.context,
        };
        let common = CommonArgs {
            title: args.title,
            session: args.session,
            pin: args.pin,
            note: args.note,
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

    /// Push a table card. Provide `rows` (array of objects) or `csv` (string). `columns` overrides inferred order.
    #[tool(description = "Push a table card. Provide `rows` (array of objects) or `csv` (string). `columns` overrides inferred order.")]
    pub async fn show_table(
        &self,
        Parameters(args): Parameters<ShowTableArgs>,
    ) -> std::result::Result<CallToolResult, ErrorData> {
        let (columns, rows) = match (&args.rows, &args.csv, &args.columns) {
            (Some(rows), _, explicit) => infer_table_from_rows(rows, explicit.as_deref()),
            (None, Some(csv), explicit) => parse_csv(csv, explicit.as_deref())
                .map_err(|m| ErrorData::invalid_params(m, None))?,
            (None, None, _) => {
                return Err(ErrorData::invalid_params(
                    "must provide `rows` or `csv`",
                    None,
                ))
            }
        };
        let common = CommonArgs {
            title: args.title,
            session: args.session,
            pin: args.pin,
            note: args.note,
        };
        let card = self
            .push_card(common, CardKind::Table { columns, rows })
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        let CardId(uuid) = card.id;
        Ok(CallToolResult::success(vec![rmcp::model::Content::text(
            format!("ok:{uuid}"),
        )]))
    }

    /// Push a chart card. `kind` is one of line|bar|scatter|sparkline|hist. Each series provides points or a flat values array.
    #[tool(description = "Push a chart card. `kind` is one of line|bar|scatter|sparkline|hist. Each series provides points or a flat values array.")]
    pub async fn show_chart(
        &self,
        Parameters(args): Parameters<ShowChartArgs>,
    ) -> std::result::Result<CallToolResult, ErrorData> {
        let kind = match args.kind.as_str() {
            "line" => ChartKind::Line,
            "bar" => ChartKind::Bar,
            "scatter" => ChartKind::Scatter,
            "sparkline" => ChartKind::Sparkline,
            "hist" => ChartKind::Hist,
            other => {
                return Err(ErrorData::invalid_params(
                    format!("unknown chart kind '{other}'"),
                    None,
                ));
            }
        };
        let series: Vec<ChartSeries> = args
            .series
            .into_iter()
            .map(|s| {
                let points = match (s.points, s.values) {
                    (Some(p), _) => p,
                    (None, Some(v)) => v
                        .into_iter()
                        .enumerate()
                        .map(|(i, y)| (i as f64, y))
                        .collect(),
                    (None, None) => Vec::new(),
                };
                ChartSeries { name: s.name, points }
            })
            .collect();
        let common = CommonArgs {
            title: args.title,
            session: args.session,
            pin: args.pin,
            note: args.note,
        };
        let card = self
            .push_card(
                common,
                CardKind::Chart {
                    kind,
                    series,
                    x_label: args.x_label,
                    y_label: args.y_label,
                },
            )
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        let CardId(uuid) = card.id;
        Ok(CallToolResult::success(vec![rmcp::model::Content::text(
            format!("ok:{uuid}"),
        )]))
    }

    /// Push a tree card. Provide `data` (any JSON, auto-rendered as a tree) or `nodes` (explicit hierarchical labels).
    #[tool(description = "Push a tree card. Provide `data` (any JSON, auto-rendered as a tree) or `nodes` (explicit hierarchical labels).")]
    pub async fn show_tree(
        &self,
        Parameters(args): Parameters<ShowTreeArgs>,
    ) -> std::result::Result<CallToolResult, ErrorData> {
        let root = match (args.data, args.nodes) {
            (Some(v), _) => json_to_node("$", &v),
            (None, Some(nodes)) => TreeNode {
                label: "root".into(),
                children: nodes.into_iter().map(raw_to_node).collect(),
            },
            (None, None) => {
                return Err(ErrorData::invalid_params(
                    "must provide `data` or `nodes`",
                    None,
                ))
            }
        };
        let common = CommonArgs {
            title: args.title,
            session: args.session,
            pin: args.pin,
            note: args.note,
        };
        let card = self
            .push_card(common, CardKind::Tree { root })
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        let CardId(uuid) = card.id;
        Ok(CallToolResult::success(vec![rmcp::model::Content::text(
            format!("ok:{uuid}"),
        )]))
    }

    /// Push a unified diff card. Pass `before`/`after` strings; `language` enables syntax highlighting.
    #[tool(description = "Push a unified diff card. Pass `before`/`after` strings; `language` enables syntax highlighting.")]
    pub async fn show_diff(
        &self,
        Parameters(args): Parameters<ShowDiffArgs>,
    ) -> std::result::Result<CallToolResult, ErrorData> {
        let kind = CardKind::Diff {
            before: args.before,
            after: args.after,
            language: args.language,
        };
        let common = CommonArgs {
            title: args.title,
            session: args.session,
            pin: args.pin,
            note: args.note,
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

    /// Push an image card. Provide either `path` (resolved against allowlist) or `base64` (data inline). `mime` overrides auto-detection.
    #[tool(description = "Push an image card. Provide either `path` (resolved against allowlist) or `base64` (data inline). `mime` overrides auto-detection.")]
    pub async fn show_image(
        &self,
        Parameters(args): Parameters<ShowImageArgs>,
    ) -> std::result::Result<CallToolResult, ErrorData> {
        let (bytes, source) = match (args.path, args.base64) {
            (Some(p), _) => {
                let canon = self
                    .image_paths
                    .check(std::path::Path::new(&p))
                    .map_err(|e| ErrorData::invalid_params(e.to_string(), None))?;
                let bytes = std::fs::read(&canon)
                    .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
                (bytes, ImageSource::Path(canon))
            }
            (None, Some(b64)) => {
                let bytes = B64
                    .decode(b64.as_bytes())
                    .map_err(|e| ErrorData::invalid_params(format!("base64: {e}"), None))?;
                (bytes, ImageSource::Inline)
            }
            (None, None) => {
                return Err(ErrorData::invalid_params(
                    "must provide `path` or `base64`",
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
            .push_card(
                common,
                CardKind::Image {
                    bytes,
                    mime: args.mime,
                    source,
                },
            )
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        let CardId(uuid) = card.id;
        Ok(CallToolResult::success(vec![rmcp::model::Content::text(
            format!("ok:{uuid}"),
        )]))
    }

    /// Push a progress card. Subsequent pushes with the same `id` replace the prior progress card in its pin slot. `current` and optional `total` are arbitrary numeric units.
    #[tool(description = "Push a progress card. Subsequent pushes with the same `id` replace the prior progress card in its pin slot. `current` and optional `total` are arbitrary numeric units.")]
    pub async fn show_progress(
        &self,
        Parameters(args): Parameters<ShowProgressArgs>,
    ) -> std::result::Result<CallToolResult, ErrorData> {
        let kind = CardKind::Progress {
            progress_id: args.id,
            label: args.label,
            current: args.current,
            total: args.total,
            status: args.status,
        };
        let common = CommonArgs {
            title: args.title,
            session: args.session,
            pin: args.pin,
            note: args.note,
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

    /// Release a pin slot.
    #[tool(description = "Release a pin slot.")]
    pub async fn unpin(
        &self,
        Parameters(args): Parameters<UnpinArgs>,
    ) -> std::result::Result<CallToolResult, ErrorData> {
        self.cmds
            .try_send(Command::Unpin { slot: args.slot.clone() })
            .map_err(|e| match e {
                tokio::sync::mpsc::error::TrySendError::Full(_) => {
                    ErrorData::internal_error("lookout overloaded", None)
                }
                tokio::sync::mpsc::error::TrySendError::Closed(_) => {
                    ErrorData::internal_error("state task closed", None)
                }
            })?;
        Ok(CallToolResult::success(vec![rmcp::model::Content::text(
            format!("ok:{}", args.slot),
        )]))
    }

    /// Clear the feed. Does not affect pins.
    #[tool(description = "Clear the feed. Does not affect pins.")]
    pub async fn clear_feed(
        &self,
        Parameters(_args): Parameters<ClearFeedArgs>,
    ) -> std::result::Result<CallToolResult, ErrorData> {
        self.cmds
            .try_send(Command::ClearFeed)
            .map_err(|e| match e {
                tokio::sync::mpsc::error::TrySendError::Full(_) => {
                    ErrorData::internal_error("lookout overloaded", None)
                }
                tokio::sync::mpsc::error::TrySendError::Closed(_) => {
                    ErrorData::internal_error("state task closed", None)
                }
            })?;
        Ok(CallToolResult::success(vec![rmcp::model::Content::text("ok")]))
    }

    /// Give a session/connection a stable display label and optional color (0..=15).
    #[tool(description = "Give a session/connection a stable display label and optional color (0..=15).")]
    pub async fn set_session_label(
        &self,
        Parameters(args): Parameters<SetSessionLabelArgs>,
    ) -> std::result::Result<CallToolResult, ErrorData> {
        self.cmds
            .try_send(Command::SetSessionLabel {
                session: args.session.clone(),
                label: args.label.clone(),
                color: args.color,
            })
            .map_err(|e| match e {
                tokio::sync::mpsc::error::TrySendError::Full(_) => {
                    ErrorData::internal_error("lookout overloaded", None)
                }
                tokio::sync::mpsc::error::TrySendError::Closed(_) => {
                    ErrorData::internal_error("state task closed", None)
                }
            })?;
        Ok(CallToolResult::success(vec![rmcp::model::Content::text(
            format!("ok:{}", args.session),
        )]))
    }
}

// ── ServerHandler wiring ──────────────────────────────────────────────────────

#[tool_handler(router = self.tool_router)]
impl rmcp::ServerHandler for LookoutServer {}

// ── Table helpers ─────────────────────────────────────────────────────────

fn infer_table_from_rows(
    rows: &[serde_json::Map<String, JsonValue>],
    explicit: Option<&[String]>,
) -> (Vec<String>, Vec<Vec<JsonValue>>) {
    let columns: Vec<String> = match explicit {
        Some(c) => c.to_vec(),
        None => {
            let mut seen = Vec::new();
            for r in rows {
                for k in r.keys() {
                    if !seen.iter().any(|s: &String| s == k) {
                        seen.push(k.clone());
                    }
                }
            }
            seen
        }
    };
    let body: Vec<Vec<JsonValue>> = rows
        .iter()
        .map(|r| {
            columns
                .iter()
                .map(|c| r.get(c).cloned().unwrap_or(JsonValue::Null))
                .collect()
        })
        .collect();
    (columns, body)
}

fn parse_csv(
    csv: &str,
    explicit: Option<&[String]>,
) -> std::result::Result<(Vec<String>, Vec<Vec<JsonValue>>), String> {
    let mut lines = csv.lines();
    let header_line = lines
        .next()
        .ok_or_else(|| "csv is empty".to_string())?;
    let header: Vec<String> = match explicit {
        Some(cols) => cols.to_vec(),
        None => header_line.split(',').map(str::trim).map(String::from).collect(),
    };
    let body: Vec<Vec<JsonValue>> = lines
        .map(|line| {
            line.split(',')
                .map(|cell| JsonValue::String(cell.trim().to_string()))
                .collect()
        })
        .collect();
    Ok((header, body))
}

#[cfg(test)]
mod table_tests {
    use super::*;

    #[test]
    fn rows_infer_columns_in_first_seen_order() {
        let rows = vec![
            {
                let mut m = serde_json::Map::new();
                m.insert("id".into(), JsonValue::from(1));
                m.insert("name".into(), JsonValue::from("a"));
                m
            },
            {
                let mut m = serde_json::Map::new();
                m.insert("name".into(), JsonValue::from("b"));
                m.insert("score".into(), JsonValue::from(0.5));
                m
            },
        ];
        let (cols, body) = infer_table_from_rows(&rows, None);
        assert_eq!(cols, vec!["id", "name", "score"]);
        assert_eq!(body[0][0], JsonValue::from(1));
        assert_eq!(body[1][0], JsonValue::Null);
        assert_eq!(body[1][2], JsonValue::from(0.5));
    }

    #[test]
    fn csv_parses_simple_input() {
        let (cols, body) = parse_csv("a,b\n1,2\n3,4", None).unwrap();
        assert_eq!(cols, vec!["a", "b"]);
        assert_eq!(body.len(), 2);
        assert_eq!(body[0][0], JsonValue::from("1"));
    }
}
