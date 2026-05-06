//! TUI task — owns the terminal, redraws on state deltas, handles input.

use crate::card::Card;
use crate::error::Result;
use crate::state::StateDelta;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
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
    focused_idx: usize,
    expanded: Option<crate::card::CardId>,
}

impl TuiApp {
    pub fn new(
        deltas: broadcast::Receiver<StateDelta>,
        refresh: Arc<dyn Fn() -> UiSnapshot + Send + Sync>,
    ) -> Self {
        let initial = refresh();
        Self {
            snapshot: Arc::new(Mutex::new(initial)),
            deltas,
            refresh,
            focused_idx: 0,
            expanded: None,
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

            // Clamp focused_idx to valid range after any refresh.
            let snap = self.snapshot.lock().unwrap().clone();
            if !snap.feed.is_empty() {
                self.focused_idx = self.focused_idx.min(snap.feed.len() - 1);
            }

            // Render.
            terminal.draw(|f| draw(f, &snap, self.focused_idx, self.expanded))?;

            // Poll for keyboard or sleep until next tick.
            if event::poll(tick)? {
                if let Event::Key(KeyEvent {
                    code,
                    kind: KeyEventKind::Press,
                    ..
                }) = event::read()?
                {
                    match code {
                        KeyCode::Char('q') => return Ok(()),
                        KeyCode::Char('j') | KeyCode::Down => {
                            let len = self.snapshot.lock().unwrap().feed.len();
                            if self.focused_idx + 1 < len {
                                self.focused_idx += 1;
                            }
                        }
                        KeyCode::Char('k') | KeyCode::Up => {
                            if self.focused_idx > 0 {
                                self.focused_idx -= 1;
                            }
                        }
                        KeyCode::Char('o') | KeyCode::Enter => {
                            let snap = self.snapshot.lock().unwrap();
                            if !snap.feed.is_empty() {
                                // Newest at top: card at displayed index `i` is feed[len - 1 - i].
                                let len = snap.feed.len();
                                let idx = self.focused_idx.min(len - 1);
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
                        _ => {}
                    }
                }
            }
        }
    }
}

fn draw(f: &mut ratatui::Frame, snap: &UiSnapshot, focused_idx: usize, expanded: Option<crate::card::CardId>) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(f.area());
    crate::tui::header::render(f, chunks[0], snap);

    // Split body into feed (70%) and pin sidebar (30%).
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
        .split(chunks[1]);

    if let Some(id) = expanded {
        if let Some(card) = snap.feed.iter().find(|c| c.id == id) {
            // Render the card's body in the feed area, with a title bar.
            use ratatui::widgets::{Block, Borders};
            let block = Block::default().borders(Borders::ALL).title(format!("▾ {}", card.title.as_deref().unwrap_or("(no title)")));
            let inner = block.inner(body[0]);
            f.render_widget(block, body[0]);
            crate::tui::render::render_body(f, inner, card);
        } else {
            // Card not found (evicted?) — fall back to feed.
            crate::tui::feed::render(f, body[0], crate::tui::feed::FeedView { cards: &snap.feed, focused: focused_idx });
        }
    } else {
        crate::tui::feed::render(
            f,
            body[0],
            crate::tui::feed::FeedView {
                cards: &snap.feed,
                focused: focused_idx,
            },
        );
    }

    // Pin sidebar placeholder (Task 30 fills this in).
    let pin_block = ratatui::widgets::Block::default()
        .borders(ratatui::widgets::Borders::ALL)
        .title("Pinned");
    f.render_widget(pin_block, body[1]);
}
