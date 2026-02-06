use dioxus::prelude::*;
use shared_types::AppDefinition;

#[component]
pub fn DesktopIcons(
    apps: Vec<AppDefinition>,
    on_open_app: Callback<AppDefinition>,
    is_mobile: bool,
) -> Element {
    let columns = if is_mobile { 2 } else { 4 };
    let icon_size = if is_mobile { "4rem" } else { "5rem" };

    rsx! {
        div {
            class: "desktop-icons",
            style: "position: absolute; top: 1rem; left: 1rem; z-index: 1; display: grid; grid-template-columns: repeat({columns}, {icon_size}); gap: 1.5rem; padding: 1rem;",

            for app in apps {
                DesktopIcon {
                    app: app.clone(),
                    on_open_app,
                    is_mobile,
                }
            }
        }
    }
}

#[component]
pub fn DesktopIcon(
    app: AppDefinition,
    on_open_app: Callback<AppDefinition>,
    is_mobile: bool,
) -> Element {
    let icon_size = if is_mobile { "3rem" } else { "3.5rem" };
    let font_size = if is_mobile { "2rem" } else { "2.5rem" };
    let mut last_click_time = use_signal(|| 0i64);
    let mut is_pressed = use_signal(|| false);

    let app_for_closure = app.clone();
    let app_icon = app.icon.clone();
    let app_name = app.name.clone();

    let handle_click = move |_| {
        let now = js_sys::Date::now() as i64;
        let last = *last_click_time.read();

        if now - last >= 500 {
            on_open_app.call(app_for_closure.clone());
            last_click_time.set(now);
        }

        is_pressed.set(true);
        let mut is_pressed_clone = is_pressed;
        spawn(async move {
            wasm_bindgen_futures::JsFuture::from(js_sys::Promise::new(&mut |resolve, _| {
                web_sys::window()
                    .unwrap()
                    .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, 150)
                    .unwrap();
            }))
            .await
            .unwrap();
            is_pressed_clone.set(false);
        });
    };

    let bg_opacity = if *is_pressed.read() { "0.95" } else { "0.8" };
    let scale = if *is_pressed.read() { "0.95" } else { "1.0" };
    let border_color = if *is_pressed.read() {
        "#60a5fa"
    } else {
        "#334155"
    };
    let shadow = if *is_pressed.read() {
        "0 2px 12px rgba(96, 165, 250, 0.5)"
    } else {
        "none"
    };

    rsx! {
        button {
            class: "desktop-icon",
            style: "display: flex; flex-direction: column; align-items: center; gap: 0.5rem; padding: 0.75rem; background: transparent; border: none; border-radius: var(--radius-md, 8px); cursor: pointer; transition: all 0.15s ease-out; transform: scale({scale});",
            onclick: handle_click,
            onmouseleave: move |_| is_pressed.set(false),

            div {
                style: "width: {icon_size}; height: {icon_size}; display: flex; align-items: center; justify-content: center; background: var(--dock-bg, rgba(30, 41, 59, {bg_opacity})); border-radius: var(--radius-lg, 12px); backdrop-filter: blur(8px); border: 1px solid {border_color}; box-shadow: {shadow}; transition: all 0.15s ease-out;",
                span { style: "font-size: {font_size}; pointer-events: none; user-select: none;", "{app_icon}" }
            }
            span {
                style: "font-size: 0.75rem; color: var(--text-secondary, #94a3b8); text-align: center; max-width: 100%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; text-shadow: 0 1px 2px rgba(0,0,0,0.5); pointer-events: none; user-select: none;",
                "{app_name}"
            }
        }
    }
}
