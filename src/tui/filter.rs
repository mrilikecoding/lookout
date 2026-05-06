use crate::card::{Card, CardKind};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use std::collections::HashSet;

#[derive(Debug, Clone, Default)]
pub struct FilterState {
    /// If non-empty, only sessions in this set are shown.
    pub sessions: HashSet<String>,
    /// If non-empty, only kinds in this set are shown.
    pub kinds: HashSet<String>,
    pub query: Option<String>,
}

impl FilterState {
    pub fn matches(&self, c: &Card) -> bool {
        if !self.sessions.is_empty() && !self.sessions.contains(&c.session) {
            return false;
        }
        if !self.kinds.is_empty() {
            let k = match c.kind {
                CardKind::Text { .. } => "text",
                CardKind::Table { .. } => "table",
                CardKind::Chart { .. } => "chart",
                CardKind::Tree { .. } => "tree",
                CardKind::Diff { .. } => "diff",
                CardKind::Log { .. } => "log",
                CardKind::Image { .. } => "image",
                CardKind::Progress { .. } => "progress",
                CardKind::Status { .. } => "status",
                CardKind::Question { .. } => "question",
            };
            if !self.kinds.contains(k) {
                return false;
            }
        }
        if let Some(q) = &self.query {
            if let Some(t) = &c.title {
                if !t.to_lowercase().contains(&q.to_lowercase()) {
                    return false;
                }
            } else {
                return false;
            }
        }
        true
    }
}

pub fn render(
    f: &mut Frame,
    area: Rect,
    all_sessions: &[String],
    state: &FilterState,
    prompt: Option<&str>,
) {
    // When in filter-prompt mode, show the typed query with a cursor instead of chips.
    if let Some(s) = prompt {
        let bar = Paragraph::new(Line::from(vec![
            Span::raw("Filter: /"),
            Span::styled(s.to_string(), Style::default().fg(Color::Yellow)),
            Span::styled("_", Style::default().add_modifier(Modifier::RAPID_BLINK)),
            Span::raw("    Enter confirm  Esc cancel"),
        ]));
        f.render_widget(bar, area);
        return;
    }

    let mut spans = vec![Span::raw("Filter: ")];
    let all = state.sessions.is_empty();
    spans.push(Span::styled(
        if all { "[all] " } else { " all  " }.to_string(),
        if all {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default().fg(Color::DarkGray)
        },
    ));
    for s in all_sessions {
        let active = state.sessions.contains(s);
        spans.push(Span::styled(
            format!("{s} "),
            if active {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            },
        ));
    }
    if let Some(q) = &state.query {
        spans.push(Span::raw(format!(" /{q} ")));
    }
    spans.push(Span::raw("    j/k  o expand  p pin  / filter  q quit"));
    let p = Paragraph::new(Line::from(spans));
    f.render_widget(p, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::{CardKind, CommonArgs, TextFormat};

    fn card(session: &str, title: Option<&str>) -> Card {
        Card::build(
            CommonArgs {
                title: title.map(str::to_string),
                session: Some(session.into()),
                ..Default::default()
            },
            "default".into(),
            CardKind::Text {
                content: "x".into(),
                format: TextFormat::Plain,
                language: None,
            },
        )
    }

    #[test]
    fn empty_filter_matches_all() {
        let f = FilterState::default();
        assert!(f.matches(&card("a", None)));
    }

    #[test]
    fn session_filter_excludes_non_members() {
        let mut f = FilterState::default();
        f.sessions.insert("research".into());
        assert!(f.matches(&card("research", None)));
        assert!(!f.matches(&card("deploy", None)));
    }

    #[test]
    fn query_matches_title_substring_case_insensitively() {
        let mut f = FilterState::default();
        f.query = Some("rev".into());
        assert!(f.matches(&card("a", Some("Revenue per day"))));
        assert!(!f.matches(&card("a", Some("uptime"))));
    }
}
