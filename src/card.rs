//! Card — the unit of content displayed in lookout.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CardId(pub Uuid);

impl CardId {
    pub fn new() -> Self {
        CardId(Uuid::new_v4())
    }
}

impl Default for CardId {
    fn default() -> Self {
        Self::new()
    }
}

pub type SessionId = String;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Card {
    pub id: CardId,
    pub created_at: DateTime<Utc>,
    pub session: SessionId,
    pub title: Option<String>,
    pub note: Option<String>,
    pub pin_slot: Option<String>,
    pub kind: CardKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CardKind {
    Text {
        content: String,
        format: TextFormat,
        language: Option<String>,
    },
    Table {
        columns: Vec<String>,
        rows: Vec<Vec<JsonValue>>,
    },
    Chart {
        kind: ChartKind,
        series: Vec<ChartSeries>,
        x_label: Option<String>,
        y_label: Option<String>,
    },
    Tree {
        root: TreeNode,
    },
    Diff {
        before: String,
        after: String,
        language: Option<String>,
    },
    Log {
        entries: Vec<LogEntry>,
    },
    Image {
        bytes: Vec<u8>,
        mime: Option<String>,
        source: ImageSource,
    },
    Progress {
        progress_id: String,
        label: String,
        current: f64,
        total: Option<f64>,
        status: Option<String>,
    },
    Status {
        fields: Vec<StatusField>,
    },
    Question {
        question: String,
        options: Vec<String>,
        context: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TextFormat {
    Plain,
    Markdown,
    Code,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChartKind {
    Line,
    Bar,
    Scatter,
    Sparkline,
    Hist,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChartSeries {
    pub name: String,
    pub points: Vec<(f64, f64)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeNode {
    pub label: String,
    #[serde(default)]
    pub children: Vec<TreeNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub ts: Option<DateTime<Utc>>,
    pub level: Option<String>,
    pub source: Option<String>,
    pub msg: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ImageSource {
    Path(std::path::PathBuf),
    Inline,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusField {
    pub label: String,
    pub value: String,
    #[serde(default)]
    pub trend: Option<Trend>,
    #[serde(default)]
    pub style: Option<StatusStyle>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Trend {
    Up,
    Down,
    Flat,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StatusStyle {
    Good,
    Warn,
    Bad,
}

/// Common arguments shared by every `show_*` MCP tool.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CommonArgs {
    pub title: Option<String>,
    pub session: Option<String>,
    pub pin: Option<String>,
    pub note: Option<String>,
}

impl Card {
    /// Construct a Card from a CardKind plus the common args resolved from the
    /// MCP layer (with `session` defaulted to the connection-derived id).
    pub fn build(common: CommonArgs, default_session: SessionId, kind: CardKind) -> Self {
        Self {
            id: CardId::new(),
            created_at: Utc::now(),
            session: common.session.unwrap_or(default_session),
            title: common.title,
            note: common.note,
            pin_slot: common.pin,
            kind,
        }
    }

    /// Convenience: progress cards auto-target slot `progress:<id>` unless
    /// `pin` was explicitly set.
    pub fn auto_pin_slot(&self) -> Option<String> {
        if self.pin_slot.is_some() {
            return self.pin_slot.clone();
        }
        if let CardKind::Progress { progress_id, .. } = &self.kind {
            return Some(format!("progress:{progress_id}"));
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn common(pin: Option<&str>) -> CommonArgs {
        CommonArgs {
            title: Some("t".into()),
            session: None,
            pin: pin.map(str::to_string),
            note: None,
        }
    }

    #[test]
    fn card_build_uses_default_session_when_missing() {
        let c = Card::build(
            common(None),
            "default".into(),
            CardKind::Text {
                content: "hi".into(),
                format: TextFormat::Plain,
                language: None,
            },
        );
        assert_eq!(c.session, "default");
        assert_eq!(c.title.as_deref(), Some("t"));
    }

    #[test]
    fn progress_without_pin_resolves_to_progress_slot() {
        let c = Card::build(
            common(None),
            "s".into(),
            CardKind::Progress {
                progress_id: "deploy".into(),
                label: "uploading".into(),
                current: 0.5,
                total: Some(1.0),
                status: None,
            },
        );
        assert_eq!(c.auto_pin_slot().as_deref(), Some("progress:deploy"));
    }

    #[test]
    fn explicit_pin_overrides_progress_default() {
        let c = Card::build(
            common(Some("custom_slot")),
            "s".into(),
            CardKind::Progress {
                progress_id: "deploy".into(),
                label: "x".into(),
                current: 0.0,
                total: None,
                status: None,
            },
        );
        assert_eq!(c.auto_pin_slot().as_deref(), Some("custom_slot"));
    }

    #[test]
    fn non_progress_card_without_pin_has_no_auto_slot() {
        let c = Card::build(
            common(None),
            "s".into(),
            CardKind::Text {
                content: "x".into(),
                format: TextFormat::Plain,
                language: None,
            },
        );
        assert_eq!(c.auto_pin_slot(), None);
    }

    #[test]
    fn card_kind_serde_round_trip() {
        let kind = CardKind::Status {
            fields: vec![StatusField {
                label: "p95".into(),
                value: "84ms".into(),
                trend: Some(Trend::Up),
                style: Some(StatusStyle::Good),
            }],
        };
        let s = serde_json::to_string(&kind).unwrap();
        let back: CardKind = serde_json::from_str(&s).unwrap();
        match back {
            CardKind::Status { fields } => {
                assert_eq!(fields[0].label, "p95");
                assert_eq!(fields[0].trend, Some(Trend::Up));
            }
            _ => panic!("wrong variant"),
        }
    }
}
