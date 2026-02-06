use shared_types::AppDefinition;

pub fn core_apps() -> Vec<AppDefinition> {
    vec![
        AppDefinition {
            id: "chat".to_string(),
            name: "Chat".to_string(),
            icon: "💬".to_string(),
            component_code: "ChatApp".to_string(),
            default_width: 600,
            default_height: 500,
        },
        AppDefinition {
            id: "writer".to_string(),
            name: "Writer".to_string(),
            icon: "📝".to_string(),
            component_code: "WriterApp".to_string(),
            default_width: 800,
            default_height: 600,
        },
        AppDefinition {
            id: "terminal".to_string(),
            name: "Terminal".to_string(),
            icon: "🖥️".to_string(),
            component_code: "TerminalApp".to_string(),
            default_width: 700,
            default_height: 450,
        },
        AppDefinition {
            id: "files".to_string(),
            name: "Files".to_string(),
            icon: "📁".to_string(),
            component_code: "FilesApp".to_string(),
            default_width: 700,
            default_height: 500,
        },
    ]
}

pub fn get_app_icon(app_id: &str) -> &'static str {
    match app_id {
        "chat" => "💬",
        "writer" => "📝",
        "terminal" => "🖥️",
        "files" => "📁",
        _ => "📱",
    }
}
