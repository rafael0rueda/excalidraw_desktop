//! The tool bar: Excalidraw's tools with their keys, the tool lock, undo
//! and redo, on an island in the theme's colours as Excalidraw floats its
//! own. Labels for now; icons come with the parity work (phase 8).

use crate::chrome::{chip, island, small};
use crate::theme::Look;
use crate::Message;
use editor::{Editor, Tool};
use iced::widget::{button, container, row, Space};
use iced::{Alignment, Background, Element, Length};

const TOOLS: [(Tool, &str, &str); 9] = [
    (Tool::Hand, "Hand", "H"),
    (Tool::Selection, "Select", "V"),
    (Tool::Rectangle, "Rectangle", "R"),
    (Tool::Diamond, "Diamond", "D"),
    (Tool::Ellipse, "Ellipse", "O"),
    (Tool::Arrow, "Arrow", "A"),
    (Tool::Line, "Line", "L"),
    (Tool::FreeDraw, "Draw", "P"),
    (Tool::Text, "Text", "T"),
];

fn divider<'a>(look: Look) -> Element<'a, Message> {
    container(Space::new().width(1).height(20))
        .style(move |_| container::Style { background: Some(Background::Color(look.surface_alt)), ..container::Style::default() })
        .into()
}

pub fn toolbar(look: Look, editor: &Editor) -> Element<'_, Message> {
    let mut tools = row![].spacing(2).align_y(Alignment::Center);
    let lock = button(small(if editor.tool_locked { "Locked" } else { "Lock" })).on_press(Message::ToggleLock).style(chip(look, editor.tool_locked));
    tools = tools.push(lock).push(divider(look));
    for (tool, name, key) in TOOLS {
        let label = row![small(name), small(key).size(10).color(look.muted)].spacing(4).align_y(Alignment::Center);
        tools = tools.push(button(label).on_press(Message::Tool(tool)).style(chip(look, editor.tool == tool)));
    }
    let undo = button(small("Undo")).on_press_maybe(editor.can_undo().then_some(Message::Undo)).style(chip(look, false));
    let redo = button(small("Redo")).on_press_maybe(editor.can_redo().then_some(Message::Redo)).style(chip(look, false));
    tools = tools.push(divider(look)).push(undo).push(redo);
    let bar = row![Space::new().width(Length::Fill), container(tools).padding(4).style(island(look)), Space::new().width(Length::Fill)];
    container(bar).padding(8).width(Length::Fill).into()
}
