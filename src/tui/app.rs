//! TUI task — owns the terminal, redraws on state deltas, handles input.

use crate::card::Card;
use crate::error::Result;
use crate::state::StateDelta;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::Terminal;
use std::io::Stdout;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::broadcast;

/// Which region of the TUI currently owns keyboard focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusRegion {
    Pins,
    Feed,
}

/// Read-only view of the parts of AppState the TUI needs.
#[derive(Clone, Default)]
pub struct UiSnapshot {
    pub feed: Vec<Card>,
    pub pins: Vec<(String, Card)>,
    pub url: String,
}

pub struct TuiApp {
    snapshot: Arc<Mutex<UiSnapshot>>,
    deltas: broadcast::Receiver<StateDelta>,
    /// Closure that produces a fresh snapshot from the live AppState.
    /// (We pass a closure to keep the AppState ownership in the state task.)
    refresh: Arc<dyn Fn() -> UiSnapshot + Send + Sync>,
    cmd_tx: tokio::sync::mpsc::Sender<crate::state::Command>,
    focus: FocusRegion,
    pin_focused_idx: usize,
    feed_focused_idx: usize,
    expanded: Option<crate::card::CardId>,
    zoomed_pin: Option<String>,
    feed_compact: bool,
    filter: crate::tui::filter::FilterState,
    /// When `Some`, we're in filter prompt mode and the buffer holds the typed query.
    filter_prompt: Option<String>,
}

impl TuiApp {
    pub fn new(
        deltas: broadcast::Receiver<StateDelta>,
        refresh: Arc<dyn Fn() -> UiSnapshot + Send + Sync>,
        cmd_tx: tokio::sync::mpsc::Sender<crate::state::Command>,
    ) -> Self {
        let initial = refresh();
        Self {
            snapshot: Arc::new(Mutex::new(initial)),
            deltas,
            refresh,
            cmd_tx,
            focus: FocusRegion::Pins,
            pin_focused_idx: 0,
            feed_focused_idx: 0,
            expanded: None,
            zoomed_pin: None,
            feed_compact: true,
            filter: crate::tui::filter::FilterState::default(),
            filter_prompt: None,
        }
    }

    pub async fn run(mut self) -> Result<()> {
        enable_raw_mode()?;
        let mut stdout = std::io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        let result = self.event_loop(&mut terminal).await;

        // Always restore the terminal.
        disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
        terminal.show_cursor()?;
        result
    }

    async fn event_loop(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> Result<()> {
        let tick = Duration::from_millis(16);
        loop {
            // Refresh snapshot if the state task has new deltas.
            while let Ok(_d) = self.deltas.try_recv() {
                let fresh = (self.refresh)();
                *self.snapshot.lock().unwrap() = fresh;
            }

            // Clamp feed_focused_idx to valid range after any refresh.
            let snap = self.snapshot.lock().unwrap().clone();
            if !snap.feed.is_empty() {
                self.feed_focused_idx = self.feed_focused_idx.min(snap.feed.len() - 1);
            }

            // Render.
            terminal.draw(|f| {
                draw(f, &snap, self.feed_focused_idx, self.expanded, &self.filter, self.filter_prompt.as_deref())
            })?;

            // Poll for keyboard or sleep until next tick.
            if event::poll(tick)? {
                if let Event::Key(KeyEvent {
                    code,
                    kind: KeyEventKind::Press,
                    modifiers,
                    ..
                }) = event::read()?
                {
                    // Ctrl-C — quit regardless of mode (including filter prompt).
                    if modifiers.contains(KeyModifiers::CONTROL) && code == KeyCode::Char('c') {
                        return Ok(());
                    }

                    // Filter-prompt mode: route keys to the input buffer.
                    if let Some(buf) = self.filter_prompt.as_mut() {
                        match code {
                            KeyCode::Esc => {
                                self.filter_prompt = None;
                            }
                            KeyCode::Enter => {
                                let query = std::mem::take(buf).trim().to_string();
                                self.filter.query =
                                    if query.is_empty() { None } else { Some(query) };
                                self.filter_prompt = None;
                            }
                            KeyCode::Backspace => {
                                buf.pop();
                            }
                            KeyCode::Char(c) => {
                                buf.push(c);
                            }
                            _ => {}
                        }
                        // Prompt consumed the key; skip normal bindings.
                    } else {
                        match code {
                            KeyCode::Char('q') => return Ok(()),
                            KeyCode::Char(c @ '1'..='9') => {
                                let idx = (c as u8 - b'1') as usize;
                                let snap = self.snapshot.lock().unwrap();
                                let mut sessions: Vec<String> = snap
                                    .feed
                                    .iter()
                                    .map(|c| c.session.clone())
                                    .collect::<std::collections::HashSet<_>>()
                                    .into_iter()
                                    .collect();
                                sessions.sort();
                                drop(snap);
                                if let Some(s) = sessions.get(idx).cloned() {
                                    if !self.filter.sessions.remove(&s) {
                                        self.filter.sessions.insert(s);
                                    }
                                }
                            }
                            KeyCode::Char('j') | KeyCode::Down => {
                                let len = self.snapshot.lock().unwrap().feed.len();
                                if self.feed_focused_idx + 1 < len {
                                    self.feed_focused_idx += 1;
                                }
                            }
                            KeyCode::Char('k') | KeyCode::Up => {
                                if self.feed_focused_idx > 0 {
                                    self.feed_focused_idx -= 1;
                                }
                            }
                            KeyCode::Char('o') | KeyCode::Enter => {
                                let snap = self.snapshot.lock().unwrap();
                                if !snap.feed.is_empty() {
                                    // Newest at top: card at displayed index `i` is feed[len - 1 - i].
                                    let len = snap.feed.len();
                                    let idx = self.feed_focused_idx.min(len - 1);
                                    let card_idx = len - 1 - idx;
                                    let id = snap.feed[card_idx].id;
                                    drop(snap);
                                    if self.expanded == Some(id) {
                                        self.expanded = None;
                                    } else {
                                        self.expanded = Some(id);
                                    }
                                }
                            }
                            KeyCode::Esc => {
                                self.expanded = None;
                            }
                            KeyCode::Char('p') => {
                                // Pin focused card to a slot named "pinned:<short_id>".
                                let snap = self.snapshot.lock().unwrap();
                                if !snap.feed.is_empty() {
                                    let len = snap.feed.len();
                                    let idx = self.feed_focused_idx.min(len - 1);
                                    let card_idx = len - 1 - idx;
                                    let mut card = snap.feed[card_idx].clone();
                                    drop(snap);
                                    let short = card.id.0.to_string()[..8].to_string();
                                    card.pin_slot = Some(format!("pinned:{short}"));
                                    let _ = self
                                        .cmd_tx
                                        .try_send(crate::state::Command::PushCard(card));
                                }
                            }
                            KeyCode::Char('P') => {
                                // Unpin focused card's slot if it's pinned.
                                let snap = self.snapshot.lock().unwrap();
                                if !snap.feed.is_empty() {
                                    let len = snap.feed.len();
                                    let idx = self.feed_focused_idx.min(len - 1);
                                    let card_idx = len - 1 - idx;
                                    if let Some(slot) = snap.feed[card_idx].pin_slot.clone() {
                                        drop(snap);
                                        let _ = self
                                            .cmd_tx
                                            .try_send(crate::state::Command::Unpin { slot });
                                    }
                                }
                            }
                            KeyCode::Char('c') => {
                                let _ = self
                                    .cmd_tx
                                    .try_send(crate::state::Command::ClearFeed);
                            }
                            KeyCode::Char('/') => {
                                self.filter_prompt = Some(String::new());
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }
}

fn draw(f: &mut ratatui::Frame, snap: &UiSnapshot, focused_idx: usize, expanded: Option<crate::card::CardId>, filter: &crate::tui::filter::FilterState, prompt: Option<&str>) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),  // header
            Constraint::Length(1),  // filter bar
            Constraint::Min(1),     // body
        ])
        .split(f.area());
    crate::tui::header::render(f, chunks[0], snap);

    let mut all_sessions: Vec<String> = snap
        .feed
        .iter()
        .map(|c| c.session.clone())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    all_sessions.sort();
    crate::tui::filter::render(f, chunks[1], &all_sessions, filter, prompt);

    // Split body into feed (70%) and pin sidebar (30%).
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
        .split(chunks[2]);

    let filtered: Vec<Card> = snap.feed.iter().filter(|c| filter.matches(c)).cloned().collect();

    if let Some(id) = expanded {
        if let Some(card) = filtered.iter().find(|c| c.id == id) {
            // Render the card's body in the feed area, with a title bar.
            use ratatui::widgets::{Block, Borders};
            let block = Block::default().borders(Borders::ALL).title(format!("▾ {}", card.title.as_deref().unwrap_or("(no title)")));
            let inner = block.inner(body[0]);
            f.render_widget(block, body[0]);
            crate::tui::render::render_body(f, inner, card);
        } else {
            // Card not found (evicted?) — fall back to feed.
            crate::tui::feed::render(f, body[0], crate::tui::feed::FeedView { cards: &filtered, focused: focused_idx });
        }
    } else {
        crate::tui::feed::render(
            f,
            body[0],
            crate::tui::feed::FeedView {
                cards: &filtered,
                focused: focused_idx,
            },
        );
    }

    crate::tui::pins::render(
        f,
        body[1],
        crate::tui::pins::PinView {
            pins: &snap.pins,
            focused: None,
            zoomed: None,
        },
    );
}
