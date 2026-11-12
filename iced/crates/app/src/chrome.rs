//! The window around the drawing: tabs, the File and Theme menus, the
//! questions the app asks, and the styles all of it (and the tool bar and
//! the theme editor) is drawn with, from the theme on screen.

use crate::theme::Look;
use crate::{App, Message, Prompt};
use iced::widget::{button, column, container, pick_list, row, slider, text, text_input, Space};
use iced::{Alignment, Background, Border, Color, Element, Length, Shadow, Vector};

/// The tab bar's height: the drawing gets the window less this.
pub const TAB_BAR: f32 = 36.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Menu {
    File,
    Theme,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    Save,
    Discard,
    Cancel,
    /// Yes, go ahead (delete a theme).
    Confirm,
}

pub fn small(label: impl Into<String>) -> iced::widget::Text<'static> {
    text(label.into()).size(13)
}

/// `a` moved toward `b`.
pub fn blend(a: Color, b: Color, t: f32) -> Color {
    Color::from_rgba(a.r + (b.r - a.r) * t, a.g + (b.g - a.g) * t, a.b + (b.b - a.b) * t, a.a + (b.a - a.a) * t)
}

fn rounded(radius: f32) -> Border {
    Border { radius: radius.into(), ..Border::default() }
}

// ------------------------------------------------------------ styles

/// A button in a bar or a list: the accent's soft shade when `on` (the
/// tool in use), a shade of the panel when hovered.
pub fn chip(look: Look, on: bool) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    move |_, status| {
        let background = match (on, status) {
            (true, _) => Some(look.accent_soft),
            (false, button::Status::Hovered) => Some(look.hover),
            (false, button::Status::Pressed) => Some(look.surface_alt),
            _ => None,
        };
        button::Style {
            background: background.map(Background::Color),
            text_color: if status == button::Status::Disabled { Color { a: 0.5, ..look.muted } } else { look.text },
            border: rounded(6.0),
            ..button::Style::default()
        }
    }
}

/// The button that does the thing: the accent.
pub fn primary(look: Look) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    move |_, status| button::Style {
        background: Some(Background::Color(match status {
            button::Status::Hovered | button::Status::Pressed => look.accent_hover,
            button::Status::Disabled => Color { a: 0.5, ..look.accent },
            _ => look.accent,
        })),
        text_color: look.accent_text,
        border: rounded(6.0),
        ..button::Style::default()
    }
}

/// Any other button in a dialog.
pub fn secondary(look: Look) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    move |_, status| button::Style {
        background: Some(Background::Color(match status {
            button::Status::Hovered | button::Status::Pressed => look.hover,
            _ => look.surface,
        })),
        text_color: if status == button::Status::Disabled { look.muted } else { look.text },
        border: Border { color: look.surface_alt, width: 1.0, radius: 6.0.into() },
        ..button::Style::default()
    }
}

/// A destructive action.
pub fn danger(look: Look) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    move |_, status| button::Style {
        background: Some(Background::Color(blend(look.surface, look.danger, if status == button::Status::Hovered { 0.3 } else { 0.18 }))),
        text_color: look.danger,
        border: rounded(6.0),
        ..button::Style::default()
    }
}

/// A tab: the open one in the canvas colour, joined to the drawing below.
fn tab(look: Look, open: bool) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    move |_, status| {
        let background = match (open, status) {
            (true, _) => Some(look.canvas),
            (false, button::Status::Hovered | button::Status::Pressed) => Some(look.hover),
            _ => None,
        };
        button::Style {
            background: background.map(Background::Color),
            text_color: if open { look.text } else { look.muted },
            border: Border { radius: iced::border::Radius::default().top(6.0), ..Border::default() },
            ..button::Style::default()
        }
    }
}

/// A floating panel: the tool bar, a menu, a dialog.
pub fn island(look: Look) -> impl Fn(&iced::Theme) -> container::Style {
    move |_| container::Style {
        text_color: Some(look.text),
        background: Some(Background::Color(look.surface)),
        border: Border { color: look.surface_alt, width: 1.0, radius: 8.0.into() },
        shadow: Shadow { color: Color { a: if look.dark { 0.4 } else { 0.12 }, ..Color::BLACK }, offset: Vector::new(0.0, 2.0), blur_radius: 10.0 },
        ..container::Style::default()
    }
}

pub fn input(look: Look) -> impl Fn(&iced::Theme, text_input::Status) -> text_input::Style {
    move |_, status| text_input::Style {
        background: Background::Color(look.canvas),
        border: Border {
            color: if matches!(status, text_input::Status::Focused { .. }) { look.accent } else { look.surface_alt },
            width: 1.0,
            radius: 6.0.into(),
        },
        icon: look.muted,
        placeholder: look.muted,
        value: look.text,
        selection: Color { a: 0.35, ..look.accent },
    }
}

pub fn pick(look: Look) -> impl Fn(&iced::Theme, pick_list::Status) -> pick_list::Style {
    move |_, status| pick_list::Style {
        text_color: look.text,
        placeholder_color: look.muted,
        handle_color: look.muted,
        background: Background::Color(if status == pick_list::Status::Hovered { look.hover } else { look.canvas }),
        border: Border { color: look.surface_alt, width: 1.0, radius: 6.0.into() },
    }
}

pub fn pick_menu(look: Look) -> impl Fn(&iced::Theme) -> iced::overlay::menu::Style {
    move |_| iced::overlay::menu::Style {
        background: Background::Color(look.surface),
        border: Border { color: look.surface_alt, width: 1.0, radius: 6.0.into() },
        text_color: look.text,
        selected_text_color: look.text,
        selected_background: Background::Color(look.accent_soft),
        shadow: Shadow::default(),
    }
}

pub fn rail(look: Look) -> impl Fn(&iced::Theme, slider::Status) -> slider::Style {
    move |theme, status| {
        let mut s = slider::default(theme, status);
        s.rail.backgrounds = (Background::Color(look.accent), Background::Color(look.surface_alt));
        s.handle.background = Background::Color(look.accent);
        s.handle.border_color = look.surface;
        s
    }
}

// ------------------------------------------------------------ the tab bar

pub fn tab_bar(app: &App) -> Element<'_, Message> {
    let look = app.look();
    let mut tabs = row![].spacing(2).align_y(Alignment::End);
    for (i, t) in app.tabs.iter().enumerate() {
        let open = i == app.active;
        let label = if t.dirty() { format!("{} •", t.title()) } else { t.title() };
        tabs = tabs.push(
            button(row![small(label), button(small("×")).on_press(Message::CloseTab(t.id.clone())).style(chip(look, false)).padding([0, 4])].spacing(6).align_y(Alignment::Center))
                .on_press(Message::SelectTab(i))
                .style(tab(look, open))
                .padding([5, 10]),
        );
    }
    tabs = tabs.push(button(small("+")).on_press(Message::NewTab).style(chip(look, false)).padding([5, 10]));
    let menu = |label: &str, m: Menu| button(small(label.to_owned())).on_press(Message::ToggleMenu(m)).style(chip(look, app.menu == Some(m))).padding([5, 10]);
    // Everything sits on the bottom edge, where the open tab joins the drawing.
    let bar = row![menu("File", Menu::File), menu("Theme", Menu::Theme), Space::new().width(8), tabs].spacing(2).align_y(Alignment::End).height(Length::Fill);
    let bar = container(bar).padding(iced::Padding { top: 0.0, right: 8.0, bottom: 0.0, left: 8.0 }).width(Length::Fill).height(Length::Fill);
    // A line under it, for themes whose panels and canvas are one colour.
    container(column![bar, rule(look)])
        .height(TAB_BAR)
        .width(Length::Fill)
        .style(move |_| container::Style { background: Some(Background::Color(look.surface)), text_color: Some(look.text), ..container::Style::default() })
        .into()
}

// ------------------------------------------------------------ menus

fn item<'a>(look: Look, label: &str, key: &str, message: Message) -> Element<'a, Message> {
    button(row![small(label.to_owned()), Space::new().width(Length::Fill), small(key.to_owned()).color(look.muted)].width(Length::Fill))
        .on_press(message)
        .style(chip(look, false))
        .width(Length::Fill)
        .into()
}

fn heading<'a>(look: Look, label: &str) -> Element<'a, Message> {
    container(small(label.to_owned()).color(look.muted)).padding([6, 10]).into()
}

fn rule<'a>(look: Look) -> Element<'a, Message> {
    container(Space::new().height(1)).width(Length::Fill).style(move |_| container::Style {
        background: Some(Background::Color(look.surface_alt)),
        ..container::Style::default()
    })
    .into()
}

/// A menu under its button in the tab bar.
fn dropdown<'a>(look: Look, left: f32, items: iced::widget::Column<'a, Message>) -> Element<'a, Message> {
    container(container(items.spacing(0)).padding(4).style(island(look)).width(280))
        .padding(iced::Padding { top: TAB_BAR, left, right: 0.0, bottom: 0.0 })
        .into()
}

pub fn file_menu(app: &App) -> Element<'_, Message> {
    let look = app.look();
    let mut items = column![
        item(look, "New tab", "Ctrl+N", Message::NewTab),
        item(look, "Open…", "Ctrl+O", Message::Open),
        item(look, "Save", "Ctrl+S", Message::Save),
        item(look, "Save as…", "Ctrl+Shift+S", Message::SaveAs),
        item(look, "Close tab", "Ctrl+W", Message::CloseTab(app.tabs[app.active].id.clone())),
    ];
    if !app.recent.is_empty() {
        items = items.push(rule(look)).push(heading(look, "Open recent"));
        for entry in &app.recent {
            items = items.push(item(look, &entry.name, "", Message::OpenPath(entry.path.clone().into())));
        }
        items = items.push(item(look, "Clear recent", "", Message::ClearRecent));
    }
    items = items.push(rule(look)).push(item(look, "Quit", "Ctrl+Q", Message::Quit));
    dropdown(look, 8.0, items)
}

/// Follow the desktop, or one theme; the editor; reading the files again.
pub fn theme_menu(app: &App) -> Element<'_, Message> {
    let look = app.look();
    let selection = &app.themes.settings.theme;
    let check = |on: bool| if on { "✓" } else { "" };
    let system = if app.themes.system_dark { "Dark" } else { "Light" };
    let mut items = column![item(
        look,
        &format!("Follow system ({system})"),
        check(selection == crate::theme::SYSTEM),
        Message::SelectTheme(crate::theme::SYSTEM.into())
    )];
    items = items.push(rule(look));
    for t in app.themes.all() {
        let custom = if app.themes.is_user(&t.id) { " (custom)" } else { "" };
        items = items.push(item(look, &format!("{}{custom}", t.name), check(*selection == t.id), Message::SelectTheme(t.id.clone())));
    }
    items = items.push(rule(look)).push(item(look, "Customise themes…", "Ctrl+,", Message::Theme(crate::theme_editor::Edit::Open))).push(item(look, "Reload user themes", "", Message::ReloadThemes));
    dropdown(look, 52.0, items)
}

// ------------------------------------------------------------ questions

/// A question that must be answered before anything else.
pub fn prompt<'a>(look: Look, prompt: &Prompt) -> Element<'a, Message> {
    let (message, buttons): (String, Vec<(&str, Answer)>) = match prompt {
        Prompt::Unsaved { title, .. } => (
            format!("{title} has unsaved changes. Save before continuing?"),
            vec![("Save", Answer::Save), ("Discard", Answer::Discard), ("Cancel", Answer::Cancel)],
        ),
        Prompt::Recover { what, .. } => (format!("{what} not saved when Excalidraw Desktop last closed. Recover?"), vec![("Recover", Answer::Save), ("Discard", Answer::Discard)]),
        Prompt::Error(e) | Prompt::Info(e) => (e.clone(), vec![("OK", Answer::Cancel)]),
        Prompt::DiscardTheme { name, .. } => (format!("\"{name}\" has unsaved changes. Discard them?"), vec![("Discard", Answer::Discard), ("Keep editing", Answer::Cancel)]),
        Prompt::DeleteTheme { name, .. } => (format!("Delete \"{name}\"?"), vec![("Delete", Answer::Confirm), ("Keep", Answer::Cancel)]),
    };
    let mut row = row![Space::new().width(Length::Fill)].spacing(8);
    for (i, (label, answer)) in buttons.into_iter().enumerate() {
        let b = button(small(label)).on_press(Message::Answer(answer)).padding([6, 12]);
        let b = match (i, answer) {
            (0, Answer::Discard | Answer::Confirm) if !matches!(prompt, Prompt::Recover { .. }) => b.style(danger(look)),
            (0, _) => b.style(primary(look)),
            _ => b.style(secondary(look)),
        };
        row = row.push(b);
    }
    let card = container(column![text(message).size(14), row].spacing(16).width(440)).padding(20).style(island(look));
    // The drawing behind is dimmed and takes no input.
    container(card)
        .center(Length::Fill)
        .style(move |_| container::Style { background: Some(Color { a: 0.6, ..look.canvas }.into()), ..container::Style::default() })
        .into()
}
