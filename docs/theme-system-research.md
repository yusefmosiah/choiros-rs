# Theme System Requirements Research for Dioxus Web Desktop

## Executive Summary

This document provides comprehensive research findings and recommendations for implementing a robust theme system for the ChoirOS web desktop built with Dioxus. The research covers modern CSS theming approaches, best practices from industry-leading UI libraries, accessibility considerations, and specific recommendations for Dioxus-based applications.

---

## ChoirOS Compatibility Notes (2026-02-05)

- Canonical theme preference should be stored in backend actor/EventStore state.
- Browser `localStorage` can be used only as a write-through/read-through cache to improve
  startup UX, never as authoritative storage.
- Conflicts between backend value and cached browser value must resolve in favor of backend.

---

## 1. Theme Architecture

### 1.1 Recommended Approach: CSS Custom Properties (Variables)

**CSS Custom Properties (CSS Variables)** is the recommended architecture for Dioxus web desktop theming.

**Why CSS Variables?**
- **Browser-native**: Built-in CSS feature with 95%+ browser support
- **Dynamic runtime updates**: Changes propagate instantly without page reloads
- **Cascade support**: Follows CSS cascade rules for natural inheritance
- **No build-step overhead**: No need for preprocessor compilation
- **SSR-friendly**: Prevents flash of incorrect theme (FOIT) on server-side rendering
- **Debuggable**: Variables appear as-is in browser dev tools

**Comparison of Approaches:**

| Approach | Pros | Cons | Recommendation |
|----------|------|-------|----------------|
| CSS Custom Properties | Native, dynamic, cascade-aware, SSR-safe | No type safety in CSS alone | **RECOMMENDED** |
| Preprocessor Variables (SASS/LESS) | Type-safe, familiar | Requires build step, not dynamic at runtime | Use for design tokens only |
| CSS-in-JS (Styled Components) | Component-scoped, type-safe | Larger bundle, no cascade, harder to debug | Avoid for global themes |
| Separate Stylesheets | Simple, no JS needed | Page reload required, duplicate CSS | For legacy support only |

### 1.2 Architecture Pattern for Dioxus

```
┌─────────────────────────────────────────────────────────────┐
│  Dioxus App Root                                    │
│  ┌─────────────────────────────────────────────────┐   │
│  │  ThemeProvider (CSS Variable Injection)      │   │
│  │  ┌───────────────────────────────────────┐ │   │
│  │  │  CSS Variables (Theme Tokens)         │ │   │
│  │  │  :root {                          │ │   │
│  │  │    --primary: #3b82f6;            │ │   │
│  │  │    --bg-primary: #0f172a;         │ │   │
│  │  │    --text-primary: #f8fafc;         │ │   │
│  │  │    ...                              │ │   │
│  │  └───────────────────────────────────────┘ │   │
│  │                                             │   │
│  │  [data-theme="dark"] override               │   │
│  │  [data-theme="custom"] override            │   │
│  └─────────────────────────────────────────────────┘   │
│                                                     │
│  Components (use var(--token-name))             │
└─────────────────────────────────────────────────────┘
```

**Implementation Strategy:**
1. Define all theme tokens in a single CSS file or Dioxus component
2. Inject theme variables at app root using `style` tag
3. Override variables via `[data-theme]` or class-based selectors
4. Components reference tokens using `var(--token-name, fallback)`

---

## 2. Color System Design

### 2.1 Color Token Naming Convention

Follow semantic naming for clarity and maintainability:

```css
/* Base Colors (Semantic Intent) */
--color-primary: #3b82f6;           /* Main action color */
--color-secondary: #64748b;         /* Secondary action */
--color-accent: #0ea5e9;             /* Highlights/notifications */
--color-success: #10b981;            /* Positive feedback */
--color-warning: #f59e0b;            /* Cautions */
--color-danger: #ef4444;              /* Errors/destructive */
--color-info: #06b6d4;               /* Neutral info */

/* Neutral Scale (Backgrounds & Text) */
--bg-primary: #ffffff;                 /* Main backgrounds */
--bg-secondary: #f3f4f6;             /* Cards/panels */
--bg-tertiary: #e5e7eb;             /* Hover states */
--bg-elevated: #ffffff;              /* Elevated surfaces */

--text-primary: #0f172a;              /* Headings, emphasis */
--text-secondary: #475569;            /* Body text */
--text-tertiary: #94a3b8;             /* Descriptions, labels */
--text-muted: #cbd5e1;                  /* Disabled, placeholders */

/* Semantic Surfaces */
--surface-window: #ffffff;              /* Window chrome */
--surface-dock: #f3f4f6;                /* Dock/toolbar */
--surface-input: #ffffff;                /* Form inputs */
--surface-overlay: rgba(0, 0, 0, 0.8);  /* Modals/dropdowns */

/* Border System */
--border-default: #e2e8f0;             /* Standard borders */
--border-strong: #cbd5e1;              /* Stronger borders */
--border-subtle: #f1f5f9;             /* Soft separators */
```

### 2.2 Color Palette Structure

**Light Theme (Default)**
```css
:root {
  /* Primary Brand Color */
  --color-primary: #3b82f6;
  --color-primary-hover: #2563eb;
  --color-primary-active: #1d4ed8;
  
  /* Neutral Backgrounds */
  --bg-primary: #ffffff;
  --bg-secondary: #f8fafc;
  --bg-tertiary: #f1f5f9;
  
  /* Neutral Text */
  --text-primary: #020617;
  --text-secondary: #475569;
  --text-muted: #94a3b8;
  
  /* Semantic Surfaces */
  --surface-window: #ffffff;
  --surface-elevated: #ffffff;
  --border-color: #e2e8f0;
}
```

**Dark Theme**
```css
[data-theme="dark"] {
  /* Invert but don't use pure black */
  --color-primary: #60a5fa;
  --color-primary-hover: #3b82f6;
  --color-primary-active: #93c5fd;
  
  /* Dark backgrounds (off-black) */
  --bg-primary: #0f172a;
  --bg-secondary: #1e293b;
  --bg-tertiary: #334155;
  
  /* Light text on dark backgrounds */
  --text-primary: #f8fafc;
  --text-secondary: #cbd5e1;
  --text-muted: #64748b;
  
  /* Dark surfaces */
  --surface-window: #1e293b;
  --surface-elevated: #334155;
  --border-color: #374151;
}
```

### 2.3 Color System Best Practices

**Contrast Ratios**
- Ensure WCAG AA compliance: 4.5:1 minimum contrast ratio
- Use automated contrast checker in design phase
- Test with color blindness simulators

**Desaturation Strategy**
- Dark themes should use desaturated primary colors
- Avoid fully saturated colors on dark backgrounds (causes eye strain)
- Reduce saturation by 15-30% for dark mode variants

**Depth & Elevation**
- Use opacity for depth perception in dark mode
- Light mode: darker colors for depth
- Dark mode: lighter colors with opacity for depth
- Consistent: 4-6 elevation levels maximum

---

## 3. Light/Dark Mode Support

### 3.1 System Preference Detection

Use CSS `prefers-color-scheme` media query for automatic detection:

```css
/* Default (light theme) - always defined */
:root {
  --bg-primary: #ffffff;
  --text-primary: #0f172a;
}

/* Automatic dark mode based on OS preference */
@media (prefers-color-scheme: dark) {
  :root {
    --bg-primary: #0f172a;
    --text-primary: #f8fafc;
  }
}

/* User override takes precedence over system preference */
[data-theme="light"]:root {
  --bg-primary: #ffffff;
  --text-primary: #0f172a;
}

[data-theme="dark"]:root {
  --bg-primary: #0f172a;
  --text-primary: #f8fafc;
}
```

### 3.2 JavaScript Toggle Implementation

```rust
// Dioxus component for theme toggle
#[component]
pub fn ThemeToggle() -> Element {
    let theme = use_theme_signal();
    let prefers_dark = use_sync_prefers_color_scheme();
    
    let toggle_theme = move |_| {
        let new_theme = match theme.read().as_str() {
            "light" => "dark",
            "dark" => "light",
            _ => "light", // Default to light on undefined
        };
        theme.set(new_theme.to_string());
        save_theme_preference(new_theme.to_string());
        apply_theme_to_document(new_theme.to_string());
    };
    
    rsx! {
        button {
            onclick: toggle_theme,
            class: "theme-toggle-btn",
            aria_label: "Toggle theme",
            "{if *theme.read() == "dark" { '☀️' } else { '🌙' }}"
        }
    }
}
```

### 3.3 Theme Persistence

**Backend-first with Optional localStorage Cache (Recommended)**
```rust
// Cache user preference (non-authoritative)
fn save_theme_preference(theme: &str) {
    window().local_storage()
        .set_item("theme-preference", theme)
        .unwrap();
}

// Load cached preference on app init
fn load_theme_preference() -> Option<String> {
    window().local_storage()
        .get_item("theme-preference")
        .ok()?
}

// Apply theme on app initialization while backend preference is loading
fn initialize_theme() {
    let saved = load_theme_preference();
    let system_prefers_dark = window()
        .match_media("(prefers-color-scheme: dark)")
        .matches();
    
    let final_theme = saved.unwrap_or_else(|| {
        if system_prefers_dark { "dark" } else { "light" }
    });
    
    apply_theme_to_document(&final_theme);
}
```

**Backend Storage (authoritative)**
```rust
// Sync with user profile
async fn sync_theme_to_backend(theme: &str) -> Result<(), ApiError> {
    let payload = serde_json::json!({
        "theme": theme
    });
    
    fetch("/api/user/preferences", post_json(payload)).await?;
}
```

---

## 4. Custom Themes and User Theme Creation

### 4.1 Theme Definition Structure

```rust
// Define theme schema
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Theme {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub is_builtin: bool,
    pub preview_url: Option<String>,
    pub tokens: ThemeTokens,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ThemeTokens {
    pub color_primary: String,
    pub color_secondary: Option<String>,
    pub bg_primary: String,
    pub bg_secondary: Option<String>,
    pub text_primary: String,
    pub text_secondary: Option<String>,
    // ... additional tokens
}

// Built-in themes
const BUILTIN_THEMES: &[Theme] = &[
    Theme {
        id: "light".to_string(),
        name: "Light".to_string(),
        description: Some("Clean light theme".to_string()),
        is_builtin: true,
        preview_url: None,
        tokens: ThemeTokens {
            color_primary: "#3b82f6".to_string(),
            bg_primary: "#ffffff".to_string(),
            text_primary: "#0f172a".to_string(),
            // ...
        },
    },
    Theme {
        id: "dark".to_string(),
        name: "Dark".to_string(),
        description: Some("Easy on the eyes dark theme".to_string()),
        is_builtin: true,
        preview_url: None,
        tokens: ThemeTokens {
            color_primary: "#60a5fa".to_string(),
            bg_primary: "#0f172a".to_string(),
            text_primary: "#f8fafc".to_string(),
            // ...
        },
    },
];
```

### 4.2 Theme Customization UI

```rust
#[component]
pub fn ThemeCreator() -> Element {
    let mut custom_theme = use_signal(|| Theme {
        id: "custom".to_string(),
        name: "Custom Theme".to_string(),
        is_builtin: false,
        preview_url: None,
        tokens: ThemeTokens {
            color_primary: "#3b82f6".to_string(),
            bg_primary: "#ffffff".to_string(),
            text_primary: "#0f172a".to_string(),
            // Use defaults from current theme
            ..ThemeTokens::default()
        },
    });
    
    let save_theme = move |_| {
        save_custom_theme(custom_theme.read().clone());
        switch_to_theme("custom".to_string());
    };
    
    rsx! {
        div { class: "theme-creator" {
            h2 { "Create Custom Theme" }
            
            // Color pickers for each token
            ColorPicker {
                label: "Primary Color",
                value: custom_theme.with(|t| t.tokens.color_primary.clone()),
                on_change: move |color| {
                    custom_theme.write().tokens.color_primary = color;
                }
            }
            
            ColorPicker {
                label: "Background",
                value: custom_theme.with(|t| t.tokens.bg_primary.clone()),
                on_change: move |color| {
                    custom_theme.write().tokens.bg_primary = color;
                }
            }
            
            ColorPicker {
                label: "Text Color",
                value: custom_theme.with(|t| t.tokens.text_primary.clone()),
                on_change: move |color| {
                    custom_theme.write().tokens.text_primary = color;
                }
            }
            
            // Preview window
            ThemePreview { theme: custom_theme.read() }
            
            button {
                onclick: save_theme,
                class: "btn-primary",
                "Save and Apply Theme"
            }
        }
    }
}
```

### 4.3 Theme Import/Export

```rust
// Export theme as JSON
pub fn export_theme(theme: &Theme) -> String {
    serde_json::to_string_pretty(theme).unwrap()
}

// Import theme from JSON
pub fn import_theme(json_str: &str) -> Result<Theme, serde_json::Error> {
    serde_json::from_str(json_str)
}

// Share theme URL
pub fn generate_share_url(theme: &Theme) -> String {
    let encoded = base64::encode(&export_theme(theme));
    format!("https://choiros.app/theme/import?data={}", encoded)
}
```

---

## 5. Theme Persistence and Storage

### 5.1 Multi-Layer Storage Strategy

```
┌─────────────────────────────────────────────────────────────┐
│  Storage Priority (Highest to Lowest)                 │
│  1. Backend Actor/EventStore                     │
│     - Synced across devices                          │
│     - Persistent across sessions                       │
│  2. localStorage cache (Browser)                │
│     - Device-specific                               │
│     - Fast startup hint, not canonical                 │
│  3. System Preference (OS)                     │
│     - Default for new users                        │
│     - Respects user's OS settings                    │
└─────────────────────────────────────────────────────────────┘
```

### 5.2 Storage Implementation

```rust
pub struct ThemeStorage {
    local: LocalStorage,
    backend: BackendApi,
}

impl ThemeStorage {
    /// Save theme preference with sync
    pub async fn save_theme(&self, theme_id: &str) -> Result<(), StorageError> {
        // Persist canonical state first
        if let Some(user_id) = self.backend.get_user_id()? {
            self.backend.update_user_theme(user_id, theme_id).await?;
        }

        // Cache in localStorage for startup speed
        self.local.set("theme", theme_id)?;
        
        Ok(())
    }
    
    /// Load theme with fallback chain
    pub async fn load_theme(&self) -> String {
        // 1. Try backend first (canonical)
        if let Some(user_id) = self.backend.get_user_id().await {
            if let Ok(Some(theme)) = self.backend.get_user_theme(user_id).await {
                // Cache to localStorage
                self.local.set("theme", &theme).ok();
                return theme;
            }
        }

        // 2. Use localStorage cache
        if let Some(theme) = self.local.get("theme") {
            return theme;
        }
        
        // 3. Fallback to system preference
        let prefers_dark = window()
            .match_media("(prefers-color-scheme: dark)")
            .matches();
        
        if prefers_dark { "dark" } else { "light" }
    }
}
```

---

## 6. Theme Application to Components

### 6.1 Dioxus Component Integration

**Using CSS Variables in Dioxus:**
```rust
#[component]
pub fn Button(props: ButtonProps) -> Element {
    rsx! {
        button {
            class: "btn",
            // Use CSS variable with fallback
            style: "background: var(--color-primary, #3b82f6); color: var(--color-primary-foreground, #ffffff);",
            {props.children}
        }
    }
}
```

**Themed Component Approach:**
```rust
#[component]
pub fn Button(props: ButtonProps) -> Element {
    // Read theme from context
    let theme = use_theme_context();
    
    rsx! {
        button {
            class: "btn",
            // Use CSS variables for automatic theme switching
            "background: var(--color-primary)",
            "color: var(--color-primary-foreground)",
            "border: 1px solid var(--border-default)",
            {props.children}
        }
    }
}
```

### 6.2 Component Theme Overrides

**Per-component theme customization:**
```rust
// Allow components to override theme tokens locally
#[component]
pub fn ThemedCard(props: ThemedCardProps) -> Element {
    let custom_style = if let Some(bg) = props.custom_bg.as_ref() {
        format!("background: {};", bg)
    } else {
        String::new()
    };
    
    rsx! {
        div {
            class: "card",
            // Custom style overrides CSS variables
            style: "{custom_style}",
            {props.children}
        }
    }
}
```

### 6.3 Theme Context for Deep Trees

```rust
// Theme provider context
pub struct ThemeContext {
    theme: Signal<String>,
    set_theme: Callback<String>,
}

impl ThemeContext {
    pub fn new(initial_theme: String) -> Self {
        let theme = use_signal(|| initial_theme);
        let set_theme = use_callback(move |new_theme: String| {
            theme.set(new_theme);
            apply_theme_to_document(&new_theme);
        });
        
        Self { theme, set_theme }
    }
}

// Provide context at app root
#[component]
pub fn ThemeProvider(props: ThemeProviderProps) -> Element {
    let ctx = ThemeContext::new(props.initial_theme.clone());
    
    rsx! {
        ThemeContextProvider { value: ctx.clone() {
            {props.children}
        }
    }
}
```

---

## 7. Icon Theming

### 7.1 SVG Icon Theming

**Direct fill/color theming:**
```css
/* Base icon styles */
.icon {
    width: 1.25rem;
    height: 1.25rem;
    display: inline-block;
}

/* Icon colors follow theme */
.icon svg,
.icon svg path {
    fill: currentColor; /* Inherits from text color */
    stroke: currentColor;
    transition: fill 0.2s, stroke 0.2s;
}

/* Specific icon theming */
.icon-primary svg { fill: var(--color-primary); }
.icon-secondary svg { fill: var(--text-secondary); }
.icon-accent svg { fill: var(--color-accent); }
```

**Dioxus icon component:**
```rust
#[component]
pub fn Icon(props: IconProps) -> Element {
    let icon_class = format!("icon icon-{}", props.variant);
    
    rsx! {
        span { class: "{icon_class}", 
            // Inline SVG for single-file icons
            dangerous_inner_html: &props.svg_path
        }
    }
}
```

### 7.2 Icon Set Variants

**Themed icon colors:**
```css
/* Light mode icons */
:root {
    --icon-success: #10b981;
    --icon-warning: #f59e0b;
    --icon-danger: #ef4444;
    --icon-info: #06b6d4;
}

/* Dark mode icons - adjust for contrast */
[data-theme="dark"] {
    --icon-success: #34d399;
    --icon-warning: #fbbf24;
    --icon-danger: #f87171;
    --icon-info: #38bdf8;
}
```

---

## 8. Window Chrome Theming

### 8.1 Desktop Window Styling

**Window titlebar:**
```css
.window-titlebar {
    background: var(--surface-window);
    border-bottom: 1px solid var(--border-subtle);
    /* Glassmorphism effect */
    backdrop-filter: blur(10px);
    background: var(--surface-window);
    /* Subtle transparency */
    background: var(--surface-window);
    background: rgba(
        var(--window-bg-rgb),
        var(--window-bg-alpha)
    );
}

/* Dark mode windows have different glass effect */
[data-theme="dark"] .window-titlebar {
    background: rgba(30, 41, 59, 0.95);
    border-bottom: 1px solid rgba(255, 255, 255, 0.1);
}
```

**Window controls (macOS/Windows styling):**
```css
.window-controls {
    display: flex;
    gap: 8px;
}

.window-controls button {
    width: 12px;
    height: 12px;
    border-radius: 50%;
    border: 1px solid var(--border-subtle);
    background: transparent;
    transition: all 0.2s;
}

.window-controls button:hover {
    background: var(--bg-tertiary);
}

/* Traffic light colors for macOS feel */
[data-window-controls="macos"] .window-controls button {
    &.close { background: #ff5f57; }
    &.minimize { background: #ffbd2e; }
    &.maximize { background: #27c93f; }
    
    &:hover { filter: brightness(1.1); }
}
```

### 8.2 Dock/Prompt Bar Theming

```css
.prompt-bar {
    background: var(--surface-dock);
    border-top: 1px solid var(--border-default);
    /* Elevated above workspace */
    box-shadow: 0 -4px 20px rgba(0, 0, 0, 0.1);
    backdrop-filter: blur(20px);
    background: rgba(
        var(--dock-bg-rgb),
        var(--dock-bg-alpha)
    );
}

/* Active app indicator */
.running-app.active {
    background: var(--color-primary);
    color: var(--color-primary-foreground);
    box-shadow: 0 0 10px rgba(var(--color-primary-rgb), 0.3);
}
```

---

## 9. Transition Animations Between Themes

### 9.1 Smooth Theme Switching

**CSS transitions for all theme-aware properties:**
```css
/* Apply transitions to theme-aware properties */
* {
    transition-property:
        background-color,
        color,
        border-color,
        fill,
        stroke,
        box-shadow;
    transition-duration: 0.2s;
    transition-timing-function: cubic-bezier(0.4, 0, 0.2, 1);
}

/* Faster transitions for interactive elements */
button,
a,
input {
    transition-duration: 0.1s;
}

/* Background elements transition slower */
.desktop-workspace {
    transition-duration: 0.3s;
}
```

### 9.2 Preventing FOUC (Flash of Unstyled Content)

**Critical CSS injection order:**
```html
<!DOCTYPE html>
<html>
<head>
    <!-- 1. Load CSS variables FIRST (no FOUC) -->
    <style>
        :root {
            --bg-primary: #ffffff;
            --text-primary: #0f172a;
            /* All theme tokens defined here */
        }
    </style>
    
    <!-- 2. Load component CSS -->
    <link rel="stylesheet" href="/styles.css">
    
    <!-- 3. Load Dioxus app (applies [data-theme]) -->
    <script type="module" src="/main.js"></script>
</head>
</html>
```

**Dioxus implementation:**
```rust
// Inject theme styles immediately in root component
#[component]
pub fn App() -> Element {
    let theme = use_theme_signal();
    
    rsx! {
        // Theme styles injected BEFORE children
        style {
            r#"
            :root {{
                --bg-primary: {};
                --text-primary: {};
                /* ... all other tokens */
            }}
            
            [data-theme="dark"] {{
                --bg-primary: #0f172a;
                --text-primary: #f8fafc;
            }}
            "#,
            if *theme.read() == "dark" {
                "var(--dark-bg-primary)",
                "var(--dark-text-primary)",
            } else {
                "var(--light-bg-primary)",
                "var(--light-text-primary)",
            }
        }
        
        // App children (now themed)
        Desktop {}
    }
}
```

### 9.3 Animated Theme Transitions

**Advanced: Cross-fade between themes:**
```css
@keyframes theme-cross-fade {
    0%, 100% { opacity: 1; }
    50% { opacity: 0; }
}

.theme-transitioning * {
    animation: theme-cross-fade 0.4s ease-in-out;
}
```

```rust
// Apply transition class in Dioxus
fn switch_theme(new_theme: String) {
    // Add transition class
    document()
        .body()
        .class_list()
        .add("theme-transitioning");
    
    // Wait for midpoint, switch theme
    set_timeout(Box::new(|| {
        apply_theme_to_document(&new_theme);
    }), 200);
    
    // Remove transition class
    set_timeout(Box::new(|| {
        document()
            .body()
            .class_list()
            .remove("theme-transitioning");
    }), 400);
}
```

---

## 10. High Contrast and Accessibility Considerations

### 10.1 WCAG Compliance Requirements

**Minimum Standards:**
- **WCAG AA**: 4.5:1 contrast ratio for normal text
- **WCAG AAA**: 7:1 contrast ratio for large text (18pt+)
- **Color Blindness Safe**: Works with protanopia, deuteranopia, etc.

**Contrast Checking:**
```rust
// Runtime contrast validation
fn validate_contrast(foreground: &str, background: &str) -> bool {
    let fg = parse_color(foreground);
    let bg = parse_color(background);
    let ratio = calculate_contrast_ratio(fg, bg);
    
    ratio >= 4.5 // WCAG AA
}

// Log warnings for low contrast
if !validate_contrast(&theme.text_primary, &theme.bg_primary) {
    warn!("Low contrast detected for text-primary on bg-primary");
}
```

### 10.2 High Contrast Mode

**Separate high-contrast theme variant:**
```css
/* High contrast theme for accessibility */
[data-theme="high-contrast"] {
    --color-primary: #0000ff; /* Pure blue */
    --bg-primary: #000000;     /* Pure black */
    --text-primary: #ffffff;     /* Pure white */
    --border-color: #ffffff;     /* Max contrast borders */
}

/* Increased font weight for readability */
[data-theme="high-contrast"] body {
    font-weight: 500;
}

/* Remove decorative elements */
[data-theme="high-contrast"] .shadow,
[data-theme="high-contrast"] .gradient {
    display: none;
}
```

### 10.3 Reduced Motion Preference

**Respect user's motion preferences:**
```css
/* Respect prefers-reduced-motion */
@media (prefers-reduced-motion: reduce) {
    * {
        animation-duration: 0.01ms !important;
        animation-iteration-count: 1 !important;
        transition-duration: 0.01ms !important;
    }
    
    /* Disable theme transition animation */
    .theme-transitioning * {
        animation: none !important;
    }
}
```

```rust
// Detect reduced motion in Dioxus
let prefers_reduced_motion = window()
    .match_media("(prefers-reduced-motion: reduce)")
    .matches();

// Skip animations if requested
let transition_duration = if prefers_reduced_motion {
    "0ms"
} else {
    "0.2s"
};
```

### 10.4 Focus Indicators in Themed Components

**Visible focus states across all themes:**
```css
/* Always-visible focus ring */
*:focus-visible {
    outline: 2px solid var(--color-primary);
    outline-offset: 2px;
}

/* Ensure focus ring has sufficient contrast */
[data-theme="dark"] *:focus-visible {
    outline-color: var(--color-primary);
    outline-width: 3px;
}
```

---

## 11. Theme Preview and Selection UI

### 11.1 Theme Selector Component

```rust
#[component]
pub fn ThemeSelector() -> Element {
    let themes = use_theme_list();
    let current_theme = use_theme_signal();
    let is_open = use_signal(|| false);
    
    rsx! {
        div { class: "theme-selector" {
            button {
                class: "theme-selector-trigger",
                onclick: move |_| is_open.set(!is_open()),
                "🎨"
            }
            
            if *is_open() {
                div { class: "theme-dropdown" {
                    for theme in themes.iter() {
                        ThemeCard {
                            theme: theme.clone(),
                            is_active: *current_theme.read() == theme.id,
                            on_select: move |theme_id| {
                                set_theme(theme_id);
                                is_open.set(false);
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ThemeCard(props: ThemeCardProps) -> Element {
    let active_class = if props.is_active {
        "theme-card theme-card--active"
    } else {
        "theme-card"
    };
    
    rsx! {
        button {
            class: "{active_class}",
            onclick: move |_| props.on_select(props.theme.id.clone()),
            
            // Preview colored circle
            div { class: "theme-preview",
                style: "background: var(--color-primary);"
            }
            
            h3 { "{props.theme.name}" }
            
            if let Some(desc) = props.theme.description.as_ref() {
                p { class: "theme-description", "{desc}" }
            }
            
            if props.is_active {
                span { class: "active-indicator", "✓" }
            }
        }
    }
}
```

### 11.2 Theme Preview Window

```rust
#[component]
pub fn ThemePreviewWindow() -> Element {
    let themes = use_theme_list();
    let active_index = use_signal(|| 0usize);
    
    rsx! {
        div { class: "theme-preview-container" {
            // Thumbnail grid
            div { class: "theme-grid" {
                for (index, theme) in themes.iter().enumerate() {
                    button {
                        class: if *active_index.read() == index {
                            "theme-thumb theme-thumb--active"
                        } else {
                            "theme-thumb"
                        },
                        onclick: move |_| active_index.set(index),
                        style: "background: var(--bg-primary); color: var(--text-primary);",
                        
                        // Mini preview of theme
                        div { class: "theme-thumb-preview",
                            style: format!(
                                "background: {}; color: {};",
                                theme.tokens.bg_primary, theme.tokens.text_primary
                            )
                        }
                        
                        h4 { "{theme.name}" }
                    }
                }
            }
            
            // Live preview area
            div { class: "theme-live-preview" {
                // Sample window with current theme
                div { class: "mini-window",
                    style: "background: var(--surface-window); border: 1px solid var(--border-default);",
                    
                    div { class: "mini-titlebar",
                        style: "background: var(--surface-dock);",
                        span { "Preview Window" }
                    }
                    
                    div { class: "mini-content",
                        style: "background: var(--bg-primary); color: var(--text-primary);",
                        "Theme preview content"
                    }
                }
            }
        }
    }
}
```

### 11.3 Color Picker Integration

```rust
#[component]
pub fn ColorPicker(props: ColorPickerProps) -> Element {
    let is_open = use_signal(|| false);
    
    rsx! {
        div { class: "color-picker-wrapper" {
            button {
                class: "color-preview",
                onclick: move |_| is_open.set(!is_open()),
                style: "background: {props.value};",
                aria_label: &props.label,
            }
            
            if *is_open() {
                div { class: "color-picker-popover" {
                    // Preset colors
                    div { class: "color-presets" {
                        for color in PRESET_COLORS.iter() {
                            button {
                                class: "color-swatch",
                                onclick: move |_| {
                                    props.on_change(color.to_string());
                                    is_open.set(false);
                                },
                                style: "background: {color};",
                                aria_label: color,
                            }
                        }
                    }
                    
                    // Custom color input
                    input {
                        r#type: "color",
                        value: "{props.value}",
                        onchange: move |e| {
                            if let Event::FormData(form_data) = e {
                                props.on_change(form_data.value().to_string());
                            }
                        }
                    }
                }
            }
        }
    }
}
```

---

## 12. Implementation Roadmap for Dioxus

### Phase 1: Foundation (Week 1)
- [ ] Create `ThemeContext` provider
- [ ] Define base CSS token set
- [ ] Implement light/dark theme tokens
- [ ] Add `[data-theme]` attribute support
- [ ] Create theme toggle component
- [ ] Add backend preference persistence + localStorage cache

### Phase 2: Components (Week 2)
- [ ] Migrate existing components to use CSS variables
- [ ] Create `ThemeButton` component
- [ ] Create `ThemedCard` component
- [ ] Update window chrome styling
- [ ] Theme dock/prompt bar

### Phase 3: Customization (Week 3)
- [ ] Build theme selector UI
- [ ] Create theme preview system
- [ ] Implement custom theme creator
- [ ] Add theme import/export
- [ ] Add color picker component

### Phase 4: Advanced (Week 4)
- [ ] High contrast mode
- [ ] Reduced motion support
- [ ] Multiple color schemes beyond light/dark
- [ ] Theme sync with backend
- [ ] Accessibility compliance testing

---

## 13. Recommended File Structure

```
sandbox-ui/
├── src/
│   ├── theme/
│   │   ├── mod.rs
│   │   ├── tokens.rs         # Theme token definitions
│   │   ├── context.rs        # ThemeProvider & hooks
│   │   ├── themes.rs         # Built-in theme definitions
│   │   └── storage.rs       # Theme persistence
│   ├── components/
│   │   ├── button.rs
│   │   ├── card.rs
│   │   ├── window.rs
│   │   └── ...
│   └── main.rs
└── public/
    ├── styles/
    │   └── tokens.css        # CSS variable definitions
    └── index.html
```

---

## 14. Best Practices Summary

### DO ✅
- **Use CSS Custom Properties** for dynamic theming
- **Define tokens semantically** (--color-primary, not --blue-500)
- **Support system preferences** with `prefers-color-scheme`
- **Provide manual override** for user control
- **Persist canonical preferences** in backend actor/EventStore and optionally cache in localStorage
- **Ensure WCAG compliance** for all color combinations
- **Test with screen readers** and keyboard navigation
- **Support reduced motion** preferences
- **Use CSS transitions** for smooth theme switching
- **Validate contrast** at build time
- **Provide fallback values** in `var()` calls
- **Test with color blindness simulators**
- **Document theme tokens** for designers

### DON'T ❌
- **Don't use preprocessor variables** for runtime theme switching
- **Don't inline theme styles** in components
- **Don't rely on JavaScript-only** theme switching (no CSS fallback)
- **Don't use pure black (#000000)** in dark mode
- **Don't hardcode colors** in component CSS
- **Don't ignore system preferences**
- **Don't create flash of unstyled content** (FOUC)
- **Don't forget hover/focus states** in all themes
- **Don't sacrifice accessibility** for aesthetics
- **Don't use too many color variations** (max 3-4 main variants)
- **Don't ignore reduced motion** preferences

---

## 15. References and Resources

### CSS & Theming
- [MDN: Using CSS Custom Properties](https://developer.mozilla.org/en-US/docs/Web/CSS/Using_CSS_custom_properties)
- [CSS Custom Properties Spec](https://www.w3.org/TR/css-variables-1/)
- [CSS-Tricks: Dark Mode Guide](https://css-tricks.com/a-complete-guide-to-dark-mode-on-the-web/)
- [Material Design: Dark Theme](https://material.io/design/color/dark-theme.html)

### UI Libraries (Inspiration)
- [MUI (Material UI): Theming System](https://mui.com/material-ui/customization/theming/)
- [shadcn/ui: Theming with CSS Variables](https://ui.shadcn.com/docs/theming)
- [Radix UI: Theming](https://www.radix-ui.com/docs/theming)
- [Chakra UI: Theming](https://chakra-ui.com/docs/styled-system/theming)

### Accessibility
- [WCAG 2.1 Guidelines](https://www.w3.org/WAI/WCAG21/quickref/)
- [WebAIM Contrast Checker](https://webaim.org/resources/contrastchecker/)
- [A11y Project Color Contrast](https://a11yproject.com/color-contrast)

### Dioxus
- [Dioxus Documentation](https://dioxuslabs.com/)
- [Dioxus Styling Guide](https://dioxuslabs.com/learn/0.6/guide/styling)

### Tools
- [Coolors: Color Palette Generator](https://coolors.co/)
- [Contrast Checker](https://contrastchecker.com/)
- [Color blindness simulator](https://www.color-blindness.com/)

---

## Appendix A: Complete Theme Token Set

```css
:root {
  /* === Primary Colors === */
  --color-primary: #3b82f6;
  --color-primary-hover: #2563eb;
  --color-primary-active: #1d4ed8;
  --color-primary-foreground: #ffffff;
  
  /* === Secondary Colors === */
  --color-secondary: #64748b;
  --color-secondary-hover: #475569;
  --color-secondary-active: #334155;
  
  /* === Accent Colors === */
  --color-accent: #0ea5e9;
  --color-success: #10b981;
  --color-warning: #f59e0b;
  --color-danger: #ef4444;
  --color-info: #06b6d4;
  
  /* === Neutral Backgrounds === */
  --bg-primary: #ffffff;
  --bg-secondary: #f8fafc;
  --bg-tertiary: #f1f5f9;
  --bg-elevated: #ffffff;
  --bg-overlay: rgba(0, 0, 0, 0.8);
  
  /* === Neutral Text === */
  --text-primary: #0f172a;
  --text-secondary: #475569;
  --text-tertiary: #94a3b8;
  --text-muted: #cbd5e1;
  --text-inverse: #ffffff;
  
  /* === Semantic Surfaces === */
  --surface-window: #ffffff;
  --surface-dock: #f3f4f6;
  --surface-input: #ffffff;
  --surface-card: #ffffff;
  
  /* === Borders === */
  --border-default: #e2e8f0;
  --border-strong: #cbd5e1;
  --border-subtle: #f1f5f9;
  
  /* === Shadows === */
  --shadow-sm: 0 1px 2px rgba(0, 0, 0, 0.05);
  --shadow-md: 0 4px 6px rgba(0, 0, 0, 0.1);
  --shadow-lg: 0 10px 15px rgba(0, 0, 0, 0.15);
  --shadow-xl: 0 20px 25px rgba(0, 0, 0, 0.25);
  
  /* === Typography === */
  --font-sans: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
  --font-mono: 'SF Mono', Monaco, 'Cascadia Code', monospace;
  --font-size-xs: 0.75rem;
  --font-size-sm: 0.875rem;
  --font-size-base: 1rem;
  --font-size-lg: 1.125rem;
  --font-size-xl: 1.25rem;
  
  /* === Spacing === */
  --space-1: 0.25rem;
  --space-2: 0.5rem;
  --space-3: 0.75rem;
  --space-4: 1rem;
  --space-5: 1.25rem;
  --space-6: 1.5rem;
  --space-8: 2rem;
  
  /* === Border Radius === */
  --radius-sm: 0.25rem;
  --radius-md: 0.5rem;
  --radius-lg: 0.75rem;
  --radius-full: 9999px;
  
  /* === Transitions === */
  --duration-fast: 0.1s;
  --duration-base: 0.2s;
  --duration-slow: 0.3s;
  --easing-default: cubic-bezier(0.4, 0, 0.2, 1);
}

/* === Dark Theme === */
[data-theme="dark"] {
  /* Primary colors - desaturated */
  --color-primary: #60a5fa;
  --color-primary-hover: #3b82f6;
  --color-primary-active: #93c5fd;
  
  /* Dark backgrounds (off-black) */
  --bg-primary: #0f172a;
  --bg-secondary: #1e293b;
  --bg-tertiary: #334155;
  --bg-elevated: #1e293b;
  
  /* Light text on dark backgrounds */
  --text-primary: #f8fafc;
  --text-secondary: #cbd5e1;
  --text-tertiary: #94a3b8;
  --text-muted: #64748b;
  
  /* Dark surfaces */
  --surface-window: #1e293b;
  --surface-dock: #0f172a;
  --surface-input: #1e293b;
  --surface-card: #1e293b;
  
  /* Dark borders */
  --border-default: #374151;
  --border-strong: #475569;
  --border-subtle: #1e293b;
  
  /* Dark shadows (use opacity) */
  --shadow-sm: 0 1px 2px rgba(0, 0, 0, 0.3);
  --shadow-md: 0 4px 6px rgba(0, 0, 0, 0.4);
  --shadow-lg: 0 10px 15px rgba(0, 0, 0, 0.5);
}

/* === High Contrast Theme === */
[data-theme="high-contrast"] {
  --color-primary: #0000ff;
  --bg-primary: #000000;
  --text-primary: #ffffff;
  --border-default: #ffffff;
}
```

---

*Document Version: 1.0*  
*Last Updated: 2025-02-05*  
*Author: Theme Research for ChoirOS Web Desktop*
