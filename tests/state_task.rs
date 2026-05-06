use lookout::card::{Card, CardKind, CommonArgs, TextFormat};
use lookout::state::{state_task, AppState, Command, StateDelta};
use tokio::sync::{broadcast, mpsc};

fn text(content: &str) -> Card {
    Card::build(
        CommonArgs {
            session: Some("s".into()),
            ..Default::default()
        },
        "default".into(),
        CardKind::Text {
            content: content.into(),
            format: TextFormat::Plain,
            language: None,
        },
    )
}

#[tokio::test]
async fn pushes_emit_card_pushed_delta() {
    let (cmd_tx, cmd_rx) = mpsc::channel(8);
    let (delta_tx, mut delta_rx) = broadcast::channel(16);
    let task = tokio::spawn(state_task(AppState::new(8), cmd_rx, delta_tx));

    cmd_tx.send(Command::PushCard(text("hi"))).await.unwrap();

    let d = delta_rx.recv().await.unwrap();
    assert!(matches!(d, StateDelta::CardPushed { in_feed: true, .. }));

    drop(cmd_tx);
    task.await.unwrap();
}

#[tokio::test]
async fn clear_feed_command_is_observable() {
    let (cmd_tx, cmd_rx) = mpsc::channel(8);
    let (delta_tx, mut delta_rx) = broadcast::channel(16);
    let task = tokio::spawn(state_task(AppState::new(8), cmd_rx, delta_tx));

    cmd_tx.send(Command::PushCard(text("hi"))).await.unwrap();
    let _ = delta_rx.recv().await.unwrap(); // CardPushed
    cmd_tx.send(Command::ClearFeed).await.unwrap();
    let d = delta_rx.recv().await.unwrap();
    assert!(matches!(d, StateDelta::FeedCleared));

    drop(cmd_tx);
    task.await.unwrap();
}
