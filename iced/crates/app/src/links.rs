//! Links on elements: Ctrl+K to edit the selected element's, and following
//! one from its icon. A link to part of this drawing (`?element=` or
//! `?group=`) brings it into view; anything else goes to the desktop's own
//! browser or mail client, and only if it is http, https, mailto or tel
//! (plan, "Security"). The app never opens a page itself.

use crate::doc::fit;
use crate::{App, Message, Prompt};
use iced::Task;

/// The schemes handed to the desktop.
const OPENED: [&str; 4] = ["http://", "https://", "mailto:", "tel:"];

impl App {
    /// Ctrl+K: the link editor, for one selected element.
    pub fn edit_link(&mut self) -> Task<Message> {
        let editor = &mut self.tab_mut().doc.editor;
        editor.finish_text();
        match editor.link_target() {
            Some((_, link)) => {
                self.prompt = Some(Prompt::Link { draft: link });
                iced::widget::operation::focus(crate::chrome::LINK_FIELD)
            }
            None => {
                self.status = Some("Select one element to give it a link.".into());
                Task::none()
            }
        }
    }

    pub fn follow_link(&mut self, link: &str) {
        let editor = &self.tab().doc.editor;
        let targets = editor.link_targets(link);
        if !targets.is_empty() {
            let b = editor::geometry::bounds_of(targets.iter().filter_map(|id| editor.scene.by_id(id)));
            let view = fit(b, self.size);
            self.set_view(view);
            return;
        }
        let lower = link.trim().to_ascii_lowercase();
        if !OPENED.iter().any(|s| lower.starts_with(s)) {
            self.status = Some(format!("Only web and mail links are opened: {link}"));
            return;
        }
        match std::process::Command::new("xdg-open")
            .arg(link.trim())
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
        {
            Ok(mut child) => {
                // Reaped when it is done, so it leaves no zombie.
                std::thread::spawn(move || child.wait());
                self.status = Some(format!("Opened {link}"));
            }
            Err(e) => self.prompt = Some(Prompt::Error(format!("Could not open {link}: {e}"))),
        }
    }
}
