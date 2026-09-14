//! Native dialogs whose outcome the renderer cannot be trusted to finish.

use std::path::{Path, PathBuf};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind, MessageDialogResult};

/// What is being saved, which fixes the extension the file has to end in.
#[derive(serde::Deserialize, Clone, Copy, Debug)]
#[serde(rename_all = "lowercase")]
pub enum SaveKind {
    Drawing,
    Png,
    Svg,
}

impl SaveKind {
    fn extension(self) -> &'static str {
        match self {
            SaveKind::Drawing => "excalidraw",
            SaveKind::Png => "png",
            SaveKind::Svg => "svg",
        }
    }

    fn label(self) -> &'static str {
        match self {
            SaveKind::Drawing => "Excalidraw drawing",
            SaveKind::Png => "PNG image",
            SaveKind::Svg => "SVG image",
        }
    }

    fn title(self) -> &'static str {
        match self {
            SaveKind::Drawing => "Save drawing as",
            SaveKind::Png => "Export as PNG image",
            SaveKind::Svg => "Export as SVG image",
        }
    }
}

/// `path` ending in `.extension`, and whether that had to be added. A name
/// that already ends in it, in any case, is left alone.
fn with_extension(path: PathBuf, extension: &str) -> (PathBuf, bool) {
    let has = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case(extension));
    if has {
        return (path, false);
    }
    let mut name = path.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(".");
    name.push(extension);
    (path.with_file_name(name), true)
}

/// Asks where to save, and returns a path that already carries the extension.
///
/// The dialog's own "replace this file?" check only ever sees the name as
/// typed. Typing `plan` and getting `plan.excalidraw` used to overwrite an
/// existing `plan.excalidraw` without a word, so when the extension is added
/// here, the existing file is asked about here too. Declining goes back to the
/// dialog rather than cancelling the save.
#[tauri::command]
pub async fn pick_save_path(
    window: tauri::WebviewWindow,
    kind: SaveKind,
    suggested: String,
) -> Result<Option<String>, String> {
    let suggested = PathBuf::from(suggested);
    let mut directory = suggested
        .parent()
        .filter(|dir| dir.is_absolute())
        .map(Path::to_path_buf);
    let mut file_name = suggested
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| format!("Untitled.{}", kind.extension()));

    loop {
        let mut dialog = window
            .dialog()
            .file()
            .set_title(kind.title())
            .add_filter(kind.label(), &[kind.extension()])
            .set_file_name(&file_name)
            .set_parent(&window);
        if let Some(dir) = &directory {
            dialog = dialog.set_directory(dir);
        }
        let Some(picked) = dialog.blocking_save_file() else {
            return Ok(None);
        };
        let picked = picked.into_path().map_err(|e| e.to_string())?;
        let (path, added) = with_extension(picked, kind.extension());

        if added && path.exists() {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let replace = "Replace".to_string();
            let result = window
                .dialog()
                .message(format!("A file named \"{name}\" already exists. Replace it?"))
                .title("Replace file?")
                .kind(MessageDialogKind::Warning)
                .buttons(MessageDialogButtons::OkCancelCustom(replace.clone(), "Cancel".into()))
                .parent(&window)
                .blocking_show_with_result();
            let confirmed = matches!(result, MessageDialogResult::Ok | MessageDialogResult::Yes)
                || result == MessageDialogResult::Custom(replace);
            if !confirmed {
                directory = path.parent().map(Path::to_path_buf);
                file_name = name;
                continue;
            }
        }
        return Ok(Some(path.to_string_lossy().into_owned()));
    }
}

#[cfg(test)]
mod tests {
    use super::with_extension;
    use std::path::PathBuf;

    #[test]
    fn the_extension_is_added_only_when_missing() {
        let (path, added) = with_extension(PathBuf::from("/tmp/plan"), "excalidraw");
        assert_eq!(path, PathBuf::from("/tmp/plan.excalidraw"));
        assert!(added);

        let (path, added) = with_extension(PathBuf::from("/tmp/plan.EXCALIDRAW"), "excalidraw");
        assert_eq!(path, PathBuf::from("/tmp/plan.EXCALIDRAW"));
        assert!(!added, "a different case is still the same extension");

        let (path, added) = with_extension(PathBuf::from("/tmp/plan.v2"), "png");
        assert_eq!(path, PathBuf::from("/tmp/plan.v2.png"), "a dot in the name is not the extension");
        assert!(added);
    }
}
