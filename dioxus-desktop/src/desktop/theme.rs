pub const DEFAULT_THEME: &str = "dark";
const THEME_KEY: &str = "theme-preference";

pub fn next_theme(current_theme: &str) -> String {
    if current_theme == "light" {
        "dark".to_string()
    } else {
        "light".to_string()
    }
}

pub fn apply_theme_to_document(theme: &str) {
    if !matches!(theme, "light" | "dark") {
        return;
    }

    if let Some(document) = web_sys::window().and_then(|w| w.document()) {
        if let Some(root) = document.document_element() {
            let _ = root.set_attribute("data-theme", theme);
        }
    }
}

pub fn get_cached_theme_preference() -> Option<String> {
    web_sys::window()
        .and_then(|window| window.local_storage().ok().flatten())
        .and_then(|storage| storage.get_item(THEME_KEY).ok().flatten())
        .filter(|theme| matches!(theme.as_str(), "light" | "dark"))
}

pub fn set_cached_theme_preference(theme: &str) {
    if !matches!(theme, "light" | "dark") {
        return;
    }

    if let Some(storage) =
        web_sys::window().and_then(|window| window.local_storage().ok().flatten())
    {
        let _ = storage.set_item(THEME_KEY, theme);
    }
}
