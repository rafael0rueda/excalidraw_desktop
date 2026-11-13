//! The right-click menu, with Excalidraw 0.18.1's entries
//! (`getContextMenuItems`) for what this build does: one for the canvas,
//! one for the selection. A right click on something not selected selects
//! it first; on nothing, it clears the selection.

use crate::chrome::{chip, island, small};
use crate::{library, App, Message};
use editor::{Axis, Tool};
use iced::widget::{button, column, container, opaque, pin, row, Space};
use iced::{clipboard, Alignment, Background, Element, Length, Point, Task};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Cut,
    Copy,
    Paste,
    CopyPng,
    CopyStyles,
    PasteStyles,
    Group,
    Ungroup,
    AddToLibrary,
    SendBackward,
    BringForward,
    SendToBack,
    BringToFront,
    FlipX,
    FlipY,
    Link,
    Duplicate,
    Lock,
    Delete,
    SelectAll,
    UnlockAll,
    ToggleGrid,
}

/// The menu's width, and an entry's height, for keeping it on screen.
const WIDTH: f32 = 250.0;
const ENTRY: f32 = 27.0;

/// One entry, or a rule between groups.
enum Entry {
    Item(&'static str, &'static str, Action),
    Rule,
}

impl App {
    /// A right click at `p` in the drawing, `screen` in the window.
    pub fn open_context(&mut self, p: [f64; 2], screen: Point) {
        self.popup = None;
        self.pointer = p;
        let editor = &mut self.tabs[self.active].doc.editor;
        editor.finish_text();
        editor.tool = Tool::Selection;
        let hit = editor.element_at(p).map(|e| e.id.clone());
        match hit {
            Some(id) => {
                if !editor.selection.contains(&id) {
                    editor.select(vec![id]);
                }
            }
            None => editor.selection.clear(),
        }
        self.context = Some(screen);
    }

    fn entries(&self) -> Vec<Entry> {
        use Action::*;
        use Entry::{Item, Rule};
        let editor = &self.tab().doc.editor;
        if editor.selection.is_empty() {
            let mut v = vec![Item("Paste", "Ctrl+V", Paste), Rule, Item("Copy to clipboard as PNG", "Ctrl+Shift+C", CopyPng), Rule, Item("Select all", "Ctrl+A", SelectAll)];
            if editor.any_locked() {
                v.push(Item("Unlock all", "", UnlockAll));
            }
            v.push(Rule);
            v.push(Item(if editor.grid_size().is_some() { "Hide grid" } else { "Show grid" }, "Ctrl+'", ToggleGrid));
            return v;
        }
        let selected = editor.selection.len();
        let grouped = editor.selection.iter().any(|id| editor.scene.by_id(id).is_some_and(|e| !e.group_ids.is_empty()));
        let mut v = vec![Item("Cut", "Ctrl+X", Cut), Item("Copy", "Ctrl+C", Copy), Item("Paste", "Ctrl+V", Paste), Rule];
        v.extend([Item("Copy to clipboard as PNG", "Ctrl+Shift+C", CopyPng), Rule, Item("Copy styles", "Ctrl+Alt+C", CopyStyles), Item("Paste styles", "Ctrl+Alt+V", PasteStyles), Rule]);
        if selected > 1 {
            v.push(Item("Group selection", "Ctrl+G", Group));
        }
        if grouped {
            v.push(Item("Ungroup selection", "Ctrl+Shift+G", Ungroup));
        }
        if selected > 1 || grouped {
            v.push(Rule);
        }
        v.extend([
            Item("Add to library", "", AddToLibrary),
            Rule,
            Item("Send backward", "Ctrl+[", SendBackward),
            Item("Bring forward", "Ctrl+]", BringForward),
            Item("Send to back", "Ctrl+Shift+[", SendToBack),
            Item("Bring to front", "Ctrl+Shift+]", BringToFront),
            Rule,
            Item("Flip horizontal", "Shift+H", FlipX),
            Item("Flip vertical", "Shift+V", FlipY),
            Rule,
        ]);
        if editor.link_target().is_some() {
            v.extend([Item("Link", "Ctrl+K", Link), Rule]);
        }
        let locked = editor.selection_locked();
        v.extend([
            Item("Duplicate", "Ctrl+D", Duplicate),
            Item(if locked { "Unlock" } else { "Lock" }, "Ctrl+Shift+L", Lock),
            Rule,
            Item("Delete", "Delete", Delete),
        ]);
        v
    }

    pub fn context_action(&mut self, action: Action) -> Task<Message> {
        self.context = None;
        let editor = &mut self.tabs[self.active].doc.editor;
        match action {
            Action::Cut => {
                if let Some(text) = editor.cut() {
                    return clipboard::write(text);
                }
            }
            Action::Copy => {
                if let Some(text) = editor.copy() {
                    return clipboard::write(text);
                }
            }
            Action::Paste => return crate::inserting::read_clipboard(),
            Action::CopyPng => {
                let selection = !editor.selection.is_empty();
                return self.copy_image_of(selection);
            }
            Action::CopyStyles => {
                editor.copy_styles();
            }
            Action::PasteStyles => editor.paste_styles(),
            Action::Group => editor.group(),
            Action::Ungroup => editor.ungroup(),
            Action::AddToLibrary => return self.handle(Message::Library(library::Msg::AddSelection)),
            Action::SendBackward => editor.send_backward(),
            Action::BringForward => editor.bring_forward(),
            Action::SendToBack => editor.send_to_back(),
            Action::BringToFront => editor.bring_to_front(),
            Action::FlipX => editor.flip(Axis::X),
            Action::FlipY => editor.flip(Axis::Y),
            Action::Link => return self.edit_link(),
            Action::Duplicate => editor.duplicate(),
            Action::Lock => editor.toggle_lock(),
            Action::Delete => editor.delete_selected(),
            Action::SelectAll => editor.select_all(),
            Action::UnlockAll => editor.unlock_all(),
            Action::ToggleGrid => self.toggle_grid(),
        }
        Task::none()
    }
}

pub fn view(app: &App) -> Option<Element<'_, Message>> {
    let at = app.context?;
    let look = app.look();
    let entries = app.entries();
    let mut list = column![].spacing(0).width(WIDTH);
    for e in &entries {
        let entry: Element<'_, Message> = match e {
            Entry::Item(label, keys, action) => button(
                row![small(*label), Space::new().width(Length::Fill), small(*keys).size(12).color(look.muted)].align_y(Alignment::Center),
            )
            .on_press(Message::Context(*action))
            .width(Length::Fill)
            .padding([5, 10])
            .style(chip(look, false))
            .into(),
            Entry::Rule => container(Space::new().height(1))
                .width(Length::Fill)
                .style(move |_| container::Style { background: Some(Background::Color(look.surface_alt)), ..container::Style::default() })
                .padding([0, 4])
                .into(),
        };
        list = list.push(entry);
    }
    let items = entries.iter().filter(|e| matches!(e, Entry::Item(..))).count() as f32;
    let height = items * ENTRY + 16.0;
    // Kept inside the drawing area.
    let x = at.x.min(app.size.width - WIDTH - 12.0).max(4.0);
    let y = at.y.min(app.size.height - height - 4.0).max(4.0);
    let menu = container(list).padding(6).style(island(look));
    Some(pin(opaque(menu)).x(x).y(y).into())
}

impl App {
    /// Ctrl+' and the menu: the grid of the drawing in view.
    pub fn toggle_grid(&mut self) {
        self.tab_mut().doc.editor.toggle_grid();
    }
}
