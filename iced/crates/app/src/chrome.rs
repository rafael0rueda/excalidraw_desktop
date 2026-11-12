//! The window around the drawing: tabs, the File menu, the tool bar, the
//! status line, and the questions the app asks (unsaved changes, recovery).
//! Plain widgets until themes and the real chrome arrive in phase 5.

use crate::{App, Message, Prompt};
use iced::widget::{button, column, container, row, text, Space};
use iced::{Alignment, Color, Element, Length};

fn small(label: impl Into<String>) -> iced::widget::Text<'static> {
    text(label.into()).size(13)
}

pub fn tab_bar(app: &App) -> Element<'_, Message> {
    let mut tabs = row![].spacing(2).align_y(Alignment::Center);
    tabs = tabs.push(button(small("File")).on_press(Message::ToggleMenu).style(button::secondary));
    for (i, tab) in app.tabs.iter().enumerate() {
        let label = if tab.dirty() { format!("{} •", tab.title()) } else { tab.title() };
        let style = if i == app.active { button::primary } else { button::secondary };
        tabs = tabs.push(
            row![
                button(small(label)).on_press(Message::SelectTab(i)).style(style),
                button(small("×")).on_press(Message::CloseTab(tab.id.clone())).style(style),
            ]
            .spacing(0),
        );
    }
    tabs = tabs.push(button(small("+")).on_press(Message::NewTab).style(button::secondary));
    container(tabs).padding([0, 8]).align_y(Alignment::Center).height(crate::TAB_BAR).width(Length::Fill).into()
}

/// The File menu, under its button.
pub fn file_menu(app: &App) -> Element<'_, Message> {
    let item = |label: &str, key: &str, message: Message| {
        button(row![small(label.to_owned()), Space::new().width(Length::Fill), small(key.to_owned()).color(Color::from_rgb(0.5, 0.5, 0.5))].width(240))
            .on_press(message)
            .style(button::text)
            .width(Length::Fill)
    };
    let mut items = column![
        item("New tab", "Ctrl+N", Message::NewTab),
        item("Open…", "Ctrl+O", Message::Open),
        item("Save", "Ctrl+S", Message::Save),
        item("Save as…", "Ctrl+Shift+S", Message::SaveAs),
        item("Close tab", "Ctrl+W", Message::CloseTab(app.tabs[app.active].id.clone())),
    ]
    .spacing(0);
    if !app.recent.is_empty() {
        items = items.push(container(small("Open recent").color(Color::from_rgb(0.5, 0.5, 0.5))).padding([6, 10]));
        for entry in &app.recent {
            items = items.push(item(&entry.name, "", Message::OpenPath(entry.path.clone().into())));
        }
        items = items.push(item("Clear recent", "", Message::ClearRecent));
    }
    items = items.push(item("Quit", "Ctrl+Q", Message::Quit));
    container(container(items).padding(4).style(container::bordered_box).width(260))
        .padding(iced::Padding { top: crate::TAB_BAR, left: 8.0, right: 0.0, bottom: 0.0 })
        .into()
}

/// A question that must be answered before anything else.
pub fn prompt(prompt: &Prompt) -> Element<'_, Message> {
    let (message, buttons): (String, Vec<(&str, Message)>) = match prompt {
        Prompt::Unsaved { title, .. } => (
            format!("{title} has unsaved changes. Save before continuing?"),
            vec![("Save", Message::Answer(Answer::Save)), ("Discard", Message::Answer(Answer::Discard)), ("Cancel", Message::Answer(Answer::Cancel))],
        ),
        Prompt::Recover { what, .. } => (
            format!("{what} not saved when Excalidraw Desktop last closed. Recover?"),
            vec![("Recover", Message::Answer(Answer::Save)), ("Discard", Message::Answer(Answer::Discard))],
        ),
        Prompt::Error(e) => (e.clone(), vec![("OK", Message::Answer(Answer::Cancel))]),
    };
    let mut row = row![Space::new().width(Length::Fill)].spacing(8);
    for (i, (label, message)) in buttons.into_iter().enumerate() {
        let style = if i == 0 { button::primary } else { button::secondary };
        row = row.push(button(small(label)).on_press(message).style(style));
    }
    let card = container(column![text(message).size(14), row].spacing(16).width(420)).padding(20).style(container::bordered_box);
    // The drawing behind is dimmed and takes no input.
    container(card)
        .center(Length::Fill)
        .style(|_| container::Style { background: Some(Color { a: 0.35, ..Color::BLACK }.into()), ..Default::default() })
        .into()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    Save,
    Discard,
    Cancel,
}
