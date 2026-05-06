use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;
use ratatui_image::{picker::Picker, StatefulImage};

/// Detect the terminal's image protocol once. Falls back to a font-size-based picker
/// (which uses halfblocks) when stdio probing fails (e.g. when not a TTY).
fn picker() -> &'static Picker {
    static PICKER: std::sync::OnceLock<Picker> = std::sync::OnceLock::new();
    PICKER.get_or_init(|| {
        Picker::from_query_stdio()
            .unwrap_or_else(|_| Picker::from_fontsize((8, 16)))
    })
}

pub fn render(f: &mut Frame, area: Rect, bytes: &[u8], _mime: Option<&str>) {
    let block = Block::default().borders(Borders::ALL).title("Image");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let img = match image::load_from_memory(bytes) {
        Ok(i) => i,
        Err(e) => {
            let p = Paragraph::new(format!("[image: decode failed: {e}]"));
            f.render_widget(p, inner);
            return;
        }
    };
    let mut protocol = picker().new_resize_protocol(img);
    let widget = StatefulImage::default();
    f.render_stateful_widget(widget, inner, &mut protocol);
}
