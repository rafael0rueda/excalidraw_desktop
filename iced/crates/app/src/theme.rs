//! Themes: the Tauri app's, read and written the same way (src/theme/*.ts).
//!
//! A theme is ten colours and a name, hand-authorable JSON in the themes
//! directory. It owns the canvas colour (written into saved drawings as
//! `viewBackgroundColor`, as the Tauri app does), the colours new elements
//! are drawn in, the selection colour, and the window around the drawing.

use excalidraw_desktop_core::settings::{self, Settings};
use iced::Color;
use serde_json::Value;

/// Follow the desktop's light or dark preference instead of one theme.
pub const SYSTEM: &str = "system";

/// Used when settings name a theme that no longer exists.
pub const FALLBACK: &str = "light";

/// Excalidraw's own defaults for new elements. A drawing default still at
/// one of these is the theme's to change.
pub const FACTORY_STROKE: &str = "#1e1e1e";
pub const FACTORY_FILL: &str = "transparent";

/// The one non-colour value a field may hold, and only `fill` may hold it.
pub const TRANSPARENT: &str = "transparent";

/// The id rule: core's `safe_id`, since a theme's id names its file.
pub const MAX_ID_LENGTH: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Colors {
    /// The drawing surface behind the elements.
    pub canvas: String,
    /// Tool bars, panels, dialogs.
    pub surface: String,
    /// Hovers, borders, secondary panels.
    pub surface_alt: String,
    pub text: String,
    pub text_muted: String,
    pub accent: String,
    /// Text and icons drawn on top of `accent`.
    pub accent_text: String,
    pub danger: String,
    /// Stroke colour given to newly drawn elements.
    pub stroke: String,
    /// Fill given to newly drawn elements; usually "transparent".
    pub fill: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Theme {
    pub id: String,
    pub name: String,
    /// A dark colour scheme: which way hovers move, which slot it fills when
    /// following the desktop. The drawing is never inverted.
    pub dark: bool,
    pub colors: Colors,
}

/// The ten colour fields, in the order the schema documents them: their
/// JSON key, label, and what they colour.
pub const FIELDS: [(&str, &str, &str); 10] = [
    ("canvas", "Canvas", "Behind the drawing"),
    ("surface", "Surface", "Toolbars and panels"),
    ("surfaceAlt", "Surface alt", "Hovers and borders"),
    ("text", "Text", "Labels and icons"),
    ("textMuted", "Text muted", "Secondary labels"),
    ("accent", "Accent", "Selection and active tool"),
    ("accentText", "On accent", "Drawn on top of accent"),
    ("danger", "Danger", "Destructive actions"),
    ("stroke", "Stroke", "New elements draw in this"),
    ("fill", "Fill", "New elements fill with this"),
];

impl Colors {
    pub fn get(&self, i: usize) -> &str {
        [
            &self.canvas,
            &self.surface,
            &self.surface_alt,
            &self.text,
            &self.text_muted,
            &self.accent,
            &self.accent_text,
            &self.danger,
            &self.stroke,
            &self.fill,
        ][i]
    }

    pub fn get_mut(&mut self, i: usize) -> &mut String {
        match i {
            0 => &mut self.canvas,
            1 => &mut self.surface,
            2 => &mut self.surface_alt,
            3 => &mut self.text,
            4 => &mut self.text_muted,
            5 => &mut self.accent,
            6 => &mut self.accent_text,
            7 => &mut self.danger,
            8 => &mut self.stroke,
            _ => &mut self.fill,
        }
    }
}

// ------------------------------------------------------------------ colours

/// `#rgb` or `#rrggbb`, `#` required, nothing around it: the one grammar
/// for a theme colour (review 2026-11-04, FE-4).
pub fn parse_hex(s: &str) -> Option<[u8; 3]> {
    let body = s.strip_prefix('#')?;
    if !body.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let digit = |c: u8| (c as char).to_digit(16).map(|d| d as u8);
    let b = body.as_bytes();
    match b.len() {
        3 => Some([digit(b[0])? * 17, digit(b[1])? * 17, digit(b[2])? * 17]),
        6 => Some([digit(b[0])? * 16 + digit(b[1])?, digit(b[2])? * 16 + digit(b[3])?, digit(b[4])? * 16 + digit(b[5])?]),
        _ => None,
    }
}

fn format_hex(rgb: [f64; 3]) -> String {
    // JS `Math.round` after a clamp, as color.ts does.
    let c = |v: f64| (v.clamp(0.0, 255.0) + 0.5).floor() as u8;
    format!("#{:02x}{:02x}{:02x}", c(rgb[0]), c(rgb[1]), c(rgb[2]))
}

/// Whether `value` may stand in field `i`.
pub fn is_color_value(i: usize, value: &str) -> bool {
    parse_hex(value).is_some() || (FIELDS[i].0 == "fill" && value == TRANSPARENT)
}

/// Blends `a` toward `b`; a value that is not a colour passes through.
pub fn mix(a: &str, b: &str, t: f64) -> String {
    match (parse_hex(a), parse_hex(b)) {
        (Some(x), Some(y)) => format_hex([0, 1, 2].map(|i| f64::from(x[i]) + (f64::from(y[i]) - f64::from(x[i])) * t)),
        _ => a.to_owned(),
    }
}

/// Moves a colour away from its own background: lighter on dark themes,
/// darker on light ones.
pub fn emphasis(hex: &str, dark: bool, t: f64) -> String {
    mix(hex, if dark { "#ffffff" } else { "#000000" }, t)
}

/// A theme colour for iced; anything unparseable (`transparent`) is clear.
pub fn color(s: &str) -> Color {
    parse_hex(s).map_or(Color::TRANSPARENT, |[r, g, b]| Color::from_rgb8(r, g, b))
}

// ------------------------------------------------------------ files

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= MAX_ID_LENGTH && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// A theme read from disk, or what is wrong with it: user themes are
/// hand-written, so a missing key is said and the file skipped.
pub fn parse(value: &Value) -> Result<Theme, String> {
    let Some(raw) = value.as_object() else { return Err("not a JSON object".into()) };
    let id = match raw.get("id").and_then(Value::as_str) {
        Some(id) if !id.is_empty() => id,
        _ => return Err("missing \"id\"".into()),
    };
    if !valid_id(id) {
        return Err(format!("{id}: \"id\" may only use lower-case letters, digits and dashes"));
    }
    let name = match raw.get("name").and_then(Value::as_str) {
        Some(n) if !n.trim().is_empty() => n,
        _ => return Err(format!("{id}: missing \"name\"")),
    };
    let Some(colors) = raw.get("colors").and_then(Value::as_object) else { return Err(format!("{id}: missing \"colors\"")) };
    let mut out = PRESETS[0].colors.clone();
    for (i, (key, _, _)) in FIELDS.iter().enumerate() {
        let value = match colors.get(*key).and_then(Value::as_str) {
            Some(v) if !v.is_empty() => v,
            _ => return Err(format!("{id}: missing \"colors.{key}\"")),
        };
        if !is_color_value(i, value) {
            let also = if *key == "fill" { format!(", or \"{TRANSPARENT}\"") } else { String::new() };
            return Err(format!("{id}: \"colors.{key}\" is not a colour — use #rgb or #rrggbb{also}"));
        }
        *out.get_mut(i) = value.to_owned();
    }
    Ok(Theme { id: id.to_owned(), name: name.to_owned(), dark: raw.get("dark") == Some(&Value::Bool(true)), colors: out })
}

/// A theme as written to disk: schema order, two-space indent, trailing
/// newline, as `JSON.stringify(theme, null, 2)` writes it in types.ts.
pub fn serialize(theme: &Theme) -> String {
    let s = |v: &str| serde_json::to_string(v).unwrap_or_default();
    let mut out = format!("{{\n  \"id\": {},\n  \"name\": {},\n  \"dark\": {},\n  \"colors\": {{\n", s(&theme.id), s(&theme.name), theme.dark);
    for (i, (key, _, _)) in FIELDS.iter().enumerate() {
        let comma = if i + 1 < FIELDS.len() { "," } else { "" };
        out.push_str(&format!("    \"{key}\": {}{comma}\n", s(theme.colors.get(i))));
    }
    out.push_str("  }\n}\n");
    out
}

/// A display name as an id core will accept.
pub fn slugify(name: &str) -> String {
    let mut out = String::new();
    for c in name.to_lowercase().chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_matches('-');
    out.chars().take(MAX_ID_LENGTH).collect()
}

/// `base`, or `base-2`, `base-3`… until it collides with nothing.
pub fn unique_id(base: &str, taken: &[String]) -> String {
    let root = if base.is_empty() { "custom" } else { base };
    if !taken.iter().any(|t| t == root) {
        return root.to_owned();
    }
    (2..).map(|n| format!("{root}-{n}")).find(|c| !taken.iter().any(|t| t == c)).unwrap_or_default()
}

/// The draft, safe to paint with: a half-typed colour takes `fallback`'s.
pub fn paintable(draft: &Theme, fallback: &Theme) -> Theme {
    let mut out = draft.clone();
    for i in 0..FIELDS.len() {
        if !is_color_value(i, out.colors.get(i)) {
            *out.colors.get_mut(i) = fallback.colors.get(i).to_owned();
        }
    }
    out
}

/// Fields left in a state that cannot be saved, by label.
pub fn invalid_fields(theme: &Theme) -> Vec<&'static str> {
    (0..FIELDS.len()).filter(|&i| !is_color_value(i, theme.colors.get(i))).map(|i| FIELDS[i].1).collect()
}

// ------------------------------------------------------------ the themes

/// Settings, user themes and the desktop's preference: which theme is on.
pub struct Themes {
    pub settings: Settings,
    pub user: Vec<Theme>,
    /// The desktop prefers dark.
    pub system_dark: bool,
    /// Writes settings and themes: off in the development modes.
    persist: bool,
}

impl Themes {
    /// From disk. Broken theme files are skipped quietly here; Reload says
    /// what was wrong with them, as the Tauri app does.
    pub fn load(system_dark: bool) -> Self {
        Self { settings: settings::load_settings(), user: read_user_themes().0, system_dark, persist: true }
    }

    /// One theme, nothing read or written: the development modes.
    pub fn fixed(id: &str) -> Self {
        Self { settings: Settings { theme: id.to_owned(), ..Settings::default() }, user: Vec::new(), system_dark: false, persist: false }
    }

    /// Presets, then user themes; a user theme with a preset's id takes its
    /// place.
    pub fn all(&self) -> Vec<Theme> {
        let mut out: Vec<Theme> = PRESETS.to_vec();
        for t in &self.user {
            match out.iter_mut().find(|p| p.id == t.id) {
                Some(slot) => *slot = t.clone(),
                None => out.push(t.clone()),
            }
        }
        out
    }

    pub fn find(&self, id: &str) -> Option<Theme> {
        self.all().into_iter().find(|t| t.id == id)
    }

    /// A theme by id, else the fallback.
    pub fn pick(&self, id: &str) -> Theme {
        self.find(id).or_else(|| self.find(FALLBACK)).unwrap_or_else(|| PRESETS[0].clone())
    }

    /// What the settings resolve to.
    pub fn chosen(&self) -> Theme {
        let s = &self.settings;
        if s.theme != SYSTEM {
            return self.pick(&s.theme);
        }
        self.pick(if self.system_dark { &s.dark_theme } else { &s.light_theme })
    }

    pub fn is_user(&self, id: &str) -> bool {
        self.user.iter().any(|t| t.id == id)
    }

    fn save_settings(&self) {
        if self.persist {
            let _ = settings::save_settings(self.settings.clone());
        }
    }

    pub fn select(&mut self, id: &str) {
        if self.settings.theme != id {
            self.settings.theme = id.to_owned();
            self.save_settings();
        }
    }

    pub fn set_pair(&mut self, light: &str, dark: &str) {
        if self.settings.light_theme != light || self.settings.dark_theme != dark {
            self.settings.light_theme = light.to_owned();
            self.settings.dark_theme = dark.to_owned();
            self.save_settings();
        }
    }

    /// Writes a theme to its file and puts it on, in whichever slot applies:
    /// editing a dark theme while following the desktop makes it the dark
    /// one, and the other way round.
    pub fn save(&mut self, theme: Theme) -> Result<String, String> {
        let path = if self.persist { settings::save_user_theme(theme.id.clone(), serialize(&theme))? } else { String::new() };
        self.user.retain(|t| t.id != theme.id);
        self.user.push(theme.clone());
        if self.settings.theme == SYSTEM {
            let (light, dark) = (self.settings.light_theme.clone(), self.settings.dark_theme.clone());
            if theme.dark {
                self.set_pair(&light, &theme.id);
            } else {
                self.set_pair(&theme.id, &dark);
            }
        } else {
            self.select(&theme.id);
        }
        Ok(path)
    }

    /// Removes a user theme; a preset of the same id comes back.
    pub fn delete(&mut self, id: &str) -> Result<(), String> {
        if self.persist {
            settings::delete_user_theme(id.to_owned())?;
        }
        self.user.retain(|t| t.id != id);
        Ok(())
    }

    /// Reads the themes directory again: how many themes, and what was
    /// skipped and why.
    pub fn reload(&mut self) -> (usize, Vec<String>) {
        let (themes, errors) = read_user_themes();
        self.user = themes;
        (self.user.len(), errors)
    }
}

/// The user themes, and the files skipped. A theme must be in `<id>.json`:
/// saving and deleting go by that name.
fn read_user_themes() -> (Vec<Theme>, Vec<String>) {
    let mut themes = Vec::new();
    let mut errors = Vec::new();
    for file in settings::list_user_themes() {
        if let Some(e) = file.error {
            errors.push(e);
            continue;
        }
        match parse(file.value.as_ref().unwrap_or(&Value::Null)) {
            Err(e) => errors.push(e),
            Ok(t) if t.id != file.name => {
                errors.push(format!("{}.json: its \"id\" is \"{}\", so it has to be named {}.json", file.name, t.id, t.id));
            }
            Ok(t) => themes.push(t),
        }
    }
    (themes, errors)
}

// ------------------------------------------------------------ the presets

macro_rules! preset {
    ($id:literal, $name:literal, $dark:literal, [$($c:literal),*]) => {{
        let [canvas, surface, surface_alt, text, text_muted, accent, accent_text, danger, stroke, fill] = [$($c),*];
        Theme {
            id: String::new() + $id,
            name: String::new() + $name,
            dark: $dark,
            colors: Colors {
                canvas: canvas.into(),
                surface: surface.into(),
                surface_alt: surface_alt.into(),
                text: text.into(),
                text_muted: text_muted.into(),
                accent: accent.into(),
                accent_text: accent_text.into(),
                danger: danger.into(),
                stroke: stroke.into(),
                fill: fill.into(),
            },
        }
    }};
}

/// The built-in themes, as src/theme/presets.ts has them (a test holds the
/// two to each other). Colours in field order: canvas, surface, surface
/// alt, text, text muted, accent, on accent, danger, stroke, fill.
pub static PRESETS: std::sync::LazyLock<Vec<Theme>> = std::sync::LazyLock::new(|| {
    vec![
        preset!("light", "Light", false, ["#ffffff", "#ffffff", "#f1f0ff", "#1b1b1f", "#8a8a90", "#6965db", "#ffffff", "#db6965", "#1e1e1e", "transparent"]),
        preset!("dark", "Dark", true, ["#121212", "#1e1e1e", "#2e2e2e", "#e3e3e3", "#8a8a8a", "#a8a5ff", "#121212", "#ff8a85", "#e3e3e3", "transparent"]),
        preset!("nord", "Nord", true, ["#2e3440", "#3b4252", "#4c566a", "#eceff4", "#7b88a1", "#88c0d0", "#2e3440", "#bf616a", "#d8dee9", "transparent"]),
        preset!("dracula", "Dracula", true, ["#282a36", "#343746", "#44475a", "#f8f8f2", "#6272a4", "#bd93f9", "#282a36", "#ff5555", "#f8f8f2", "transparent"]),
        preset!("gruvbox-dark", "Gruvbox Dark", true, ["#282828", "#3c3836", "#504945", "#ebdbb2", "#928374", "#fabd2f", "#282828", "#fb4934", "#ebdbb2", "transparent"]),
        preset!("solarized-light", "Solarized Light", false, ["#fdf6e3", "#eee8d5", "#e0dac0", "#586e75", "#93a1a1", "#268bd2", "#fdf6e3", "#dc322f", "#586e75", "transparent"]),
        preset!("solarized-dark", "Solarized Dark", true, ["#002b36", "#073642", "#10505f", "#93a1a1", "#586e75", "#268bd2", "#002b36", "#dc322f", "#93a1a1", "transparent"]),
        preset!("catppuccin-mocha", "Catppuccin Mocha", true, ["#1e1e2e", "#313244", "#45475a", "#cdd6f4", "#6c7086", "#cba6f7", "#1e1e2e", "#f38ba8", "#cdd6f4", "transparent"]),
        // sumiInk3 / sumiInk4 / sumiInk5, fujiWhite, fujiGray, crystalBlue, samuraiRed
        preset!("kanagawa-wave", "Kanagawa Wave", true, ["#1F1F28", "#2A2A37", "#363646", "#DCD7BA", "#727169", "#7E9CD8", "#1F1F28", "#E82424", "#DCD7BA", "transparent"]),
        // lotusWhite3 / lotusWhite4 / lotusWhite5, lotusInk1, lotusGray2, lotusBlue4, lotusRed3
        preset!("kanagawa-lotus", "Kanagawa Lotus", false, ["#f2ecbc", "#e7dba0", "#e4d794", "#545464", "#716e61", "#4d699b", "#f2ecbc", "#e82424", "#545464", "transparent"]),
    ]
});

// ------------------------------------------------------------ for iced

/// A theme's colours as the window draws with them: the ten, and the
/// shades derived from them the way variables.ts derives Excalidraw's.
#[derive(Debug, Clone, Copy)]
pub struct Look {
    pub dark: bool,
    pub canvas: Color,
    pub surface: Color,
    pub surface_alt: Color,
    pub text: Color,
    pub muted: Color,
    pub accent: Color,
    pub accent_text: Color,
    pub danger: Color,
    /// The accent over a panel: the active tool, a picked menu entry.
    pub accent_soft: Color,
    pub accent_hover: Color,
    /// Between surface and surface alt: a hovered entry.
    pub hover: Color,
}

impl Look {
    pub fn of(theme: &Theme) -> Self {
        let c = &theme.colors;
        Self {
            dark: theme.dark,
            canvas: color(&c.canvas),
            surface: color(&c.surface),
            surface_alt: color(&c.surface_alt),
            text: color(&c.text),
            muted: color(&c.text_muted),
            accent: color(&c.accent),
            accent_text: color(&c.accent_text),
            danger: color(&c.danger),
            accent_soft: color(&mix(&c.surface, &c.accent, 0.28)),
            accent_hover: color(&emphasis(&c.accent, theme.dark, 0.14)),
            hover: color(&mix(&c.surface, &c.surface_alt, 0.5)),
        }
    }

    /// The theme iced's own widgets take their colours from (and the window
    /// its light or dark title bar).
    pub fn iced(&self, name: &str) -> iced::Theme {
        iced::Theme::custom(
            name.to_owned(),
            iced::theme::Palette {
                background: self.surface,
                text: self.text,
                primary: self.accent,
                success: self.accent,
                warning: self.accent,
                danger: self.danger,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_match_the_tauri_app() {
        let ts = include_str!("../../../../src/theme/presets.ts");
        let field = |key: &str| -> Vec<String> {
            ts.lines()
                .filter_map(|l| l.trim().strip_prefix(&format!("{key}: ")))
                .map(|v| v.trim_end_matches(',').trim_matches('"').to_owned())
                .collect()
        };
        let ids = field("id");
        assert_eq!(ids.len(), PRESETS.len());
        for (i, p) in PRESETS.iter().enumerate() {
            assert_eq!(ids[i], p.id);
            assert_eq!(field("name")[i], p.name);
            assert_eq!(field("dark")[i], p.dark.to_string());
            for (k, (key, _, _)) in FIELDS.iter().enumerate() {
                assert_eq!(field(key)[i], p.colors.get(k), "{} {key}", p.id);
            }
        }
    }

    #[test]
    fn colours() {
        assert_eq!(parse_hex("#abc"), Some([0xaa, 0xbb, 0xcc]));
        assert_eq!(parse_hex("#1F1F28"), Some([0x1f, 0x1f, 0x28]));
        for bad in ["abc", " #abc", "#abcd", "#ggg", "#", "transparent"] {
            assert_eq!(parse_hex(bad), None, "{bad}");
        }
        assert!(is_color_value(9, "transparent"));
        assert!(!is_color_value(0, "transparent"));
        assert_eq!(mix("#000000", "#ffffff", 0.5), "#808080");
        assert_eq!(mix("transparent", "#ffffff", 0.5), "transparent");
        assert_eq!(emphasis("#6965db", false, 0.14), "#5a57bc");
    }

    #[test]
    fn files_round_trip() {
        let t = PRESETS[8].clone();
        let text = serialize(&t);
        assert!(text.starts_with("{\n  \"id\": \"kanagawa-wave\",\n  \"name\": \"Kanagawa Wave\",\n  \"dark\": true,\n  \"colors\": {\n    \"canvas\": \"#1F1F28\","));
        assert!(text.ends_with("    \"fill\": \"transparent\"\n  }\n}\n"));
        assert_eq!(parse(&serde_json::from_str(&text).unwrap()).unwrap(), t);
        let bad = |json: &str| parse(&serde_json::from_str(json).unwrap()).unwrap_err();
        assert_eq!(bad("[]"), "not a JSON object");
        assert_eq!(bad(r#"{"id":"Nope"}"#), "Nope: \"id\" may only use lower-case letters, digits and dashes");
        assert_eq!(bad(r#"{"id":"x","name":" "}"#), "x: missing \"name\"");
        let mut v: Value = serde_json::from_str(&text).unwrap();
        v["colors"]["canvas"] = " #fff".into();
        assert_eq!(parse(&v).unwrap_err(), "kanagawa-wave: \"colors.canvas\" is not a colour — use #rgb or #rrggbb");
    }

    #[test]
    fn ids() {
        assert_eq!(slugify("  My Theme! 2 "), "my-theme-2");
        assert_eq!(slugify("Ñandú"), "and");
        let taken = vec!["nord".to_owned(), "nord-2".to_owned()];
        assert_eq!(unique_id("nord", &taken), "nord-3");
        assert_eq!(unique_id("", &taken), "custom");
    }

    #[test]
    fn choosing() {
        let mut t = Themes::fixed(SYSTEM);
        assert_eq!(t.chosen().id, "kanagawa-lotus");
        t.system_dark = true;
        assert_eq!(t.chosen().id, "kanagawa-wave");
        t.select("gone");
        assert_eq!(t.chosen().id, FALLBACK);
        // A user theme with a preset's id takes its place in the list.
        t.user.push(Theme { name: "Mine".into(), ..PRESETS[2].clone() });
        let all = t.all();
        assert_eq!(all.len(), PRESETS.len());
        assert_eq!(all[2].name, "Mine");
        // Saving a dark theme while following the desktop makes it the dark one.
        t.select(SYSTEM);
        t.save(Theme { id: "night".into(), ..PRESETS[1].clone() }).unwrap();
        assert_eq!((t.settings.light_theme.as_str(), t.settings.dark_theme.as_str()), ("kanagawa-lotus", "night"));
        t.select("nord");
        t.save(Theme { id: "day".into(), ..PRESETS[0].clone() }).unwrap();
        assert_eq!(t.settings.theme, "day");
    }
}
