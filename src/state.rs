//! In-memory application state. Single-writer; readers consume StateDeltas.

use crate::card::{Card, CardId, SessionId};
use indexmap::IndexMap;
use std::collections::{HashMap, VecDeque};

#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub label: String,
    /// Stable color slot 0..=15 chosen on first sight; never changes.
    pub color: u8,
}

#[derive(Debug)]
pub struct AppState {
    feed: VecDeque<Card>,
    feed_max: usize,
    pins: IndexMap<String, Card>,
    sessions: HashMap<SessionId, SessionInfo>,
    /// Counter used to assign initial session colors.
    next_color: u8,
}

#[derive(Debug, Clone)]
pub enum StateDelta {
    CardPushed { id: CardId, in_feed: bool, pin_slot: Option<String> },
    CardEvicted { id: CardId },
    PinReplaced { slot: String },
    PinRemoved { slot: String },
    FeedCleared,
    SessionUpdated(SessionId),
}

impl AppState {
    pub fn new(feed_max: usize) -> Self {
        assert!(feed_max > 0, "feed_max must be > 0");
        Self {
            feed: VecDeque::with_capacity(feed_max),
            feed_max,
            pins: IndexMap::new(),
            sessions: HashMap::new(),
            next_color: 0,
        }
    }

    pub fn feed(&self) -> &VecDeque<Card> {
        &self.feed
    }

    pub fn pins(&self) -> &IndexMap<String, Card> {
        &self.pins
    }

    pub fn sessions(&self) -> &HashMap<SessionId, SessionInfo> {
        &self.sessions
    }

    /// Push a card. Returns deltas describing the change(s).
    /// If the card has a pin slot (explicit or auto from progress), the slot
    /// is replaced. The card is also appended to the feed.
    pub fn push(&mut self, card: Card) -> Vec<StateDelta> {
        let mut deltas = Vec::new();
        let card_id = card.id;
        self.touch_session(&card.session);
        let pin_slot = card.auto_pin_slot();

        if let Some(slot) = &pin_slot {
            self.pins.insert(slot.clone(), card.clone());
            deltas.push(StateDelta::PinReplaced { slot: slot.clone() });
        }

        // Always append to feed.
        if self.feed.len() == self.feed_max {
            if let Some(evicted) = self.feed.pop_front() {
                deltas.push(StateDelta::CardEvicted { id: evicted.id });
            }
        }
        self.feed.push_back(card);
        deltas.push(StateDelta::CardPushed {
            id: card_id,
            in_feed: true,
            pin_slot,
        });
        deltas
    }

    pub fn unpin(&mut self, slot: &str) -> Option<StateDelta> {
        self.pins
            .shift_remove(slot)
            .map(|_| StateDelta::PinRemoved { slot: slot.to_string() })
    }

    pub fn clear_feed(&mut self) -> StateDelta {
        self.feed.clear();
        StateDelta::FeedCleared
    }

    pub fn set_session_label(
        &mut self,
        session: &SessionId,
        label: String,
        color: Option<u8>,
    ) -> StateDelta {
        let next_color = self.next_color;
        let entry = self.sessions.entry(session.clone()).or_insert_with(|| {
            SessionInfo { label: session.clone(), color: next_color }
        });
        // If we just inserted, advance the color counter.
        if entry.color == next_color && entry.label == *session {
            self.next_color = self.next_color.wrapping_add(1);
        }
        // Apply user-provided overrides.
        let entry = self.sessions.get_mut(session).expect("just inserted");
        entry.label = label;
        if let Some(c) = color {
            entry.color = c;
        }
        StateDelta::SessionUpdated(session.clone())
    }

    fn touch_session(&mut self, session: &SessionId) {
        if !self.sessions.contains_key(session) {
            let c = self.next_color;
            self.next_color = self.next_color.wrapping_add(1);
            self.sessions.insert(
                session.clone(),
                SessionInfo {
                    label: session.clone(),
                    color: c,
                },
            );
        }
    }
}

use tokio::sync::{broadcast, mpsc};

/// Commands sent to the state task. The state task is the single writer
/// for AppState; everything else is read-only.
#[derive(Debug, Clone)]
pub enum Command {
    PushCard(Card),
    Unpin { slot: String },
    ClearFeed,
    SetSessionLabel {
        session: SessionId,
        label: String,
        color: Option<u8>,
    },
}

/// Run the state task. Returns when the command sender is dropped.
pub async fn state_task(
    mut state: AppState,
    mut cmds: mpsc::Receiver<Command>,
    deltas_tx: broadcast::Sender<StateDelta>,
) {
    while let Some(cmd) = cmds.recv().await {
        let new_deltas = match cmd {
            Command::PushCard(card) => state.push(card),
            Command::Unpin { slot } => state.unpin(&slot).into_iter().collect(),
            Command::ClearFeed => vec![state.clear_feed()],
            Command::SetSessionLabel { session, label, color } => {
                vec![state.set_session_label(&session, label, color)]
            }
        };
        for d in new_deltas {
            // Lagged subscribers will miss; UI handles that via Lagged() recv error.
            let _ = deltas_tx.send(d);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::{CardKind, CommonArgs, TextFormat};

    fn text_card(session: &str, pin: Option<&str>, content: &str) -> Card {
        Card::build(
            CommonArgs {
                title: None,
                session: Some(session.into()),
                pin: pin.map(str::to_string),
                note: None,
            },
            "default".into(),
            CardKind::Text {
                content: content.into(),
                format: TextFormat::Plain,
                language: None,
            },
        )
    }

    #[test]
    fn push_appends_to_feed() {
        let mut s = AppState::new(4);
        let deltas = s.push(text_card("a", None, "hello"));
        assert_eq!(s.feed().len(), 1);
        assert_eq!(s.pins().len(), 0);
        assert!(matches!(
            deltas.last().unwrap(),
            StateDelta::CardPushed { in_feed: true, .. }
        ));
    }

    #[test]
    fn push_with_pin_replaces_slot() {
        let mut s = AppState::new(4);
        s.push(text_card("a", Some("slot"), "first"));
        s.push(text_card("a", Some("slot"), "second"));
        assert_eq!(s.pins().len(), 1);
        let c = s.pins().get("slot").unwrap();
        if let CardKind::Text { content, .. } = &c.kind {
            assert_eq!(content, "second");
        } else {
            panic!("wrong kind");
        }
        assert_eq!(s.feed().len(), 2, "pin should also appear in feed");
    }

    #[test]
    fn pin_insertion_order_is_preserved() {
        let mut s = AppState::new(4);
        s.push(text_card("a", Some("first"), "1"));
        s.push(text_card("a", Some("second"), "2"));
        let keys: Vec<&str> = s.pins().keys().map(String::as_str).collect();
        assert_eq!(keys, vec!["first", "second"]);
    }

    #[test]
    fn feed_evicts_oldest_when_full() {
        let mut s = AppState::new(2);
        let id1 = s.push(text_card("a", None, "1"));
        let _ = s.push(text_card("a", None, "2"));
        let deltas3 = s.push(text_card("a", None, "3"));
        assert_eq!(s.feed().len(), 2);
        assert!(deltas3
            .iter()
            .any(|d| matches!(d, StateDelta::CardEvicted { .. })));
        // The first push's delta should reference the same id that got evicted.
        let evicted_id_in_3 = deltas3.iter().find_map(|d| match d {
            StateDelta::CardEvicted { id } => Some(*id),
            _ => None,
        });
        let pushed_id_in_1 = id1.iter().find_map(|d| match d {
            StateDelta::CardPushed { id, .. } => Some(*id),
            _ => None,
        });
        assert_eq!(evicted_id_in_3, pushed_id_in_1);
    }

    #[test]
    fn unpin_removes_slot() {
        let mut s = AppState::new(4);
        s.push(text_card("a", Some("slot"), "x"));
        let d = s.unpin("slot").expect("present");
        assert!(matches!(d, StateDelta::PinRemoved { .. }));
        assert!(s.pins().is_empty());
    }

    #[test]
    fn unpin_nonexistent_returns_none() {
        let mut s = AppState::new(4);
        assert!(s.unpin("missing").is_none());
    }

    #[test]
    fn clear_feed_does_not_affect_pins() {
        let mut s = AppState::new(4);
        s.push(text_card("a", Some("slot"), "x"));
        s.push(text_card("a", None, "y"));
        s.clear_feed();
        assert_eq!(s.feed().len(), 0);
        assert_eq!(s.pins().len(), 1);
    }

    #[test]
    fn touch_session_assigns_distinct_colors_in_order() {
        let mut s = AppState::new(4);
        s.push(text_card("a", None, "1"));
        s.push(text_card("b", None, "2"));
        let ca = s.sessions().get("a").unwrap().color;
        let cb = s.sessions().get("b").unwrap().color;
        assert_eq!(ca, 0);
        assert_eq!(cb, 1);
    }

    #[test]
    fn set_session_label_overrides_default() {
        let mut s = AppState::new(4);
        s.push(text_card("conn-deadbeef", None, "1"));
        s.set_session_label(&"conn-deadbeef".into(), "research".into(), None);
        assert_eq!(s.sessions().get("conn-deadbeef").unwrap().label, "research");
    }
}
