use tauri::Manager;

fn glass_pref_path(app: &tauri::AppHandle) -> Option<std::path::PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join("glass.json"))
}

fn load_glass_pref(app: &tauri::AppHandle) -> bool {
    glass_pref_path(app)
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v.get("enabled").and_then(|b| b.as_bool()))
        .unwrap_or(false)
}

fn save_glass_pref(app: &tauri::AppHandle, enabled: bool) {
    if let Some(path) = glass_pref_path(app) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(path, serde_json::json!({ "enabled": enabled }).to_string());
    }
}

/// Applies (or clears) real OS window-transparency effects. macOS gets true
/// vibrancy, Windows 11 gets Mica (falling back to Acrylic on older builds);
/// Linux has no equivalent — compositor blur isn't an app-controllable API
/// and `window-vibrancy` doesn't support it, so this returns an explicit
/// error the frontend surfaces rather than a silent no-op. The in-app
/// `glassLevel` CSS theming (frontend/src/store/theme.ts) is unrelated
/// decorative blur and still works on every platform regardless of this.
fn apply_glass(window: &tauri::WebviewWindow, enabled: bool) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use window_vibrancy::{apply_vibrancy, clear_vibrancy, NSVisualEffectMaterial};
        if enabled {
            apply_vibrancy(window, NSVisualEffectMaterial::Sidebar, None, None)
                .map_err(|e| e.to_string())?;
        } else {
            clear_vibrancy(window).map_err(|e| e.to_string())?;
        }
        return Ok(());
    }

    #[cfg(target_os = "windows")]
    {
        use window_vibrancy::{apply_acrylic, apply_mica, clear_acrylic, clear_mica};
        if enabled {
            // Mica needs Windows 11; fall back to Acrylic (works from Win10 1809+).
            if apply_mica(window, None).is_err() {
                apply_acrylic(window, Some((18, 18, 18, 125))).map_err(|e| e.to_string())?;
            }
        } else {
            let _ = clear_mica(window);
            let _ = clear_acrylic(window);
        }
        return Ok(());
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = (window, enabled);
        return Err("Window transparency effects aren't supported on Linux yet".to_string());
    }
}

#[tauri::command]
fn set_glass(window: tauri::WebviewWindow, enabled: bool) -> Result<(), String> {
    apply_glass(&window, enabled)?;
    save_glass_pref(&window.app_handle().clone(), enabled);
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_os::init())
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }

            // Re-apply the user's saved glass preference on launch, not just
            // on the live toggle. Errors here (e.g. Linux) are expected and
            // silently ignored — the Settings UI is what surfaces the error
            // message, at toggle time, not at every app startup.
            let enabled = load_glass_pref(&app.handle().clone());
            if enabled {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = apply_glass(&window, true);
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![set_glass])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
