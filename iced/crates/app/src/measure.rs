//! Text measured by shaping it with the bundled fonts, through iced's own
//! font system: what Excalidraw gets from the browser's `measureText`.

use iced::advanced::graphics::text::{cosmic_text, font_system, to_attributes};

pub struct Shaped;

impl editor::Measure for Shaped {
    fn line_width(&self, line: &str, family: f64, size: f64) -> f64 {
        let Ok(mut system) = font_system().write() else {
            return 0.0;
        };
        let raw = system.raw();
        let size = size as f32;
        let mut buffer = cosmic_text::Buffer::new(
            raw,
            cosmic_text::Metrics::new(size.max(1.0), size.max(1.0) * 1.25),
        );
        buffer.set_size(raw, None, None);
        let attrs = to_attributes(crate::paint::font(family));
        buffer.set_text(raw, line, &attrs, cosmic_text::Shaping::Advanced, None);
        buffer.shape_until_scroll(raw, false);
        f64::from(
            buffer
                .layout_runs()
                .map(|run| run.line_w)
                .fold(0.0, f32::max),
        )
    }
}
