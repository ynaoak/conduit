use tauri::{
    image::Image,
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Emitter, Manager,
};

mod commands;
mod config;
mod double_tap;
mod hotkey;
mod ja;
mod pins;
mod plugin_system;
mod plugins;
mod window;

use config::ConfigState;
use plugin_system::registry::PluginRegistry;

fn show_launcher(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.center();
        let _ = window.show();
        let _ = window.set_focus();
        let _ = window.emit("conduit://focus-search", ());
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let handle = app.handle().clone();

            // Load config from app config dir
            let config_dir = app
                .path()
                .app_config_dir()
                .expect("failed to resolve app config dir");
            let config_state = ConfigState::load(&config_dir);
            let app_config = tauri::async_runtime::block_on(config_state.get());

            app.manage(config_state.clone());
            app.manage(pins::PinsState::load(&config_dir));

            // Initialize plugin registry with config
            let registry = PluginRegistry::new(config_state.clone());
            app.manage(registry);

            // Register plugins asynchronously
            let registry_state: tauri::State<'_, PluginRegistry> = app.state();
            let registry_ref = registry_state.inner().clone();
            let config_for_plugins = config_state.clone();
            let handle_for_plugins = handle.clone();

            tauri::async_runtime::spawn(async move {
                registry_ref
                    .register(Box::new(plugins::calculator::CalculatorPlugin::new(
                        handle_for_plugins.clone(),
                    )))
                    .await;
                registry_ref
                    .register(Box::new(plugins::app_launcher::AppLauncherPlugin::new(
                        handle_for_plugins.clone(),
                    )))
                    .await;
                registry_ref
                    .register(Box::new(
                        plugins::system_commands::SystemCommandsPlugin::new(),
                    ))
                    .await;
                registry_ref
                    .register(Box::new(plugins::web_search::WebSearchPlugin::new(
                        config_for_plugins,
                        handle_for_plugins.clone(),
                    )))
                    .await;
                registry_ref
                    .register(Box::new(
                        plugins::clipboard_history::ClipboardHistoryPlugin::new(
                            handle_for_plugins.clone(),
                        ),
                    ))
                    .await;
                registry_ref
                    .register(Box::new(plugins::workflows::WorkflowsPlugin::new(
                        handle_for_plugins.clone(),
                    )))
                    .await;
                registry_ref
                    .register(Box::new(plugins::file_search::FileSearchPlugin::new(
                        handle_for_plugins.clone(),
                    )))
                    .await;
                registry_ref
                    .register(Box::new(
                        plugins::window_switcher::WindowSwitcherPlugin::new(),
                    ))
                    .await;
                registry_ref
                    .register(Box::new(
                        plugins::process_monitor::ProcessMonitorPlugin::new(handle_for_plugins),
                    ))
                    .await;
            });

            // Register global shortcut from config
            let hotkey_ok = hotkey::register_global_shortcut(&handle, &app_config.hotkey);

            // Double-tap activation (default: Ctrl-Ctrl), Listary/CLaunch style
            if !app_config.hotkey.double_tap.is_empty() {
                let installed = double_tap::install(
                    &handle,
                    &app_config.hotkey.double_tap,
                    app_config.hotkey.double_tap_interval_ms,
                );
                if !installed {
                    eprintln!(
                        "Unknown double_tap key '{}'; use Ctrl / Alt / Shift / Win",
                        app_config.hotkey.double_tap
                    );
                }
            }

            // Setup window behavior from config
            window::setup_main_window(&handle, &app_config.window)?;

            // Setup system tray
            let show_item = MenuItem::with_id(app, "show", "Conduit を開く", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "終了", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_item, &quit_item])?;

            let tray_icon = Image::from_path("icons/icon.png")
                .or_else(|_| Image::from_path("icons/32x32.png"))
                .unwrap_or_else(|_| Image::from_bytes(include_bytes!("../icons/32x32.png")).expect("built-in icon"));

            let tray_handle = handle.clone();
            TrayIconBuilder::new()
                .icon(tray_icon)
                .tooltip("Conduit - ランチャー")
                .menu(&menu)
                .on_menu_event(move |app, event| match event.id.as_ref() {
                    "show" => show_launcher(app),
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let tauri::tray::TrayIconEvent::Click { button, .. } = event {
                        if button == tauri::tray::MouseButton::Left {
                            show_launcher(tray.app_handle());
                        }
                    }
                })
                .build(&tray_handle)?;

            // Show window on first launch so user knows the app is running
            show_launcher(&handle);

            if !hotkey_ok {
                eprintln!(
                    "Hotkey registration failed. Use the system tray icon to open Conduit."
                );
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::search::query,
            commands::search::browse_plugin,
            commands::execute::execute_action,
            commands::config::get_config,
            commands::config::save_config,
            commands::config::get_config_path,
            commands::config::open_config_file,
            commands::config::restart_app,
            commands::workflow::import_workflow,
            commands::workflow::list_workflows,
            commands::pin::set_pinned,
            commands::pins::list_pins,
            commands::pins::add_pin,
            commands::pins::remove_pin,
            commands::pins::move_pin,
        ])
        .run(tauri::generate_context!())
        .expect("error while running conduit");
}
