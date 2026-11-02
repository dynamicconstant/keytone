mod commands;
mod settings;
mod state;

use keytone_core::engine::Engine;
use keytone_core::library::Library;
use keytone_core::Player;
use parking_lot::Mutex;
use serde::Serialize;
use settings::Settings;
use state::{AppState, HookStatus};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, WindowEvent};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

#[derive(Clone, Serialize)]
struct KeyPayload {
    code: &'static str,
    down: bool,
}

pub fn run() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let start_hidden = std::env::args().any(|a| a == "--hidden");

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--hidden"]),
        ))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        let on = !app.state::<AppState>().player.enabled();
                        commands::apply_enabled(app, on);
                    }
                })
                .build(),
        )
        .setup(move |app| {
            let handle = app.handle().clone();
            let state = init_state(&handle)?;
            app.manage(state);
            let st = app.state::<AppState>();

            // Live key feedback for the UI keyboard (only when it asked for it).
            {
                let h = handle.clone();
                let flag = st.key_feedback.clone();
                st.player.set_observer(move |ev| {
                    if flag.load(Ordering::Relaxed) {
                        let _ = h.emit(
                            "key",
                            KeyPayload { code: ev.code, down: ev.dir == keytone_core::keyhook::Dir::Down },
                        );
                    }
                });
            }

            if let Err(e) = st.load_active() {
                log::warn!("{e}");
            }
            st.start_hook();

            let hk = st.settings.lock().toggle_hotkey.clone();
            if let Err(e) = register_hotkey(&handle, hk.as_deref()) {
                log::warn!("{e}");
            }

            build_tray(&handle)?;

            if let Some(w) = app.get_webview_window("main") {
                if !start_hidden {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window keeps Keytone running in the tray.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
                window.state::<AppState>().key_feedback.store(false, Ordering::Relaxed);
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::set_enabled,
            commands::set_volume,
            commands::set_key_feedback,
            commands::preview_key,
            commands::preview_ref,
            commands::set_active_pack,
            commands::set_mapping,
            commands::update_pack_meta,
            commands::import_sound,
            commands::delete_sound,
            commands::create_pack,
            commands::duplicate_pack,
            commands::delete_pack,
            commands::import_pack,
            commands::export_pack,
            commands::start_recording,
            commands::recording_status,
            commands::stop_recording,
            commands::preview_take,
            commands::save_take,
            commands::discard_take,
            commands::list_devices,
            commands::update_settings,
            commands::request_permission,
            commands::retry_hook,
            commands::open_permission_settings,
            commands::restart_app,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Keytone");
}

fn init_state(app: &AppHandle) -> anyhow::Result<AppState> {
    let data = app.path().app_data_dir()?;
    let config = app.path().app_config_dir()?;
    let settings_path = config.join("settings.json");
    let mut settings = Settings::load(&settings_path);

    let library = Library::open(data.join("packs"))?;
    match bundled_packs_dir(app) {
        Some(bundled) => match library.install_bundled(&bundled, &settings.installed_bundled) {
            Ok(new) => {
                if !new.is_empty() {
                    log::info!("installed bundled packs: {new:?}");
                }
                settings.installed_bundled.extend(new);
            }
            Err(e) => log::warn!("installing bundled packs: {e:#}"),
        },
        None => log::warn!("bundled packs folder not found"),
    }

    // Make sure the active pack exists.
    let packs = library.list();
    if !settings.active_pack.as_ref().map(|a| packs.iter().any(|p| &p.id == a)).unwrap_or(false) {
        settings.active_pack = packs.first().map(|p| p.id.clone());
    }

    let engine = Engine::start(settings.output_device.clone(), settings.buffer_frames);
    engine.set_volume(settings.volume);
    let player = Player::new(engine);
    player.set_enabled(settings.enabled);

    let st = AppState {
        player,
        library,
        settings: Mutex::new(settings),
        settings_path,
        hook: Mutex::new(HookStatus::default()),
        recording: Mutex::new(None),
        take: Mutex::new(None),
        key_feedback: Arc::new(AtomicBool::new(false)),
        tray_enabled_item: Mutex::new(None),
    };
    st.save_settings();
    Ok(st)
}

fn bundled_packs_dir(app: &AppHandle) -> Option<PathBuf> {
    let candidates = [
        app.path().resource_dir().ok().map(|r| r.join("packs")),
        // `tauri dev` / `cargo run` from the repo.
        Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../packs")),
    ];
    candidates.into_iter().flatten().find(|p| p.is_dir())
}

pub(crate) fn register_hotkey(app: &AppHandle, hotkey: Option<&str>) -> Result<(), String> {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    if let Some(hk) = hotkey.filter(|h| !h.trim().is_empty()) {
        let sc: Shortcut = hk.parse().map_err(|e| format!("Invalid shortcut “{hk}”: {e}"))?;
        gs.register(sc).map_err(|e| format!("Could not register “{hk}” (is another app using it?): {e}"))?;
    }
    Ok(())
}

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let st = app.state::<AppState>();
    let enabled = CheckMenuItem::with_id(app, "enabled", "Sounds on", true, st.player.enabled(), None::<&str>)?;
    let open = MenuItem::with_id(app, "open", "Open Keytone…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Keytone", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&enabled, &PredefinedMenuItem::separator(app)?, &open, &quit])?;
    *st.tray_enabled_item.lock() = Some(enabled);

    let mut tray = TrayIconBuilder::with_id("keytone-tray")
        .tooltip("Keytone")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "enabled" => {
                let on = !app.state::<AppState>().player.enabled();
                commands::apply_enabled(app, on);
            }
            "open" => show_main(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}
