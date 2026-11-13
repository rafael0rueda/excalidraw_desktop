//! The properties panel's model: which sections Excalidraw 0.18.1's
//! `SelectedShapeActions` shows for the tool and the selection, the value
//! each one shows (`getFormValue`), and what changing one does
//! (`actions/actionProperties.tsx`, `actions/actionStyles.ts`).
//!
//! A change applies to the selection and becomes what new elements look
//! like (`currentItem*`), as in Excalidraw. It is one undo step; while text
//! is being typed it joins the typing's step instead.

use crate::{text, Editor, Tool};
use scene::edit::{random_integer, update, update_num, Updates};
use scene::{Element, Kind};
use serde_json::{json, Value};

/// One change from the panel.
#[derive(Clone, Debug, PartialEq)]
pub enum Prop {
    StrokeColor(String),
    BackgroundColor(String),
    FillStyle(String),
    StrokeWidth(f64),
    StrokeStyle(String),
    /// Sloppiness: 0 architect, 1 artist, 2 cartoonist.
    Roughness(f64),
    /// Edges: round or sharp.
    Round(bool),
    /// Arrow type: round (curved) or sharp.
    ArrowRound(bool),
    StartArrowhead(Option<String>),
    EndArrowhead(Option<String>),
    FontFamily(f64),
    FontSize(f64),
    TextAlign(String),
    VerticalAlign(String),
    Opacity(f64),
}

/// One section of the panel: `None` when it is not shown, `Some(None)` when
/// shown with nothing picked (the selection disagrees).
pub type Field<T> = Option<Option<T>>;

/// What the panel shows. Only made when the panel is shown at all.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Panel {
    pub stroke_color: Field<String>,
    pub background_color: Field<String>,
    pub fill_style: Field<String>,
    pub stroke_width: Field<f64>,
    pub stroke_style: Field<String>,
    pub roughness: Field<f64>,
    pub round: Field<bool>,
    pub arrow_round: Field<bool>,
    /// Both ends: shown together.
    pub arrowheads: Option<(Option<String>, Option<String>)>,
    pub font_family: Field<f64>,
    pub font_size: Field<f64>,
    pub text_align: Field<String>,
    pub vertical_align: Field<String>,
    pub opacity: Option<f64>,
    /// Something is selected: the layer and action buttons.
    pub actions: bool,
}

/// Font sizes S, M, L, XL.
pub const FONT_SIZES: [f64; 4] = [16.0, 20.0, 28.0, 36.0];
/// `FONT_SIZE_RELATIVE_INCREASE_STEP`, for Ctrl+Shift+< and >.
const FONT_SIZE_STEP: f64 = 0.1;

// The type predicates, by element type name as Excalidraw has them.

fn has_background(t: &str) -> bool {
    matches!(t, "rectangle" | "iframe" | "embeddable" | "ellipse" | "diamond" | "line" | "freedraw")
}
fn has_stroke_color(t: &str) -> bool {
    !matches!(t, "image" | "frame" | "magicframe")
}
fn has_stroke_width(t: &str) -> bool {
    matches!(t, "rectangle" | "iframe" | "embeddable" | "ellipse" | "diamond" | "freedraw" | "arrow" | "line")
}
fn has_stroke_style(t: &str) -> bool {
    matches!(t, "rectangle" | "iframe" | "embeddable" | "ellipse" | "diamond" | "arrow" | "line")
}
fn can_change_roundness(t: &str) -> bool {
    matches!(t, "rectangle" | "iframe" | "embeddable" | "line" | "diamond" | "image")
}

fn type_name(e: &Element) -> &'static str {
    match e.kind {
        Kind::Rectangle => "rectangle",
        Kind::Diamond => "diamond",
        Kind::Ellipse => "ellipse",
        Kind::Line => "line",
        Kind::Arrow => "arrow",
        Kind::FreeDraw => "freedraw",
        Kind::Text => "text",
        Kind::Image => "image",
        Kind::Frame => "frame",
        Kind::MagicFrame => "magicframe",
        Kind::Iframe => "iframe",
        Kind::Embeddable => "embeddable",
        Kind::Unknown => "",
    }
}

fn tool_name(tool: Tool) -> &'static str {
    match tool {
        Tool::Selection => "selection",
        Tool::Hand => "hand",
        Tool::Text => "text",
        t => t.element_type().unwrap_or(""),
    }
}

fn show<T>(yes: bool, v: Option<T>) -> Field<T> {
    yes.then_some(v)
}

/// `getCommonAttributeOfSelectedElements`: the value when all agree.
fn common<T: PartialEq>(values: impl Iterator<Item = Option<T>>) -> Option<T> {
    let mut out: Option<Option<T>> = None;
    for v in values {
        match &out {
            None => out = Some(v),
            Some(first) if *first != v => return None,
            Some(_) => {}
        }
    }
    out.flatten()
}

impl Editor {
    /// `getTargetElements`: the text being typed, or the selection with
    /// its labels.
    fn targets(&self) -> Vec<Element> {
        if let Some(edit) = &self.editing {
            return self.scene.by_id(&edit.id).cloned().into_iter().collect();
        }
        self.with_labels(self.selected_in_order())
            .iter()
            .filter_map(|id| self.visible(id).cloned())
            .collect()
    }

    fn label_of(&self, e: &Element) -> Option<Element> {
        self.bound_text_of(&e.id).and_then(|t| self.visible(&t).cloned())
    }

    /// `getFormValue`: the text being typed's value; else the selection's,
    /// if they agree, or `mixed`; else `none`, for no selection.
    fn form<T: PartialEq + Clone>(
        &self,
        get: impl Fn(&Element) -> Option<T>,
        relevant: impl Fn(&Element) -> bool,
        mixed: Option<T>,
        none: Option<T>,
    ) -> Option<T> {
        if let Some(e) = self.editing.as_ref().and_then(|t| self.scene.by_id(&t.id)) {
            if let Some(v) = get(e) {
                return Some(v);
            }
        }
        let selected = self.selected();
        if selected.is_empty() {
            return none;
        }
        common(selected.into_iter().filter(|e| relevant(e)).map(get)).or(mixed)
    }

    /// A text element's own value, or its label's for a shape.
    fn text_value<T>(&self, e: &Element, get: impl Fn(&scene::Text) -> T) -> Option<T> {
        match &e.text {
            Some(t) => Some(get(t)),
            None => self.label_of(e).and_then(|l| l.text.as_ref().map(get)),
        }
    }

    fn has_text(&self, e: &Element) -> bool {
        e.text.is_some() || self.label_of(e).is_some()
    }

    /// The panel for the tool and selection, or `None` when Excalidraw shows
    /// none (`showSelectedShapeActions`).
    pub fn panel(&self) -> Option<Panel> {
        let tool = tool_name(self.tool);
        let drawing = !matches!(self.tool, Tool::Selection | Tool::Hand);
        if !drawing && self.editing.is_none() && self.selection.is_empty() {
            return None;
        }
        let targets = self.targets();
        let any = |f: &dyn Fn(&str) -> bool| targets.iter().any(|e| f(type_name(e)));
        let s = &self.style;

        let common_type = common(targets.iter().map(|e| Some(type_name(e))));
        let stroke_shown = (has_stroke_color(tool)
            && !matches!(common_type, Some("image" | "frame" | "magicframe")))
            || any(&has_stroke_color);
        let background_shown = has_background(tool) || any(&has_background);
        let fill_shown = (has_background(tool) && s.background_color != "transparent")
            || targets
                .iter()
                .any(|e| has_background(type_name(e)) && e.background_color != "transparent");
        let arrow_shown = tool == "arrow" || any(&|t| t == "arrow");
        let bound_to_shape = |e: &Element| {
            e.text
                .as_ref()
                .and_then(|t| t.container_id.as_deref())
                .and_then(|c| self.visible(c))
                .is_some_and(|c| c.kind != Kind::Arrow)
        };
        let text_shown = tool == "text" || targets.iter().any(|e| e.kind == Kind::Text);
        let align_shown = tool == "text"
            || targets
                .iter()
                .any(|e| bound_to_shape(e) || (e.kind == Kind::Text && e.text.as_ref().is_some_and(|t| t.container_id.is_none())));
        let has_legacy = targets.iter().any(|e| e.roundness.is_some_and(|r| r.kind == 1.0));

        Some(Panel {
            stroke_color: show(
                stroke_shown,
                self.form(|e| Some(e.stroke_color.clone()), |_| true, Some(s.stroke_color.clone()), Some(s.stroke_color.clone())),
            ),
            background_color: show(
                background_shown,
                self.form(|e| Some(e.background_color.clone()), |_| true, Some(s.background_color.clone()), Some(s.background_color.clone())),
            ),
            fill_style: show(fill_shown, self.form(|e| Some(e.fill_style.clone()), |_| true, None, Some(s.fill_style.clone()))),
            stroke_width: show(
                has_stroke_width(tool) || any(&has_stroke_width),
                self.form(|e| Some(e.stroke_width), |_| true, None, Some(s.stroke_width)),
            ),
            stroke_style: show(
                has_stroke_style(tool) || any(&has_stroke_style),
                self.form(|e| Some(e.stroke_style.clone()), |_| true, None, Some(s.stroke_style.clone())),
            ),
            roughness: show(
                has_stroke_style(tool) || any(&has_stroke_style),
                self.form(|e| Some(e.roughness), |_| true, None, Some(s.roughness)),
            ),
            round: show(
                can_change_roundness(tool) || any(&can_change_roundness),
                self.form(
                    |e| (!has_legacy).then_some(e.roundness.is_some()),
                    |e| e.kind != Kind::Arrow,
                    None,
                    Some(s.round),
                ),
            ),
            arrow_round: show(
                arrow_shown,
                self.form(
                    |e| (e.kind == Kind::Arrow).then_some(e.roundness.is_some()),
                    |e| e.kind == Kind::Arrow,
                    None,
                    Some(s.arrow_round),
                ),
            ),
            arrowheads: arrow_shown.then(|| {
                let end = |start: bool| {
                    let current = if start { &s.start_arrowhead } else { &s.end_arrowhead };
                    self.form(
                        |e| match (&e.linear, e.kind) {
                            (Some(l), Kind::Arrow) => Some(if start { l.start_arrowhead.clone() } else { l.end_arrowhead.clone() }),
                            _ => Some(current.clone()),
                        },
                        |_| true,
                        Some(current.clone()),
                        Some(current.clone()),
                    )
                    .flatten()
                };
                (end(true), end(false))
            }),
            font_family: show(
                text_shown,
                self.form(|e| self.text_value(e, |t| t.font_family), |e| self.has_text(e), None, Some(self.font_family)),
            ),
            font_size: show(
                text_shown,
                self.form(|e| self.text_value(e, |t| t.font_size), |e| self.has_text(e), None, Some(self.font_size)),
            ),
            text_align: show(
                text_shown && align_shown,
                self.form(|e| self.text_value(e, |t| t.text_align.clone()), |e| self.has_text(e), None, Some(self.text_align.clone())),
            ),
            vertical_align: show(
                targets.iter().any(bound_to_shape),
                self.form(
                    |e| match &e.text {
                        Some(t) if t.container_id.is_some() => Some(t.vertical_align.clone()),
                        Some(_) => None,
                        None => self.label_of(e).and_then(|l| l.text.map(|t| t.vertical_align)),
                    },
                    |e| self.has_text(e),
                    None,
                    Some("middle".into()),
                ),
            ),
            opacity: Some(self.form(|e| Some(e.opacity), |_| true, Some(s.opacity), Some(s.opacity)).unwrap_or(s.opacity)),
            actions: self.editing.is_none() && !self.selection.is_empty(),
        })
    }

    /// The elements a change applies to (`changeProperty`): the selection,
    /// with its labels if `labels`, and the text being typed.
    fn change_targets(&self, labels: bool) -> Vec<String> {
        let mut ids = self.selected_in_order();
        if labels {
            ids = self.with_labels(ids);
        }
        if let Some(edit) = &self.editing {
            if !ids.contains(&edit.id) {
                ids.push(edit.id.clone());
            }
        }
        ids
    }

    /// Applies a change from the panel.
    pub fn set_prop(&mut self, prop: Prop) {
        let s = &mut self.style;
        match &prop {
            Prop::StrokeColor(c) => s.stroke_color = c.clone(),
            Prop::BackgroundColor(c) => s.background_color = c.clone(),
            Prop::FillStyle(f) => s.fill_style = f.clone(),
            Prop::StrokeWidth(w) => s.stroke_width = *w,
            Prop::StrokeStyle(v) => s.stroke_style = v.clone(),
            Prop::Roughness(r) => s.roughness = *r,
            Prop::Round(r) => s.round = *r,
            Prop::ArrowRound(r) => s.arrow_round = *r,
            Prop::StartArrowhead(h) => s.start_arrowhead = h.clone(),
            Prop::EndArrowhead(h) => s.end_arrowhead = h.clone(),
            Prop::FontFamily(f) => self.font_family = *f,
            Prop::FontSize(n) => self.font_size = *n,
            Prop::TextAlign(a) => self.text_align = a.clone(),
            Prop::VerticalAlign(_) | Prop::Opacity(_) => {}
        }
        if let Prop::Opacity(o) = &prop {
            self.style.opacity = *o;
        }
        let labels = matches!(
            prop,
            Prop::StrokeColor(_)
                | Prop::Opacity(_)
                | Prop::FontFamily(_)
                | Prop::FontSize(_)
                | Prop::TextAlign(_)
                | Prop::VerticalAlign(_)
        );
        let ids = self.change_targets(labels);
        if ids.is_empty() {
            return;
        }
        self.edit(|ed| {
            for id in &ids {
                ed.apply(id, &prop);
            }
        });
    }

    /// Runs `f` as one undo step, or as part of the typing in progress.
    fn edit(&mut self, f: impl FnOnce(&mut Self)) {
        let typing = self.editing.is_some();
        if !typing {
            self.begin();
        }
        f(self);
        if !typing {
            self.commit();
        }
    }

    fn apply(&mut self, id: &str, prop: &Prop) {
        let Some(e) = self.scene.by_id(id).cloned() else {
            return;
        };
        let kind = type_name(&e);
        let set = |k: &str, v: Value| vec![update(k, v)];
        let updates: Updates = match prop {
            Prop::StrokeColor(c) if has_stroke_color(kind) => set("strokeColor", json!(c)),
            Prop::BackgroundColor(c) => set("backgroundColor", json!(c)),
            Prop::FillStyle(f) => set("fillStyle", json!(f)),
            Prop::StrokeWidth(w) => vec![update_num("strokeWidth", *w)],
            Prop::StrokeStyle(v) => set("strokeStyle", json!(v)),
            // A new seed, so the new sloppiness draws afresh.
            Prop::Roughness(r) => vec![update("seed", random_integer()), update_num("roughness", *r)],
            Prop::Round(r) if !e.linear.as_ref().is_some_and(|l| l.elbowed) => {
                let kind = if e.kind.uses_adaptive_radius() { 3 } else { 2 };
                set("roundness", if *r { json!({ "type": kind }) } else { Value::Null })
            }
            Prop::ArrowRound(r) if e.kind == Kind::Arrow && !e.linear.as_ref().is_some_and(|l| l.elbowed) => {
                set("roundness", if *r { json!({ "type": 2 }) } else { Value::Null })
            }
            Prop::StartArrowhead(h) if e.kind.is_linear() => set("startArrowhead", json!(h)),
            Prop::EndArrowhead(h) if e.kind.is_linear() => set("endArrowhead", json!(h)),
            Prop::Opacity(o) => vec![update_num("opacity", *o)],
            Prop::FontFamily(f) if e.text.is_some() => {
                vec![update_num("fontFamily", *f), update_num("lineHeight", scene::default_line_height(*f))]
            }
            Prop::FontSize(n) if e.text.is_some() => vec![update_num("fontSize", *n)],
            Prop::TextAlign(a) if e.text.is_some() => set("textAlign", json!(a)),
            Prop::VerticalAlign(a) if e.text.is_some() => set("verticalAlign", json!(a)),
            _ => return,
        };
        self.change(id, updates);
        let text_changed = matches!(
            prop,
            Prop::FontFamily(_) | Prop::FontSize(_) | Prop::TextAlign(_) | Prop::VerticalAlign(_)
        );
        if text_changed {
            self.redraw_text(&e, matches!(prop, Prop::FontSize(_)));
        }
    }

    /// `redrawTextBoundingBox` after a font or alignment change: a label is
    /// laid out in its shape again; a text on its own is measured again, and
    /// after a size change kept centred where it was
    /// (`offsetElementAfterFontResize`).
    fn redraw_text(&mut self, before: &Element, resized: bool) {
        let Some(t) = &before.text else { return };
        if let Some(container) = &t.container_id {
            let container = container.clone();
            self.sync_label(&container, true);
            let moving = std::collections::HashSet::from([container.clone()]);
            self.update_bound_arrows(&container, &moving);
            return;
        }
        let Some(e) = self.scene.by_id(&before.id).cloned() else { return };
        let font = Self::font_of(&e);
        let raw = e.raw();
        let original = raw.get("originalText").and_then(Value::as_str).unwrap_or(&t.text).to_owned();
        let auto = raw.get("autoResize").and_then(Value::as_bool).unwrap_or(true);
        let wrapped = if auto { original } else { text::wrap(&original, font, e.width, self.measure.as_ref()) };
        let (w, h) = text::measure(&wrapped, font, self.measure.as_ref());
        let width = if auto { w } else { e.width };
        let mut updates = vec![update("text", wrapped), update_num("width", width), update_num("height", h)];
        if resized && auto {
            let align = before.text.as_ref().map_or("left", |t| t.text_align.as_str());
            let x = match align {
                "left" => before.x,
                "center" => before.x + (before.width - width) / 2.0,
                _ => before.x + (before.width - width),
            };
            updates.push(update_num("x", x));
            updates.push(update_num("y", before.y + (before.height - h) / 2.0));
        }
        self.change(&e.id, updates);
        let moving = std::collections::HashSet::from([e.id.clone()]);
        self.update_bound_arrows(&e.id, &moving);
    }

    /// Ctrl+Shift+> and <: every selected text a tenth larger or smaller.
    pub fn step_font_size(&mut self, larger: bool) {
        let ids: Vec<String> = self
            .change_targets(true)
            .into_iter()
            .filter(|id| self.scene.by_id(id).is_some_and(|e| e.text.is_some()))
            .collect();
        if ids.is_empty() {
            return;
        }
        let mut sizes = Vec::new();
        self.edit(|ed| {
            for id in &ids {
                let Some(size) = ed.scene.by_id(id).and_then(|e| e.text.as_ref()).map(|t| t.font_size) else {
                    continue;
                };
                let next = if larger {
                    (size * (1.0 + FONT_SIZE_STEP)).round()
                } else {
                    (size / (1.0 + FONT_SIZE_STEP)).round()
                }
                .max(1.0);
                sizes.push(next);
                ed.apply(id, &Prop::FontSize(next));
            }
        });
        sizes.dedup();
        if let [one] = sizes.as_slice() {
            self.font_size = *one;
        }
    }

    /// Ctrl+Alt+C: the first selected element's style (and its label's).
    pub fn copy_styles(&mut self) -> bool {
        let Some(first) = self.selected().first().map(|e| (*e).clone()) else {
            return false;
        };
        let label = self.label_of(&first);
        self.copied_styles = Some((first, label));
        true
    }

    /// Ctrl+Alt+V: that style on the selection (`actionPasteStyles`).
    pub fn paste_styles(&mut self) {
        let Some((from, label)) = self.copied_styles.clone() else {
            return;
        };
        let ids = self.with_labels(self.selected_in_order());
        if ids.is_empty() {
            return;
        }
        self.edit(|ed| {
            for id in &ids {
                let Some(e) = ed.scene.by_id(id).cloned() else { continue };
                let bound = e.text.as_ref().is_some_and(|t| t.container_id.is_some());
                let Some(src) = (if bound { label.as_ref() } else { Some(&from) }) else {
                    continue;
                };
                let roundness = match src.roundness {
                    None => Value::Null,
                    // `canApplyRoundnessTypeToElement`, else the default.
                    Some(r) => {
                        let adaptive = e.kind.uses_adaptive_radius();
                        let can = if r.kind == 3.0 { adaptive } else { can_change_roundness(type_name(&e)) || e.kind == Kind::Arrow };
                        if can {
                            json!({ "type": r.kind })
                        } else if adaptive {
                            json!({ "type": 3 })
                        } else if matches!(e.kind, Kind::Diamond | Kind::Line | Kind::Arrow) {
                            json!({ "type": 2 })
                        } else {
                            Value::Null
                        }
                    }
                };
                let mut updates = vec![
                    update("backgroundColor", json!(src.background_color)),
                    update_num("strokeWidth", src.stroke_width),
                    update("strokeColor", json!(src.stroke_color)),
                    update("strokeStyle", json!(src.stroke_style)),
                    update("fillStyle", json!(src.fill_style)),
                    update_num("opacity", src.opacity),
                    update_num("roughness", src.roughness),
                    update("roundness", roundness),
                ];
                if e.text.is_some() {
                    let st = src.text.as_ref();
                    let family = st.map_or(5.0, |t| t.font_family);
                    updates.push(update_num("fontSize", st.map_or(20.0, |t| t.font_size)));
                    updates.push(update_num("fontFamily", family));
                    updates.push(update("textAlign", json!(st.map_or("left", |t| t.text_align.as_str()))));
                    updates.push(update_num(
                        "lineHeight",
                        st.map_or_else(|| scene::default_line_height(family), |t| t.line_height),
                    ));
                }
                if e.kind == Kind::Arrow && src.kind == Kind::Arrow {
                    if let Some(l) = &src.linear {
                        updates.push(update("startArrowhead", json!(l.start_arrowhead)));
                        updates.push(update("endArrowhead", json!(l.end_arrowhead)));
                    }
                }
                if matches!(e.kind, Kind::Frame | Kind::MagicFrame) {
                    updates.push(update("roundness", Value::Null));
                    updates.push(update("backgroundColor", json!("transparent")));
                }
                ed.change(id, updates);
                if e.text.is_some() {
                    ed.redraw_text(&e, false);
                }
            }
        });
    }
}
