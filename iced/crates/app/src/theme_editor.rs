//! The theme editor: a panel down the right of the window, as the Tauri
//! app's (src/components/ThemeEditor.tsx), with the same rules. The whole
//! window paints the draft as it is edited; nothing reaches the settings or
//! the themes directory until Save.
//!
//! One thing differs: there is no desktop colour chooser to hand a field
//! to, so each swatch opens hue, saturation and value sliders under it.

use crate::chrome::{self, small};
use crate::theme::{self, Look, Theme, FIELDS, SYSTEM};
use crate::{App, Message, Prompt};
use iced::widget::{button, checkbox, column, container, pick_list, row, scrollable, slider, text, text_input, Space};
use iced::{Alignment, Background, Border, Color, Element, Length};

pub const WIDTH: f32 = 360.0;

#[derive(Debug, Clone)]
pub enum Edit {
    Open,
    Close,
    /// Appearance: one theme, or `SYSTEM`.
    Use(String),
    /// Appearance, following the desktop: the light and the dark theme.
    Light(String),
    Dark(String),
    /// Load another theme into the editor.
    StartFrom(String),
    Name(String),
    DarkScheme(bool),
    /// A colour field, as typed.
    Color(usize, String),
    /// Open or close a field's sliders.
    Picker(usize),
    /// A field's sliders moved: hue (0–360), saturation and value (0–1).
    Hsv(usize, [f32; 3]),
    Save,
    SaveAsNew,
    Delete,
}

pub struct Panel {
    /// The theme the draft was loaded from: for "unsaved changes", and to
    /// paint with while a colour is half typed.
    pub source: Theme,
    pub draft: Theme,
    /// The field whose sliders are open, and where they stand (kept, not
    /// re-derived from the colour, so a grey keeps its hue).
    pub picker: Option<(usize, [f32; 3])>,
}

impl Panel {
    fn new(theme: Theme) -> Self {
        Self { source: theme.clone(), draft: theme, picker: None }
    }

    pub fn modified(&self) -> bool {
        self.draft != self.source
    }

    /// The draft as it can be painted.
    pub fn paintable(&self) -> Theme {
        theme::paintable(&self.draft, &self.source)
    }
}

/// An entry in a theme list.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Choice {
    id: String,
    label: String,
}

impl std::fmt::Display for Choice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

pub fn hsv([r, g, b]: [u8; 3]) -> [f32; 3] {
    let [r, g, b] = [r, g, b].map(|c| f32::from(c) / 255.0);
    let max = r.max(g).max(b);
    let d = max - r.min(g).min(b);
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    [h, if max == 0.0 { 0.0 } else { d / max }, max]
}

pub fn hex([h, s, v]: [f32; 3]) -> String {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0).rem_euclid(2.0) - 1.0).abs());
    let (r, g, b) = match (h.rem_euclid(360.0) / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    let byte = |f: f32| ((f + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    format!("#{:02x}{:02x}{:02x}", byte(r), byte(g), byte(b))
}

impl App {
    /// Carries out an edit. `confirmed`: the user already agreed to lose an
    /// unsaved draft, if that is what it costs.
    pub fn theme_edit(&mut self, edit: Edit, confirmed: bool) {
        if let Edit::Open = edit {
            if self.panel.is_none() {
                self.panel = Some(Panel::new(self.themes.chosen()));
                // One panel down the right at a time.
                self.library.open = false;
            }
            return;
        }
        let Some(panel) = &mut self.panel else {
            // From the Theme menu with the editor shut.
            if let Edit::Use(id) = edit {
                self.themes.select(&id);
            }
            return;
        };
        // Leaving the draft behind asks first.
        let leaves = matches!(edit, Edit::Close | Edit::Use(_) | Edit::Light(_) | Edit::Dark(_) | Edit::StartFrom(_));
        if leaves && panel.modified() && !confirmed {
            self.prompt = Some(Prompt::DiscardTheme { name: panel.draft.name.clone(), then: edit });
            return;
        }
        match edit {
            Edit::Open => {}
            Edit::Close => self.panel = None,
            // Changing what the app uses loads it into the editor, so the
            // panel and the window never disagree.
            Edit::Use(id) => {
                self.themes.select(&id);
                *panel = Panel::new(self.themes.chosen());
            }
            Edit::Light(id) => {
                let dark = self.themes.settings.dark_theme.clone();
                self.themes.set_pair(&id, &dark);
                *panel = Panel::new(self.themes.chosen());
            }
            Edit::Dark(id) => {
                let light = self.themes.settings.light_theme.clone();
                self.themes.set_pair(&light, &id);
                *panel = Panel::new(self.themes.chosen());
            }
            Edit::StartFrom(id) => {
                if let Some(t) = self.themes.find(&id) {
                    *panel = Panel::new(t);
                }
            }
            Edit::Name(name) => panel.draft.name = name,
            Edit::DarkScheme(dark) => panel.draft.dark = dark,
            Edit::Color(i, value) => {
                *panel.draft.colors.get_mut(i) = value.trim().to_owned();
                if let Some((open, _)) = panel.picker {
                    if open == i {
                        if let Some(rgb) = theme::parse_hex(panel.draft.colors.get(i)) {
                            panel.picker = Some((i, hsv(rgb)));
                        }
                    }
                }
            }
            Edit::Picker(i) => {
                panel.picker = match panel.picker {
                    Some((open, _)) if open == i => None,
                    _ => {
                        let rgb = theme::parse_hex(panel.draft.colors.get(i)).unwrap_or([255, 255, 255]);
                        Some((i, hsv(rgb)))
                    }
                }
            }
            Edit::Hsv(i, value) => {
                panel.picker = Some((i, value));
                *panel.draft.colors.get_mut(i) = hex(value);
            }
            Edit::Save => {
                let draft = panel.draft.clone();
                self.commit(draft);
            }
            Edit::SaveAsNew => {
                // A fresh id: the draft's own stays taken, so an unedited name
                // cannot overwrite the theme it was copied from.
                let taken: Vec<String> = self.themes.all().into_iter().map(|t| t.id).collect();
                let mut draft = panel.draft.clone();
                draft.id = theme::unique_id(&theme::slugify(&draft.name), &taken);
                self.commit(draft);
            }
            Edit::Delete => {
                if confirmed {
                    let id = panel.draft.id.clone();
                    match self.themes.delete(&id) {
                        Ok(()) => self.panel = None,
                        Err(e) => self.prompt = Some(Prompt::Error(format!("Could not delete theme: {e}"))),
                    }
                } else {
                    self.prompt = Some(Prompt::DeleteTheme { name: panel.draft.name.clone() });
                }
            }
        }
    }

    /// Saves a theme, which also puts it on screen for good.
    fn commit(&mut self, mut next: Theme) {
        next.name = next.name.trim().to_owned();
        if next.name.is_empty() {
            self.prompt = Some(Prompt::Error("Give the theme a name first.".into()));
            return;
        }
        let invalid = theme::invalid_fields(&next);
        if !invalid.is_empty() {
            self.prompt = Some(Prompt::Error(format!("Not a colour: {}.\n\nUse a hex value like #1f1f28, # included.", invalid.join(", "))));
            return;
        }
        match self.themes.save(next.clone()) {
            Ok(path) => {
                if let Some(panel) = &mut self.panel {
                    panel.source = next.clone();
                    panel.draft = next;
                }
                if !path.is_empty() {
                    self.status = Some(format!("Saved {path}"));
                }
            }
            Err(e) => self.prompt = Some(Prompt::Error(format!("Could not save theme: {e}"))),
        }
    }

    /// Reads the themes directory again, and says what came of it.
    pub fn reload_themes(&mut self) {
        let (n, errors) = self.themes.reload();
        let dir = excalidraw_desktop_core::settings::themes_dir_path();
        let summary = if n == 1 { "1 user theme".to_owned() } else { format!("{n} user themes") };
        let mut text = format!("Loaded {summary} from {dir}.");
        if !errors.is_empty() {
            text.push_str(&format!("\n\nSkipped:\n{}", errors.join("\n")));
        }
        self.prompt = Some(Prompt::Info(text));
    }
}

fn section<'a>(look: Look, title: String, body: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    column![text(title).size(12).color(look.muted), body.into()].spacing(8).into()
}

fn field<'a>(look: Look, label: &str, control: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    row![small(label.to_owned()).color(look.text).width(Length::FillPortion(2)), container(control).width(Length::FillPortion(3))]
        .spacing(8)
        .align_y(Alignment::Center)
        .into()
}

fn hint<'a>(look: Look, s: String) -> Element<'a, Message> {
    text(s).size(12).color(look.muted).into()
}

fn edit(e: Edit) -> Message {
    Message::Theme(e)
}

/// A colour's swatch: the colour, a slash for `transparent`, the danger
/// colour round it while it is not a colour at all.
fn swatch<'a>(look: Look, i: usize, value: &str) -> Element<'a, Message> {
    let valid = theme::is_color_value(i, value);
    let fill = theme::parse_hex(value).map(|[r, g, b]| Color::from_rgb8(r, g, b));
    let inner = container(if value == theme::TRANSPARENT { small("∅").color(look.muted) } else { small("") }).center(22).style(move |_| container::Style {
        background: fill.map(Background::Color),
        border: Border { color: if valid { look.surface_alt } else { look.danger }, width: if valid { 1.0 } else { 2.0 }, radius: 5.0.into() },
        ..container::Style::default()
    });
    button(inner).on_press(edit(Edit::Picker(i))).padding(0).style(chrome::chip(look, false)).into()
}

pub fn view<'a>(app: &'a App, panel: &'a Panel) -> Element<'a, Message> {
    let look = app.look();
    let themes = app.themes.all();
    let choice = |t: &Theme| Choice { id: t.id.clone(), label: t.name.clone() };
    let choices: Vec<Choice> = themes.iter().map(choice).collect();
    let find = |id: &str| choices.iter().find(|c| c.id == id).cloned();
    let picker = |options: Vec<Choice>, selected: Option<Choice>, on: fn(String) -> Edit| {
        pick_list(options, selected, move |c: Choice| edit(on(c.id))).text_size(13).width(Length::Fill).style(chrome::pick(look)).menu_style(chrome::pick_menu(look))
    };

    let s = &app.themes.settings;
    let mut uses = vec![Choice { id: SYSTEM.into(), label: "Follow system".into() }];
    uses.extend(choices.iter().cloned());
    let selected_use = uses.iter().find(|c| c.id == s.theme).cloned();
    let mut appearance = column![field(look, "Use", picker(uses, selected_use, Edit::Use))].spacing(8);
    if s.theme == SYSTEM {
        appearance = appearance
            .push(field(look, "When the desktop is light", picker(choices.clone(), find(&s.light_theme), Edit::Light)))
            .push(field(look, "When the desktop is dark", picker(choices.clone(), find(&s.dark_theme), Edit::Dark)));
    }

    let start: Vec<Choice> = themes
        .iter()
        .map(|t| Choice { id: t.id.clone(), label: if app.themes.is_user(&t.id) { format!("{} (custom)", t.name) } else { t.name.clone() } })
        .collect();
    let started = start.iter().find(|c| c.id == panel.source.id).cloned();
    let editing = column![
        field(look, "Start from", picker(start, started, Edit::StartFrom)),
        field(look, "Name", text_input("Name", &panel.draft.name).on_input(|n| edit(Edit::Name(n))).size(13).style(chrome::input(look))),
        checkbox(panel.draft.dark).label("Dark colour scheme").on_toggle(|d| edit(Edit::DarkScheme(d))).text_size(13).style(move |theme, status| {
            let mut st = checkbox::primary(theme, status);
            st.text_color = Some(look.text);
            st
        }),
        hint(look, "Affects which way hover shades move and which slot the theme fills when following the desktop. The drawing is never inverted.".into()),
    ]
    .spacing(8);

    let mut colours = column![].spacing(6);
    for (i, (_, label, what)) in FIELDS.iter().enumerate() {
        let value = panel.draft.colors.get(i);
        let valid = theme::is_color_value(i, value);
        let hex_input = text_input("#rrggbb", value).on_input(move |v| edit(Edit::Color(i, v))).size(13).width(100).style(move |t, st| {
            let mut s = chrome::input(look)(t, st);
            if !valid {
                s.border.color = look.danger;
            }
            s
        });
        colours = colours.push(
            row![swatch(look, i, value), column![small(*label).color(look.text), text(*what).size(11).color(look.muted)].width(Length::Fill), hex_input]
                .spacing(10)
                .align_y(Alignment::Center),
        );
        if let Some((open, [h, sat, v])) = panel.picker {
            if open == i {
                let bar = |label: &str, value: f32, max: f32, set: Box<dyn Fn(f32) -> [f32; 3]>| {
                    row![small(label.to_owned()).color(look.muted).width(14), slider(0.0..=max, value, move |x| edit(Edit::Hsv(i, set(x)))).step(max / 360.0).style(chrome::rail(look))]
                        .spacing(8)
                        .align_y(Alignment::Center)
                };
                colours = colours.push(
                    container(
                        column![
                            bar("H", h, 360.0, Box::new(move |x| [x, sat, v])),
                            bar("S", sat, 1.0, Box::new(move |x| [h, x, v])),
                            bar("V", v, 1.0, Box::new(move |x| [h, sat, x])),
                        ]
                        .spacing(4),
                    )
                    .padding(iced::Padding { top: 0.0, right: 0.0, bottom: 6.0, left: 32.0 }),
                );
            }
        }
    }
    colours = colours.push(hint(look, "Hex values, # included; transparent is accepted for Fill only. Click a swatch for sliders.".into()));

    let is_user = app.themes.is_user(&panel.draft.id);
    let dir = excalidraw_desktop_core::settings::themes_dir_path();
    let files = column![
        hint(
            look,
            format!(
                "Save writes {}.json{}. Save as new derives a fresh id from the name and leaves the original alone.",
                panel.draft.id,
                if is_user { "" } else { ", which from then on replaces the built-in theme of that id" }
            )
        ),
        hint(look, format!("Themes are plain JSON in {dir}. Editing one by hand and choosing Reload user themes works just as well.")),
    ]
    .spacing(6);

    let title = if panel.modified() { "Edit — unsaved changes" } else { "Edit" };
    let body = column![
        section(look, "APPEARANCE".into(), appearance),
        section(look, title.to_uppercase(), editing),
        section(look, "COLOURS".into(), colours),
        section(look, "FILES".into(), files),
    ]
    .spacing(20)
    .padding(iced::Padding { top: 4.0, right: 16.0, bottom: 16.0, left: 16.0 });

    let header = row![text("Themes").size(16).color(look.text), Space::new().width(Length::Fill), button(small("×")).on_press(edit(Edit::Close)).style(chrome::chip(look, false))]
        .align_y(Alignment::Center)
        .padding([10, 16]);
    let mut footer = row![
        button(small("Save")).on_press(edit(Edit::Save)).style(chrome::primary(look)).padding([6, 12]),
        button(small("Save as new")).on_press_maybe((!panel.draft.name.trim().is_empty()).then(|| edit(Edit::SaveAsNew))).style(chrome::secondary(look)).padding([6, 12]),
    ]
    .spacing(8);
    if is_user {
        footer = footer.push(button(small("Delete")).on_press(edit(Edit::Delete)).style(chrome::danger(look)).padding([6, 12]));
    }
    footer = footer.push(Space::new().width(Length::Fill)).push(button(small("Close")).on_press(edit(Edit::Close)).style(chrome::secondary(look)).padding([6, 12]));

    let rule = container(Space::new().height(1)).width(Length::Fill).style(move |_| container::Style { background: Some(Background::Color(look.surface_alt)), ..container::Style::default() });
    let card = column![header, rule, scrollable(body).spacing(2).height(Length::Fill), container(footer).padding([10, 16])];
    container(container(card).width(WIDTH).height(Length::Fill).style(move |t| container::Style { border: Border { radius: 0.0.into(), ..chrome::island(look)(t).border }, ..chrome::island(look)(t) }))
        .align_right(Length::Fill)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hsv_round_trips() {
        for c in ["#1f1f28", "#ffffff", "#000000", "#6965db", "#e82424", "#7e9cd8", "#808080"] {
            let rgb = theme::parse_hex(c).unwrap();
            assert_eq!(hex(hsv(rgb)), c);
        }
    }
}
