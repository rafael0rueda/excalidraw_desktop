//! The tool bar: Excalidraw's tools as icons with their number keys, the
//! tool lock, undo and redo, on an island in the theme's colours as
//! Excalidraw floats its own.

use crate::chrome::{chip, island};
use crate::icons::{icon, Icon};
use crate::theme::Look;
use crate::Message;
use editor::{Editor, Tool};
use iced::widget::{button, container, row, stack, text, tooltip, Space};
use iced::{Alignment, Background, Element, Length};

/// Each tool, its icon, its name, its letter and its number key.
const TOOLS: [(Tool, Icon, &str, &str, &str); 9] = [
    (Tool::Hand, Icon::Hand, "Hand (panning tool)", "H", ""),
    (Tool::Selection, Icon::Select, "Selection", "V", "1"),
    (Tool::Rectangle, Icon::Rectangle, "Rectangle", "R", "2"),
    (Tool::Diamond, Icon::Diamond, "Diamond", "D", "3"),
    (Tool::Ellipse, Icon::Ellipse, "Ellipse", "O", "4"),
    (Tool::Arrow, Icon::Arrow, "Arrow", "A", "5"),
    (Tool::Line, Icon::Line, "Line", "L", "6"),
    (Tool::FreeDraw, Icon::Draw, "Draw", "P", "7"),
    (Tool::Text, Icon::Text, "Text", "T", "8"),
];

fn divider<'a>(look: Look) -> Element<'a, Message> {
    container(Space::new().width(1).height(20))
        .style(move |_| container::Style { background: Some(Background::Color(look.surface_alt)), ..container::Style::default() })
        .into()
}

fn tip<'a>(look: Look, content: impl Into<Element<'a, Message>>, label: String) -> Element<'a, Message> {
    tooltip(content, container(text(label).size(12)).padding([4, 8]).style(move |t| island(look)(t)), tooltip::Position::Bottom).gap(6).into()
}

/// An icon with its number key in the bottom right corner.
fn keyed<'a>(look: Look, i: Icon, key: &str) -> Element<'a, Message> {
    let badge = container(text(key.to_owned()).size(9).color(look.muted)).align_right(Length::Fill).align_bottom(Length::Fill);
    stack![container(icon(i, look.text, 18.0)).center(Length::Fill), badge].width(26).height(24).into()
}

pub fn toolbar(look: Look, editor: &Editor) -> Element<'_, Message> {
    let mut tools = row![].spacing(2).align_y(Alignment::Center);
    let locked = editor.tool_locked;
    let lock = button(container(icon(Icon::Lock(locked), look.text, 18.0)).center(Length::Fill).width(26).height(24)).on_press(Message::ToggleLock).padding(3).style(chip(look, locked));
    tools = tools.push(tip(look, lock, "Keep selected tool active after drawing — Q".into())).push(divider(look));
    for (tool, i, name, letter, number) in TOOLS {
        let b = button(keyed(look, i, number)).on_press(Message::Tool(tool)).padding(3).style(chip(look, editor.tool == tool));
        let shortcut = if number.is_empty() { letter.to_owned() } else { format!("{letter} or {number}") };
        tools = tools.push(tip(look, b, format!("{name} — {shortcut}")));
    }
    let undo = button(container(icon(Icon::Undo, look.text, 18.0)).center(Length::Fill).width(26).height(24)).on_press_maybe(editor.can_undo().then_some(Message::Undo)).padding(3).style(chip(look, false));
    let redo = button(container(icon(Icon::Redo, look.text, 18.0)).center(Length::Fill).width(26).height(24)).on_press_maybe(editor.can_redo().then_some(Message::Redo)).padding(3).style(chip(look, false));
    tools = tools.push(divider(look)).push(tip(look, undo, "Undo — Ctrl+Z".into())).push(tip(look, redo, "Redo — Ctrl+Shift+Z".into()));
    let bar = row![Space::new().width(Length::Fill), container(tools).padding(4).style(island(look)), Space::new().width(Length::Fill)];
    container(bar).padding(8).width(Length::Fill).into()
}
