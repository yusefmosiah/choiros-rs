use shared_types::AppDefinition;

pub fn core_apps() -> Vec<AppDefinition> {
    vec![
        AppDefinition {
            id: "writer".to_string(),
            name: "Writer".to_string(),
            icon: "📝".to_string(),
            component_code: "WriterApp".to_string(),
            default_width: 1100,
            default_height: 720,
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
        AppDefinition {
            id: "logs".to_string(),
            name: "Logs".to_string(),
            icon: "📡".to_string(),
            component_code: "LogsApp".to_string(),
            default_width: 780,
            default_height: 520,
        },
        AppDefinition {
            id: "trace".to_string(),
            name: "Trace".to_string(),
            icon: "🔍".to_string(),
            component_code: "TraceApp".to_string(),
            default_width: 900,
            default_height: 600,
        },
        AppDefinition {
            id: "settings".to_string(),
            name: "Settings".to_string(),
            icon: "⚙️".to_string(),
            component_code: "SettingsApp".to_string(),
            default_width: 860,
            default_height: 560,
        },
    ]
}

pub fn get_app_icon(app_id: &str) -> &'static str {
    match app_id {
        "writer" => "📝",
        "terminal" => "🖥️",
        "files" => "📁",
        "logs" => "📡",
        "trace" => "🔍",
        "settings" => "⚙️",
        _ => "📱",
    }
}
