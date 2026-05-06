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
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
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

            // Render.
            let snap = self.snapshot.lock().unwrap().clone();
            terminal.draw(|f| draw(f, &snap))?;

            // Poll for keyboard or sleep until next tick.
            if event::poll(tick)? {
                if let Event::Key(KeyEvent {
                    code,
                    kind: KeyEventKind::Press,
                    ..
                }) = event::read()?
                {
                    if matches!(code, KeyCode::Char('q')) {
                        return Ok(());
                    }
                }
            }
        }
    }
}

fn draw(f: &mut ratatui::Frame, snap: &UiSnapshot) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(f.area());
    let header = Line::from(vec![
        Span::styled("lookout ", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(format!("— {} — feed: {}", snap.url, snap.feed.len())),
    ]);
    f.render_widget(Paragraph::new(header), chunks[0]);
    let body = Block::default().borders(Borders::ALL).title("feed");
    f.render_widget(body, chunks[1]);
}
