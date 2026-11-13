//! Which key a shortcut means, across keyboard layouts: the Tauri app's
//! `shortcutKey` (src/lib/shortcuts.ts).
//!
//! What the layout types is right for letters: on AZERTY the key labelled
//! Z is reported at W's position, and going by position would turn Ctrl+Z
//! into Close Tab. A layout that types no Latin letters (Cyrillic, Greek…)
//! leaves nothing to match, and there the key's position is the answer.
//! Digits always go by position: AZERTY types `&` for Ctrl+1.

use iced::keyboard::key::{Key, Physical};

/// The key as shortcuts match it: a lower-case printable character, or
/// the named key's name ("Tab", "PageDown").
pub fn shortcut_key(key: &Key, physical: &Physical) -> String {
    let code = match physical {
        Physical::Code(c) => format!("{c:?}"),
        Physical::Unidentified(_) => String::new(),
    };
    if let Some(d) = code.strip_prefix("Digit").filter(|d| d.len() == 1) {
        return d.to_owned();
    }
    if let Key::Character(c) = key {
        let mut chars = c.chars();
        if let (Some(ch), None) = (chars.next(), chars.next()) {
            if (' '..='~').contains(&ch) {
                return ch.to_ascii_lowercase().to_string();
            }
        }
    }
    if let Some(l) = code.strip_prefix("Key").filter(|l| l.len() == 1) {
        return l.to_ascii_lowercase();
    }
    if code == "Comma" {
        return ",".into();
    }
    match key {
        Key::Named(n) => format!("{n:?}"),
        Key::Character(c) => c.to_lowercase(),
        Key::Unidentified => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::keyboard::key::{Code, Named, NativeCode};

    fn ch(c: &str) -> Key {
        Key::Character(c.into())
    }

    #[test]
    fn letters_follow_the_layout_and_fall_back_to_the_position() {
        // AZERTY: the key at W's place types z, and means z.
        assert_eq!(shortcut_key(&ch("z"), &Physical::Code(Code::KeyW)), "z");
        // Russian: the key at Z's place types я, and means z.
        assert_eq!(shortcut_key(&ch("я"), &Physical::Code(Code::KeyZ)), "z");
        // Greek, with Shift.
        assert_eq!(shortcut_key(&ch("Σ"), &Physical::Code(Code::KeyS)), "s");
        // Digits by position: AZERTY's & is 1.
        assert_eq!(shortcut_key(&ch("&"), &Physical::Code(Code::Digit1)), "1");
        assert_eq!(shortcut_key(&ch("б"), &Physical::Code(Code::Comma)), ",");
        assert_eq!(shortcut_key(&Key::Named(Named::PageDown), &Physical::Code(Code::PageDown)), "PageDown");
        assert_eq!(shortcut_key(&ch("Q"), &Physical::Unidentified(NativeCode::Unidentified)), "q");
    }
}
