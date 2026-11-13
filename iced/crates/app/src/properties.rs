//! The properties panel down the left, as Excalidraw 0.18.1 shows it for
//! the tool in hand or the selection: stroke and background colours from
//! its palette (or any hex), fill, stroke width and style, sloppiness,
//! edges, arrow type and arrowheads, font, size and alignment, opacity,
//! then the layer and action buttons. What is shown and what a change does
//! is the editor's (`editor::Panel`, `Editor::set_prop`).

use crate::chrome::{self, chip, island, small};
use crate::icons::{icon, swatch, Head, Icon};
use crate::theme::Look;
use crate::{App, Message};
use editor::{Panel, Prop, FONT_SIZES};
use iced::widget::{button, column, container, opaque, row, scrollable, slider, text, text_input, tooltip, Space};
use iced::{Alignment, Color, Element, Length};

pub const WIDTH: f32 = 212.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Popup {
    Stroke,
    Background,
    StartHead,
    EndHead,
    Fonts,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    ToBack,
    Backward,
    Forward,
    ToFront,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Msg {
    Set(Prop),
    /// Opens a popup, or closes it if it is open.
    Popup(Popup),
    /// The hex field of the open colour popup, as typed.
    Hex(String),
    Layer(Layer),
    Duplicate,
    Delete,
    Group,
    Ungroup,
}

fn msg(m: Msg) -> Message {
    Message::Props(m)
}

/// `DEFAULT_ELEMENT_STROKE_PICKS` and `DEFAULT_ELEMENT_BACKGROUND_PICKS`.
const STROKE_PICKS: [&str; 5] = ["#1e1e1e", "#e03131", "#2f9e44", "#1971c2", "#f08c00"];
const BACKGROUND_PICKS: [&str; 5] = ["transparent", "#ffc9c9", "#b2f2bb", "#a5d8ff", "#ffec99"];

/// Excalidraw's palette (`DEFAULT_ELEMENT_*_COLOR_PALETTE`), in its order:
/// a colour, or a hue as open-color shades 0, 2, 4, 6 and 8 (radix for
/// bronze).
const PALETTE: [(&str, &[&str]); 15] = [
    ("Transparent", &["transparent"]),
    ("White", &["#ffffff"]),
    ("Gray", &["#f8f9fa", "#e9ecef", "#ced4da", "#868e96", "#343a40"]),
    ("Black", &["#1e1e1e"]),
    ("Bronze", &["#f8f1ee", "#eaddd7", "#d2bab0", "#a18072", "#846358"]),
    ("Cyan", &["#e3fafc", "#99e9f2", "#3bc9db", "#15aabf", "#0c8599"]),
    ("Blue", &["#e7f5ff", "#a5d8ff", "#4dabf7", "#228be6", "#1971c2"]),
    ("Violet", &["#f3f0ff", "#d0bfff", "#9775fa", "#7950f2", "#6741d9"]),
    ("Grape", &["#f8f0fc", "#eebefa", "#da77f2", "#be4bdb", "#9c36b5"]),
    ("Pink", &["#fff0f6", "#fcc2d7", "#f783ac", "#e64980", "#c2255c"]),
    ("Green", &["#ebfbee", "#b2f2bb", "#69db7c", "#40c057", "#2f9e44"]),
    ("Teal", &["#e6fcf5", "#96f2d7", "#38d9a9", "#12b886", "#099268"]),
    ("Yellow", &["#fff9db", "#ffec99", "#ffd43b", "#fab005", "#f08c00"]),
    ("Orange", &["#fff4e6", "#ffd8a8", "#ffa94d", "#fd7e14", "#e8590c"]),
    ("Red", &["#fff5f5", "#ffc9c9", "#ff8787", "#fa5252", "#e03131"]),
];

/// Where `c` is in the palette: the colour's index and its shade.
fn find(c: &str) -> Option<(usize, usize)> {
    PALETTE.iter().enumerate().find_map(|(i, (_, shades))| shades.iter().position(|s| s.eq_ignore_ascii_case(c)).map(|k| (i, k)))
}

/// The fonts offered: Excalidraw's three quick picks, then the rest of
/// what it lists. Virgil, Helvetica and Cascadia are only offered while
/// the selection uses them, as Excalidraw lists a drawing's own fonts.
const QUICK_FONTS: [(f64, &str); 3] = [(5.0, "Hand-drawn"), (6.0, "Normal"), (8.0, "Code")];
const FONTS: [(f64, &str); 8] = [
    (5.0, "Excalifont"),
    (6.0, "Nunito"),
    (7.0, "Lilita One"),
    (8.0, "Comic Shanns"),
    (9.0, "Liberation Sans"),
    (1.0, "Virgil"),
    (2.0, "Helvetica"),
    (3.0, "Cascadia"),
];

/// A colour as the panel paints it: `None` for transparent.
fn paint(c: &str) -> Option<Color> {
    if c == "transparent" {
        return None;
    }
    let p = render::color::parse(c);
    Some(Color::from_rgba(p.r, p.g, p.b, p.a))
}

/// A hex colour as typed (`#` optional), normalised; `None` until valid.
pub fn parse_hex(typed: &str) -> Option<String> {
    let h = typed.trim().trim_start_matches('#').to_ascii_lowercase();
    let ok = matches!(h.len(), 3 | 6) && h.chars().all(|c| c.is_ascii_hexdigit());
    ok.then(|| format!("#{h}"))
}

fn tip<'a>(look: Look, content: impl Into<Element<'a, Message>>, label: &str) -> Element<'a, Message> {
    tooltip(content, container(text(label.to_owned()).size(12)).padding([4, 8]).style(move |t| island(look)(t)), tooltip::Position::Bottom).gap(4).into()
}

/// An icon button that shows whether it is the current choice.
fn choice<'a>(look: Look, i: Icon, on: bool, m: Msg, label: &str) -> Element<'a, Message> {
    tip(look, button(icon(i, look.text, 18.0)).on_press(msg(m)).padding(5).style(chip(look, on)), label)
}

fn section<'a>(look: Look, title: &str, content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    column![small(title.to_owned()).size(12).color(look.muted), content.into()].spacing(5).into()
}

fn swatch_button<'a>(look: Look, c: &str, on: bool, m: Msg, size: f32) -> Element<'a, Message> {
    button(swatch(paint(c), look.surface_alt, size)).on_press(msg(m)).padding(2).style(chip(look, on)).into()
}

impl App {
    pub fn props(&mut self, m: Msg) {
        let editor = &mut self.tabs[self.active].doc.editor;
        match m {
            Msg::Set(p) => {
                // A colour picked from the palette closes it.
                if matches!(p, Prop::StartArrowhead(_) | Prop::EndArrowhead(_) | Prop::FontFamily(_)) {
                    self.popup = None;
                }
                editor.set_prop(p);
            }
            Msg::Popup(p) => {
                if self.popup == Some(p) {
                    self.popup = None;
                } else {
                    let panel = editor.panel().unwrap_or_default();
                    let current = match p {
                        Popup::Stroke => panel.stroke_color.flatten(),
                        Popup::Background => panel.background_color.flatten(),
                        _ => None,
                    };
                    self.hex = current.filter(|c| c != "transparent").map(|c| c.trim_start_matches('#').to_owned()).unwrap_or_default();
                    self.popup = Some(p);
                }
            }
            Msg::Hex(typed) => {
                if let Some(c) = parse_hex(&typed) {
                    match self.popup {
                        Some(Popup::Stroke) => editor.set_prop(Prop::StrokeColor(c)),
                        Some(Popup::Background) => editor.set_prop(Prop::BackgroundColor(c)),
                        _ => {}
                    }
                }
                self.hex = typed.trim_start_matches('#').to_owned();
            }
            Msg::Layer(l) => match l {
                Layer::ToBack => editor.send_to_back(),
                Layer::Backward => editor.send_backward(),
                Layer::Forward => editor.bring_forward(),
                Layer::ToFront => editor.bring_to_front(),
            },
            Msg::Duplicate => editor.duplicate(),
            Msg::Delete => editor.delete_selected(),
            Msg::Group => editor.group(),
            Msg::Ungroup => editor.ungroup(),
        }
    }
}

fn colour_section<'a>(app: &'a App, look: Look, which: Popup, current: Option<String>) -> Element<'a, Message> {
    let (title, picks, set): (&str, [&str; 5], fn(String) -> Prop) = match which {
        Popup::Stroke => ("Stroke", STROKE_PICKS, Prop::StrokeColor),
        _ => ("Background", BACKGROUND_PICKS, Prop::BackgroundColor),
    };
    let cur = current.clone().unwrap_or_default();
    let mut quick = row![].spacing(2).align_y(Alignment::Center);
    for c in picks {
        quick = quick.push(swatch_button(look, c, cur.eq_ignore_ascii_case(c), Msg::Set(set(c.to_owned())), 18.0));
    }
    let open = app.popup == Some(which);
    let divider = container(Space::new().width(1).height(20)).style(move |_| container::Style { background: Some(look.surface_alt.into()), ..container::Style::default() });
    quick = quick.push(divider).push(tip(
        look,
        button(swatch(current.as_deref().and_then(paint), look.muted, 22.0)).on_press(msg(Msg::Popup(which))).padding(2).style(chip(look, open)),
        "More colours",
    ));
    let mut body = column![quick].spacing(6);
    if open {
        // `DEFAULT_ELEMENT_STROKE_COLOR_INDEX`, `…_BACKGROUND_COLOR_INDEX`.
        let shade = if which == Popup::Stroke { 4 } else { 1 };
        body = body.push(palette(app, look, &cur, set, shade));
    }
    section(look, title, body)
}

/// The palette popup, as Excalidraw lays it out: the fifteen colours, each
/// at the shade in use (the default one for the kind of colour otherwise),
/// the five shades of the current one, and a hex field.
fn palette<'a>(app: &'a App, look: Look, current: &str, set: fn(String) -> Prop, default_shade: usize) -> Element<'a, Message> {
    let found = find(current);
    let shade = found.filter(|(i, _)| PALETTE[*i].1.len() > 1).map_or(default_shade, |(_, k)| k);
    let mut grid = column![].spacing(3);
    for chunk in PALETTE.chunks(5) {
        let mut r = row![].spacing(3);
        for (name, shades) in chunk {
            let c = shades[shade.min(shades.len() - 1)];
            let on = found.is_some_and(|(i, _)| PALETTE[i].0 == *name);
            r = r.push(tip(look, swatch_button(look, c, on, Msg::Set(set(c.to_owned())), 20.0), name));
        }
        grid = grid.push(r);
    }
    let mut body = column![grid].spacing(8);
    if let Some((i, k)) = found.filter(|(i, _)| PALETTE[*i].1.len() > 1) {
        let mut r = row![].spacing(3);
        for (j, c) in PALETTE[i].1.iter().enumerate() {
            r = r.push(swatch_button(look, c, j == k, Msg::Set(set((*c).to_owned())), 20.0));
        }
        body = body.push(column![small("Shades").size(11).color(look.muted), r].spacing(3));
    }
    let valid = app.hex.is_empty() || parse_hex(&app.hex).is_some();
    let hex = text_input("hex", &app.hex).on_input(|h| msg(Msg::Hex(h))).size(13).padding([3, 6]).style(move |t, s| {
        let mut st = chrome::input(look)(t, s);
        if !valid {
            st.border.color = look.danger;
        }
        st
    });
    body = body.push(row![small("#").color(look.muted), hex].spacing(4).align_y(Alignment::Center));
    container(body).padding(6).style(move |_| container::Style {
        background: Some(look.canvas.into()),
        border: iced::Border { color: look.surface_alt, width: 1.0, radius: 6.0.into() },
        ..container::Style::default()
    })
    .into()
}

fn heads<'a>(app: &'a App, look: Look, start: Option<String>, end: Option<String>) -> Element<'a, Message> {
    let (s, e) = (Head::of(start.as_deref()), Head::of(end.as_deref()));
    let pick = |head: Head, is_start: bool, popup: Popup| {
        tip(
            look,
            button(icon(Icon::Head(head, is_start), look.text, 18.0)).on_press(msg(Msg::Popup(popup))).padding(5).style(chip(look, app.popup == Some(popup))),
            if is_start { "Start arrowhead" } else { "End arrowhead" },
        )
    };
    let mut body = column![row![pick(s, true, Popup::StartHead), pick(e, false, Popup::EndHead)].spacing(4)].spacing(6);
    if let Some(p @ (Popup::StartHead | Popup::EndHead)) = app.popup {
        let is_start = p == Popup::StartHead;
        let current = if is_start { s } else { e };
        let mut grid = column![].spacing(2);
        for chunk in Head::ALL.chunks(4) {
            let mut r = row![].spacing(2);
            for h in chunk {
                let name = h.name().map(str::to_owned);
                let set = if is_start { Prop::StartArrowhead(name) } else { Prop::EndArrowhead(name) };
                r = r.push(choice(look, Icon::Head(*h, is_start), *h == current, Msg::Set(set), h.label()));
            }
            grid = grid.push(r);
        }
        body = body.push(grid);
    }
    section(look, "Arrowheads", body)
}

fn fonts<'a>(app: &'a App, look: Look, current: Option<f64>) -> Element<'a, Message> {
    let sample = |family: f64, label: &str, on: bool| -> Element<'a, Message> {
        tip(
            look,
            button(text("Aa").size(16).font(crate::paint::font(family)).color(look.text)).on_press(msg(Msg::Set(Prop::FontFamily(family)))).padding([3, 8]).style(chip(look, on)),
            label,
        )
    };
    let mut quick = row![].spacing(2).align_y(Alignment::Center);
    for (f, label) in QUICK_FONTS {
        quick = quick.push(sample(f, label, current == Some(f)));
    }
    let open = app.popup == Some(Popup::Fonts);
    quick = quick.push(tip(look, button(small("…")).on_press(msg(Msg::Popup(Popup::Fonts))).padding([4, 8]).style(chip(look, open)), "All fonts"));
    let mut body = column![quick].spacing(4);
    if open {
        let mut list = column![].spacing(1);
        for (f, name) in FONTS {
            let deprecated = matches!(f as i64, 1..=3);
            if deprecated && current != Some(f) {
                continue;
            }
            list = list.push(
                button(text(name).size(15).font(crate::paint::font(f)).color(look.text))
                    .on_press(msg(Msg::Set(Prop::FontFamily(f))))
                    .width(Length::Fill)
                    .padding([3, 8])
                    .style(chip(look, current == Some(f))),
            );
        }
        body = body.push(list);
    }
    section(look, "Font family", body)
}

pub fn view(app: &App) -> Option<Element<'_, Message>> {
    let editor = &app.tab().doc.editor;
    let p: Panel = editor.panel()?;
    let look = app.look();
    let mut col = column![].spacing(12);
    if let Some(c) = p.stroke_color.clone() {
        col = col.push(colour_section(app, look, Popup::Stroke, c));
    }
    if let Some(c) = p.background_color.clone() {
        col = col.push(colour_section(app, look, Popup::Background, c));
    }
    let buttons = |items: Vec<(Icon, bool, Msg, &str)>| {
        let mut r = row![].spacing(2);
        for (i, on, m, label) in items {
            r = r.push(choice(look, i, on, m, label));
        }
        r
    };
    if let Some(v) = &p.fill_style {
        let v = v.as_deref();
        col = col.push(section(
            look,
            "Fill",
            buttons(vec![
                (Icon::FillHachure, v == Some("hachure"), Msg::Set(Prop::FillStyle("hachure".into())), "Hachure"),
                (Icon::FillCrossHatch, v == Some("cross-hatch"), Msg::Set(Prop::FillStyle("cross-hatch".into())), "Cross-hatch"),
                (Icon::FillSolid, v == Some("solid"), Msg::Set(Prop::FillStyle("solid".into())), "Solid"),
            ]),
        ));
    }
    if let Some(v) = p.stroke_width {
        col = col.push(section(
            look,
            "Stroke width",
            buttons(vec![
                (Icon::StrokeWidth(1), v == Some(1.0), Msg::Set(Prop::StrokeWidth(1.0)), "Thin"),
                (Icon::StrokeWidth(2), v == Some(2.0), Msg::Set(Prop::StrokeWidth(2.0)), "Bold"),
                (Icon::StrokeWidth(4), v == Some(4.0), Msg::Set(Prop::StrokeWidth(4.0)), "Extra bold"),
            ]),
        ));
    }
    if let Some(v) = &p.stroke_style {
        let v = v.as_deref();
        col = col.push(section(
            look,
            "Stroke style",
            buttons(vec![
                (Icon::StrokeSolid, v == Some("solid"), Msg::Set(Prop::StrokeStyle("solid".into())), "Solid"),
                (Icon::StrokeDashed, v == Some("dashed"), Msg::Set(Prop::StrokeStyle("dashed".into())), "Dashed"),
                (Icon::StrokeDotted, v == Some("dotted"), Msg::Set(Prop::StrokeStyle("dotted".into())), "Dotted"),
            ]),
        ));
    }
    if let Some(v) = p.roughness {
        col = col.push(section(
            look,
            "Sloppiness",
            buttons(vec![
                (Icon::Sloppiness(0), v == Some(0.0), Msg::Set(Prop::Roughness(0.0)), "Architect"),
                (Icon::Sloppiness(1), v == Some(1.0), Msg::Set(Prop::Roughness(1.0)), "Artist"),
                (Icon::Sloppiness(2), v == Some(2.0), Msg::Set(Prop::Roughness(2.0)), "Cartoonist"),
            ]),
        ));
    }
    if let Some(v) = p.round {
        col = col.push(section(
            look,
            "Edges",
            buttons(vec![
                (Icon::EdgeSharp, v == Some(false), Msg::Set(Prop::Round(false)), "Sharp"),
                (Icon::EdgeRound, v == Some(true), Msg::Set(Prop::Round(true)), "Round"),
            ]),
        ));
    }
    if let Some(v) = p.arrow_round {
        col = col.push(section(
            look,
            "Arrow type",
            buttons(vec![
                (Icon::ArrowSharp, v == Some(false), Msg::Set(Prop::ArrowRound(false)), "Sharp"),
                (Icon::ArrowRound, v == Some(true), Msg::Set(Prop::ArrowRound(true)), "Curved"),
            ]),
        ));
    }
    if let Some((s, e)) = p.arrowheads.clone() {
        col = col.push(heads(app, look, s, e));
    }
    if let Some(v) = p.font_family {
        col = col.push(fonts(app, look, v));
    }
    if let Some(v) = p.font_size {
        let mut r = row![].spacing(2);
        for (size, label, name) in FONT_SIZES.iter().zip(["S", "M", "L", "XL"]).zip(["Small", "Medium", "Large", "Very large"]).map(|((s, l), n)| (*s, l, n)) {
            r = r.push(tip(
                look,
                button(text(label).size(13).color(look.text).width(22).align_x(Alignment::Center)).on_press(msg(Msg::Set(Prop::FontSize(size)))).padding([4, 4]).style(chip(look, v == Some(size))),
                name,
            ));
        }
        col = col.push(section(look, "Font size", r));
    }
    if let Some(v) = &p.text_align {
        let v = v.as_deref();
        col = col.push(section(
            look,
            "Text align",
            buttons(vec![
                (Icon::AlignLeft, v == Some("left"), Msg::Set(Prop::TextAlign("left".into())), "Left"),
                (Icon::AlignCenter, v == Some("center"), Msg::Set(Prop::TextAlign("center".into())), "Centre"),
                (Icon::AlignRight, v == Some("right"), Msg::Set(Prop::TextAlign("right".into())), "Right"),
            ]),
        ));
    }
    if let Some(v) = &p.vertical_align {
        let v = v.as_deref();
        col = col.push(section(
            look,
            "Vertical align",
            buttons(vec![
                (Icon::AlignTop, v == Some("top"), Msg::Set(Prop::VerticalAlign("top".into())), "Top"),
                (Icon::AlignMiddle, v == Some("middle"), Msg::Set(Prop::VerticalAlign("middle".into())), "Middle"),
                (Icon::AlignBottom, v == Some("bottom"), Msg::Set(Prop::VerticalAlign("bottom".into())), "Bottom"),
            ]),
        ));
    }
    if let Some(o) = p.opacity {
        let s = slider(0.0..=100.0, o, |v| msg(Msg::Set(Prop::Opacity(v.round())))).step(10.0).style(chrome::rail(look));
        col = col.push(section(look, "Opacity", row![s, small(format!("{o:.0}")).color(look.muted).width(26)].spacing(8).align_y(Alignment::Center)));
    }
    if p.actions {
        col = col.push(section(
            look,
            "Layers",
            buttons(vec![
                (Icon::SendToBack, false, Msg::Layer(Layer::ToBack), "Send to back — Ctrl+Shift+["),
                (Icon::SendBackward, false, Msg::Layer(Layer::Backward), "Send backward — Ctrl+["),
                (Icon::BringForward, false, Msg::Layer(Layer::Forward), "Bring forward — Ctrl+]"),
                (Icon::BringToFront, false, Msg::Layer(Layer::ToFront), "Bring to front — Ctrl+Shift+]"),
            ]),
        ));
        let selected = editor.selection.len();
        let grouped = editor.selection.iter().any(|id| editor.scene.by_id(id).is_some_and(|e| !e.group_ids.is_empty()));
        let mut actions = vec![(Icon::Duplicate, false, Msg::Duplicate, "Duplicate — Ctrl+D"), (Icon::Delete, false, Msg::Delete, "Delete")];
        if selected > 1 {
            actions.push((Icon::Group, false, Msg::Group, "Group — Ctrl+G"));
        }
        if grouped {
            actions.push((Icon::Ungroup, false, Msg::Ungroup, "Ungroup — Ctrl+Shift+G"));
        }
        col = col.push(section(look, "Actions", buttons(actions)));
    }
    let card = container(scrollable(container(col).padding(10)).spacing(2)).width(WIDTH).max_height(app.size.height - 70.0).style(island(look));
    Some(container(opaque(card)).padding(iced::Padding { top: 60.0, left: 12.0, right: 0.0, bottom: 12.0 }).into())
}
