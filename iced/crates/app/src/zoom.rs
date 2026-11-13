//! Zoom, as Excalidraw 0.18.1 has it: the −, percentage and + buttons at
//! the bottom left, steps of 10% (`ZOOM_STEP`) about the middle of the
//! view, the percentage to go back to 100%, and Shift+1 / Shift+2 to fit
//! the drawing or the selection.

use crate::chrome::{chip, island, small};
use crate::doc::fit;
use crate::{App, Message};
use iced::widget::{button, container, opaque, row, text, tooltip};
use iced::{Alignment, Element, Length, Point};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Zoom {
    In,
    Out,
    Reset,
    /// Shift+1.
    Fit,
    /// Shift+2.
    Selection,
}

/// `ZOOM_STEP`.
const STEP: f32 = 0.1;

impl App {
    pub fn zoom(&mut self, z: Zoom) {
        let centre = Point::new(self.size.width / 2.0, self.size.height / 2.0);
        let view = self.tab().view;
        let next = match z {
            Zoom::In => view.zoom_to(view.zoom + STEP, centre),
            Zoom::Out => view.zoom_to(view.zoom - STEP, centre),
            Zoom::Reset => view.zoom_to(1.0, centre),
            Zoom::Fit => fit(self.tab().doc.layers.bounds(), self.size),
            Zoom::Selection => match self.tab().doc.editor.selection_bounds() {
                Some(b) => fit(Some(b), self.size),
                None => return,
            },
        };
        self.set_view(next);
    }
}

pub fn view(app: &App) -> Element<'_, Message> {
    let look = app.look();
    let tip = |b: iced::widget::Button<'static, Message>, label: &str| -> Element<'static, Message> {
        tooltip(b, container(text(label.to_owned()).size(12)).padding([4, 8]).style(move |t| island(look)(t)), tooltip::Position::Top).gap(6).into()
    };
    let percent = format!("{:.0}%", app.tab().view.zoom * 100.0);
    let buttons = row![
        tip(button(small("−")).on_press(Message::Zoom(Zoom::Out)).padding([4, 10]).style(chip(look, false)), "Zoom out — Ctrl+−"),
        tip(button(small(percent).width(44).align_x(Alignment::Center)).on_press(Message::Zoom(Zoom::Reset)).padding([4, 4]).style(chip(look, false)), "Reset zoom — Ctrl+0"),
        tip(button(small("+")).on_press(Message::Zoom(Zoom::In)).padding([4, 10]).style(chip(look, false)), "Zoom in — Ctrl++"),
    ]
    .align_y(Alignment::Center);
    let island = container(buttons).padding(3).style(island(look));
    container(opaque(island)).align_bottom(Length::Fill).padding(iced::Padding { left: 12.0, bottom: 12.0, top: 0.0, right: 0.0 }).into()
}
