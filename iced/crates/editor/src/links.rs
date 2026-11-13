//! Links on elements, as Excalidraw 0.18.1 keeps them: one per element
//! (a shape with a label carries it on the shape), cleaned as
//! `normalizeLink` does, shown as a small icon at the top right
//! (`getLinkHandleFromCoords`), followed from that icon when the element is
//! not selected (`isPointHittingLink`).
//!
//! Following a link is the app's: this only says which.

use crate::geometry::{local_box, rotate};
use crate::Editor;
use rough::Point;
use scene::edit::update;
use scene::Element;
use serde_json::{json, Value};

/// `DEFAULT_LINK_SIZE`, in screen pixels.
pub const LINK_SIZE: f64 = 14.0;

/// `normalizeLink`: trimmed, and `@braintree/sanitize-url`'s refusal of
/// script-carrying schemes.
pub fn normalize_link(link: &str) -> String {
    let link = link.trim();
    if link.is_empty() {
        return String::new();
    }
    let scheme: String = link
        .chars()
        .filter(|c| !c.is_whitespace() && !c.is_control())
        .take_while(|c| *c != ':')
        .collect::<String>()
        .to_ascii_lowercase();
    let has_scheme = link.contains(':');
    if has_scheme && matches!(scheme.as_str(), "javascript" | "data" | "vbscript") {
        return "about:blank".into();
    }
    link.replace('"', "%22")
}

/// The link icon's box: x, y, width, height, in the scene.
pub fn link_handle(e: &Element, zoom: f64) -> [f64; 4] {
    let b = local_box(e);
    let (x1, y1, x2, y2) = (e.x + b[0], e.y + b[1], e.x + b[2], e.y + b[3]);
    let size = LINK_SIZE / zoom;
    let centering = (LINK_SIZE - 8.0) / (2.0 * zoom);
    let margin = 4.0 / zoom;
    let x = x2 + margin - centering;
    let y = y1 - margin - size + centering;
    let c = rotate([x + size / 2.0, y + size / 2.0], [(x1 + x2) / 2.0, (y1 + y2) / 2.0], e.angle);
    [c[0] - size / 2.0, c[1] - size / 2.0, size, size]
}

fn link_of(e: &Element) -> Option<&str> {
    e.link.as_deref().filter(|l| !l.is_empty())
}

impl Editor {
    /// The link whose icon is under `p`: elements not selected, top first.
    pub fn link_at(&self, p: Point) -> Option<String> {
        let threshold = 4.0 / self.zoom;
        self.scene
            .visible()
            .filter(|e| !self.selection.contains(&e.id))
            .filter_map(|e| Some((e, link_of(e)?)))
            .filter(|(e, _)| {
                let [x, y, w, h] = link_handle(e, self.zoom);
                p[0] > x - threshold && p[0] < x + w + threshold && p[1] > y - threshold && p[1] < y + h + threshold
            })
            .last()
            .map(|(_, l)| l.to_owned())
    }

    /// Elements showing a link icon.
    pub fn linked(&self) -> impl Iterator<Item = &Element> {
        self.scene.visible().filter(|e| link_of(e).is_some())
    }

    /// What a link edit applies to: the one selected element (a shape with
    /// its label counts as one), and its link now.
    pub fn link_target(&self) -> Option<(String, String)> {
        let selected = self.selected();
        let ids: Vec<&Element> = selected
            .into_iter()
            .filter(|e| e.text.as_ref().is_none_or(|t| t.container_id.is_none()))
            .collect();
        let [one] = ids.as_slice() else { return None };
        Some((one.id.clone(), one.link.clone().unwrap_or_default()))
    }

    /// Ctrl+K's Save: the link set, or removed when empty. One undo step.
    pub fn set_link(&mut self, link: &str) {
        let Some((id, _)) = self.link_target() else { return };
        let link = normalize_link(link);
        self.begin();
        self.change(&id, vec![update("link", if link.is_empty() { Value::Null } else { json!(link) })]);
        self.commit();
    }

    /// The ids an element link (`…?element=ID` or `…?group=ID`) points at.
    pub fn link_targets(&self, link: &str) -> Vec<String> {
        let Some((_, query)) = link.split_once('?') else { return Vec::new() };
        let query = query.split('#').next().unwrap_or("");
        for pair in query.split('&') {
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            match k {
                "element" => return self.visible(v).map(|e| vec![e.id.clone()]).unwrap_or_default(),
                "group" => return self.scene.visible().filter(|e| e.group_ids.iter().any(|g| g == v)).map(|e| e.id.clone()).collect(),
                _ => {}
            }
        }
        Vec::new()
    }
}
