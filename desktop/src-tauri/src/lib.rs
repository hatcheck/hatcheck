//! Hatcheck's desktop app: a menu-bar (tray) icon with a popover, and a main window, both
//! front ends to the hatcheck engine. The engine is the hatcheck CLI, shipped beside this
//! app's executable as a standalone binary (a Tauri sidecar); nothing about Claude Code's or
//! Codex's files is read here. The web pages ask the engine for JSON through `engine`.

use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, WindowEvent};

/// The PATH a login shell sets, read once. Apps started from the Finder get a bare PATH, and
/// the engine checks for cl and cx on it to print the commands a terminal can run.
static LOGIN_PATH: OnceLock<Option<String>> = OnceLock::new();

fn login_path() -> Option<String> {
    if cfg!(windows) {
        return None;
    }
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
    let out = Command::new(shell).args(["-l", "-c", "printf %s \"$PATH\""]).output().ok()?;
    let path = String::from_utf8(out.stdout).ok()?;
    (out.status.success() && !path.trim().is_empty()).then(|| path.trim().to_string())
}

/// The engine: hatcheck beside this executable (the sidecar), or else hatcheck on the PATH.
fn engine_path() -> PathBuf {
    let name = if cfg!(windows) { "hatcheck.exe" } else { "hatcheck" };
    if let Some(dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf())) {
        let beside = dir.join(name);
        if beside.is_file() {
            return beside;
        }
    }
    PathBuf::from(name)
}

/// Run the engine with args: its output, or its error message.
fn run_engine(args: &[String], terminal: Option<&str>) -> Result<String, String> {
    let mut cmd = Command::new(engine_path());
    cmd.args(args).env("NO_COLOR", "1").env("PYTHONIOENCODING", "utf-8");
    if let Some(Some(path)) = LOGIN_PATH.get() {
        cmd.env("PATH", path);
    }
    if let Some(name) = terminal.filter(|t| !t.is_empty()) {
        cmd.env("HATCHECK_TERMINAL", name);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW: no console flashing up
    }
    let out = cmd.output().map_err(|e| format!("Couldn't run the hatcheck engine ({e})"))?;
    if out.status.success() {
        return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
    }
    let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
    Err(if err.is_empty() { format!("hatcheck exited with {}", out.status) } else { err })
}

/// For the web pages: run the engine, off the main thread.
#[tauri::command]
async fn engine(args: Vec<String>, terminal: Option<String>) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || run_engine(&args, terminal.as_deref()))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
fn open_main(app: AppHandle, tab: Option<String>) {
    show_main(&app, tab);
}

#[tauri::command]
fn hide_popover(app: AppHandle) {
    if let Some(win) = app.get_webview_window("tray") {
        let _ = win.hide();
    }
}

#[tauri::command]
fn quit(app: AppHandle) {
    app.exit(0);
}

fn show_main(app: &AppHandle, tab: Option<String>) {
    if let Some(win) = app.get_webview_window("tray") {
        let _ = win.hide();
    }
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
        let _ = app.emit_to("main", "shown", tab);
    }
}

/// Show the popover under the menu-bar icon (above it when the taskbar is at the bottom, as
/// on Windows), or hide it if it's showing.
fn toggle_popover(app: &AppHandle, rect: tauri::Rect) {
    let Some(win) = app.get_webview_window("tray") else { return };
    if win.is_visible().unwrap_or(false) {
        let _ = win.hide();
        return;
    }
    let scale = win.scale_factor().unwrap_or(1.0);
    let icon_at = rect.position.to_physical::<f64>(scale);
    let icon_size = rect.size.to_physical::<f64>(scale);
    let size = win.outer_size().map(|s| (s.width as f64, s.height as f64)).unwrap_or((420.0 * scale, 640.0 * scale));
    let screen = win
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| win.primary_monitor().ok().flatten())
        .map(|m| (m.position().x as f64, m.position().y as f64, m.size().width as f64, m.size().height as f64));
    let mut x = icon_at.x + icon_size.width / 2.0 - size.0 / 2.0;
    let mut y = icon_at.y + icon_size.height + 6.0 * scale;
    if let Some((sx, sy, sw, sh)) = screen {
        x = x.clamp(sx + 8.0 * scale, (sx + sw - size.0 - 8.0 * scale).max(sx));
        if icon_at.y > sy + sh / 2.0 {
            y = icon_at.y - size.1 - 6.0 * scale; // the taskbar is at the bottom
        }
    }
    let _ = win.set_position(PhysicalPosition::new(x, y));
    let _ = win.show();
    let _ = win.set_focus();
    let _ = app.emit_to("tray", "shown", ());
}

pub fn run() {
    // Read the login shell's PATH in the background; the first engine run waits at most for it.
    std::thread::spawn(|| {
        LOGIN_PATH.get_or_init(login_path);
    });
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![engine, open_main, hide_popover, quit])
        .setup(|app| {
            // A menu-bar app: no Dock icon, and no app menu of its own on macOS.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            let open = MenuItem::with_id(app, "open", "Open Hatcheck", true, None::<&str>)?;
            let refresh = MenuItem::with_id(app, "refresh", "Refresh", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit Hatcheck", true, None::<&str>)?;
            let separator = PredefinedMenuItem::separator(app)?;
            let menu = Menu::with_items(app, &[&open, &refresh, &separator, &quit])?;
            // macOS tints a black template image to suit the menu bar. Windows and Linux show an
            // icon as it is, so they get the coloured tile, which reads on light and dark taskbars.
            #[cfg(target_os = "macos")]
            let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png"))?;
            #[cfg(not(target_os = "macos"))]
            let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray-color.png"))?;
            TrayIconBuilder::with_id("hatcheck")
                .icon(icon)
                .icon_as_template(true)
                .tooltip("Hatcheck")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open" => show_main(app, None),
                    "refresh" => {
                        let _ = app.emit("refresh", ());
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, rect, .. } = event {
                        toggle_popover(tray.app_handle(), rect);
                    }
                })
                .build(app)?;
            Ok(())
        })
        .on_window_event(|window, event| match event {
            // The popover goes away when you click elsewhere, as a menu does.
            WindowEvent::Focused(false) if window.label() == "tray" => {
                let _ = window.hide();
            }
            // Closing the main window hides it; the app lives on in the menu bar.
            WindowEvent::CloseRequested { api, .. } if window.label() == "main" => {
                api.prevent_close();
                let _ = window.hide();
            }
            _ => {}
        })
        .run(tauri::generate_context!())
        .expect("Hatcheck couldn't start");
}

/// --selftest: run the engine as the app does and print a summary; 0 when every run worked.
pub fn selftest() -> i32 {
    LOGIN_PATH.get_or_init(login_path);
    println!("engine: {}", engine_path().display());
    let mut failed = 0;
    let runs: [&[&str]; 4] = [
        &["--version"],
        &["usage", "--json", "--cached"],
        &["--json", "--cached", "--no-usage", "--all-folders", "48"],
        &["where", "--json", "0"],
    ];
    for args in runs {
        let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        match run_engine(&args, None) {
            Ok(out) => {
                let summary = match serde_json::from_str::<serde_json::Value>(&out) {
                    Ok(v) => format!(
                        "JSON with {}",
                        v.as_object().map(|o| o.keys().cloned().collect::<Vec<_>>().join(", ")).unwrap_or_default()
                    ),
                    Err(_) => out.trim().to_string(),
                };
                println!("ok    hatcheck {}: {}", args.join(" "), summary);
            }
            // where exits 1 when nothing matches, which is fine here.
            Err(e) if args[0] == "where" => println!("ok    hatcheck {}: {}", args.join(" "), e),
            Err(e) => {
                failed += 1;
                println!("FAIL  hatcheck {}: {}", args.join(" "), e);
            }
        }
    }
    i32::from(failed > 0)
}
