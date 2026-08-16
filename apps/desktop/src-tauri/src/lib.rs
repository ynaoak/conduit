use tauri::{
    image::Image,
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Manager,
};

mod capture;
mod capture_history;
mod channel;
mod color;
mod color_history;
mod commands;
mod config;
mod double_tap;
mod hotkey;
mod i18n;
mod loupe;
mod ja;
mod pins;
mod plugin_system;
mod plugins;
mod screen_color;
mod window;

use config::ConfigState;
use plugin_system::registry::PluginRegistry;

use window::show_launcher;

/// The menu bar image on macOS, the app icon everywhere else. Both are
/// looked up next to the executable first so a build can be re-skinned
/// without a rebuild, with the compiled-in copy as the fallback.
#[cfg(target_os = "macos")]
const TRAY_ICON_PATH: &str = "icons/tray-template.png";
#[cfg(not(target_os = "macos"))]
const TRAY_ICON_PATH: &str = "icons/icon.png";

#[cfg(target_os = "macos")]
const TRAY_ICON_FALLBACK: &[u8] = include_bytes!("../icons/tray-template.png");
#[cfg(not(target_os = "macos"))]
const TRAY_ICON_FALLBACK: &[u8] = include_bytes!("../icons/32x32.png");

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        // Serves workflow HTML apps (and their assets) into tool windows
        .register_uri_scheme_protocol(plugins::workflows::tool_window::SCHEME, |ctx, request| {
            plugins::workflows::tool_window::serve(ctx.app_handle(), &request)
        })
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init());

    // The Store copy does not carry the updater at all. Gating only the UI
    // would leave the plugin reachable over IPC, and "the button is hidden"
    // is a weaker promise than "the capability is absent".
    let builder = if channel::self_updates() {
        builder.plugin(tauri_plugin_updater::Builder::new().build())
    } else {
        builder
    };

    builder
        .setup(|app| {
            let handle = app.handle().clone();

            // Load config from app config dir
            let config_dir = app
                .path()
                .app_config_dir()
                .expect("failed to resolve app config dir");
            let config_state = ConfigState::load(&config_dir);
            let app_config = tauri::async_runtime::block_on(config_state.get());
            // Before anything user-visible is built (tray, plugins)
            i18n::set(&app_config.language);

            app.manage(config_state.clone());
            app.manage(pins::PinsState::load(&config_dir));
            app.manage(color_history::ColorHistoryState::load(&config_dir));
            app.manage(capture_history::CaptureStore::load(&config_dir));

            // Built-in workflow packages (Dev Tools) — installed before the
            // plugins register so the first search already sees them
            plugins::workflows::builtin::install(&handle);

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
                    .register(Box::new(plugins::base64_codec::Base64Plugin::new(
                        handle_for_plugins.clone(),
                    )))
                    .await;
                registry_ref
                    .register(Box::new(plugins::json_format::JsonFormatPlugin::new(
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
                    .register(Box::new(plugins::color_picker::ColorPickerPlugin::new(
                        handle_for_plugins.clone(),
                    )))
                    .await;
                registry_ref
                    .register(Box::new(plugins::screenshot::ScreenshotPlugin::new(
                        handle_for_plugins.clone(),
                    )))
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
                    // install() returns false for two unrelated reasons, and
                    // blaming the key name on a platform that has no detector
                    // at all sends the reader off to fix a setting that was
                    // already correct.
                    if cfg!(windows) {
                        eprintln!(
                            "Unknown double_tap key '{}'; use Ctrl / Alt / Shift / Win",
                            app_config.hotkey.double_tap
                        );
                    } else {
                        eprintln!(
                            "Double-tap activation is Windows-only (no detector is built for \
                             this platform), so '{}' has no effect. Use the global hotkey \
                             instead: hotkey.modifier + hotkey.key in config.json, currently \
                             {}+{}.",
                            app_config.hotkey.double_tap,
                            app_config.hotkey.modifier,
                            app_config.hotkey.key
                        );
                    }
                }
            }

            // Setup window behavior from config
            window::setup_main_window(&handle, &app_config.window)?;

            // Setup system tray
            let show_item =
                MenuItem::with_id(app, "show", i18n::t("app", "tray_show"), true, None::<&str>)?;
            let quit_item =
                MenuItem::with_id(app, "quit", i18n::t("app", "tray_quit"), true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_item, &quit_item])?;

            // macOS recolours a template image itself — black-plus-alpha
            // artwork that it inverts for a dark menu bar and dims when the
            // bar is inactive. Handing it the colour icon instead leaves a
            // dark tile sitting in the menu bar, legible but obviously not
            // from around here. Every other platform wants the real icon.
            let tray_icon = Image::from_path(TRAY_ICON_PATH)
                .unwrap_or_else(|_| Image::from_bytes(TRAY_ICON_FALLBACK).expect("built-in icon"));

            let tray_handle = handle.clone();
            TrayIconBuilder::new()
                .icon(tray_icon)
                // No-op off macOS; there is no template concept elsewhere
                .icon_as_template(cfg!(target_os = "macos"))
                .tooltip(i18n::t("app", "tray_tooltip"))
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
            commands::color::pick_screen_color,
            commands::color::picked_colors,
            commands::capture::take_capture,
            commands::capture::capture_frame,
            commands::capture::capture_surface_ready,
            commands::capture::capture_surface_focus,
            commands::capture::capture_screens,
            commands::capture::capture_selected,
            commands::capture::stop_recording,
            commands::capture::recording_state,
            commands::capture::capture_list,
            commands::capture::copy_capture,
            commands::capture::open_capture,
            commands::capture::reveal_capture,
            commands::capture::delete_capture,
            commands::capture::clear_captures,
            commands::capture::open_captures_folder,
            commands::config::get_config,
            commands::config::save_config,
            commands::config::get_config_path,
            commands::config::resolved_language,
            commands::config::release_channel,
            commands::config::summon_hotkey,
            commands::config::open_config_file,
            commands::config::restart_app,
            commands::workflow::import_workflow,
            commands::workflow::list_workflows,
            commands::workflow::set_workflow_enabled,
            commands::workflow::delete_workflow,
            commands::workflow::export_workflow,
            commands::pin::set_pinned,
            commands::pin::hide_window,
            commands::pin::auto_hide_window,
            commands::pin::set_resolved_theme,
            commands::pins::list_pins,
            commands::pins::add_pin,
            commands::pins::remove_pin,
            commands::pins::move_pin,
        ])
        .run(tauri::generate_context!())
        .expect("error while running conduit");
}

#[cfg(test)]
mod tray_icon_tests {
    use tauri::image::Image;

    /// macOS repaints a template image from its alpha channel alone: the
    /// colours are thrown away, so a colour icon dropped in here would not
    /// look wrong on the way in — it would just render as a silhouette in
    /// the menu bar, and only on a Mac. Assert the asset is already what
    /// the OS is going to make of it.
    #[test]
    fn the_menu_bar_icon_is_a_template_image() {
        let image = Image::from_bytes(include_bytes!("../icons/tray-template.png"))
            .expect("tray-template.png decodes");

        assert_eq!(image.width(), image.height(), "the menu bar wants a square");

        let rgba = image.rgba();
        let mut opaque = 0usize;
        for (i, pixel) in rgba.chunks_exact(4).enumerate() {
            let [r, g, b, a] = [pixel[0], pixel[1], pixel[2], pixel[3]];
            if a == 0 {
                continue;
            }
            opaque += 1;
            assert!(
                r == 0 && g == 0 && b == 0,
                "pixel {i} is #{r:02x}{g:02x}{b:02x}, not black; \
                 a template image carries its shape in the alpha channel only",
            );
        }
        assert!(opaque > 0, "an all-transparent icon would be an empty menu bar");
    }
}
