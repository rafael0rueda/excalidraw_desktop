//! The tool bar: Excalidraw's tools with their keys, the tool lock, undo
//! and redo. Plain text buttons until the real chrome (and themes) arrive
//! in phase 5.

use crate::Message;
use editor::{Editor, Tool};
use iced::widget::{button, container, row, text, Space};
use iced::{Alignment, Element, Length};

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

pub fn toolbar(editor: &Editor) -> Element<'_, Message> {
    let mut tools = row![].spacing(4).align_y(Alignment::Center);
    let lock = button(text(if editor.tool_locked { "Locked" } else { "Lock" }).size(13))
        .on_press(Message::ToggleLock)
        .style(if editor.tool_locked {
            button::primary
        } else {
            button::secondary
        });
    tools = tools.push(lock);
    for (tool, name, key) in TOOLS {
        let label = text(format!("{name} {key}")).size(13);
        let style = if editor.tool == tool {
            button::primary
        } else {
            button::secondary
        };
        tools = tools.push(button(label).on_press(Message::Tool(tool)).style(style));
    }
    let undo = button(text("Undo").size(13))
        .on_press_maybe(editor.can_undo().then_some(Message::Undo))
        .style(button::secondary);
    let redo = button(text("Redo").size(13))
        .on_press_maybe(editor.can_redo().then_some(Message::Redo))
        .style(button::secondary);
    let bar = row![
        Space::new().width(Length::Fill),
        tools,
        Space::new().width(16),
        undo,
        redo,
        Space::new().width(Length::Fill)
    ]
    .spacing(4);
    container(bar).padding(8).width(Length::Fill).into()
}
