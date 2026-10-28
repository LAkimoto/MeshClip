#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use meshclip_core::{
    model::{Config, Status, Ticket},
    Service,
};
use std::sync::Arc;
use tauri::{Manager, State};
type Backend = Arc<Service>;
#[tauri::command]
fn snapshot(s: State<Backend>) -> Status {
    s.snapshot()
}
#[tauri::command]
fn config(s: State<Backend>) -> Config {
    s.config.lock().unwrap().clone()
}
#[tauri::command]
fn save_config(s: State<Backend>, config: Config) -> Result<(), String> {
    s.configure(config).map_err(|e| e.to_string())
}
#[tauri::command]
fn invitation(s: State<Backend>) -> Ticket {
    s.ticket()
}
#[tauri::command]
async fn pair(s: State<'_, Backend>, ticket: Ticket) -> Result<(), String> {
    s.pair(ticket).await.map_err(|e| e.to_string())
}
#[tauri::command]
fn revoke(s: State<Backend>, id: String) -> Result<(), String> {
    s.revoke(&id).map_err(|e| e.to_string())
}
#[tauri::command]
fn decide(s: State<Backend>, id: String, accept: bool) -> Result<(), String> {
    s.decide(&id, accept).map_err(|e| e.to_string())
}
#[tauri::command]
async fn send_file(s: State<'_, Backend>, peer: String, path: String) -> Result<String, String> {
    let backend = s.inner().clone();
    tokio::task::spawn_blocking(move || {
        let path = std::path::Path::new(&path);
        if path.is_dir() {
            backend.queue_folder(&peer, path)
        } else {
            backend.queue_file(&peer, path)
        }
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
}
#[tauri::command]
fn retry(s: State<Backend>, id: String) -> Result<(), String> {
    s.inner().retry(&id).map_err(|e| e.to_string())
}
#[tauri::command]
fn cancel(s: State<Backend>, id: String) -> Result<(), String> {
    s.cancel(&id).map_err(|e| e.to_string())
}
#[tauri::command]
fn clear_history(s: State<Backend>) {
    s.clear_history()
}
#[tauri::command]
fn copy_text(s: State<Backend>, text: String) -> Result<(), String> {
    s.copy_local(text).map_err(|e| e.to_string())
}
fn main() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            let service = Service::open(&dir)?;
            app.manage(service.clone());
            tauri::async_runtime::spawn(async move {
                service.start_clipboard();
                service.start_monitor();
                if let Err(e) = service.clone().listen().await {
                    service.status.lock().unwrap().error = Some(e.to_string());
                }
            });
            let show =
                tauri::menu::MenuItem::with_id(app, "show", "打开 MeshClip", true, None::<&str>)?;
            let exit = tauri::menu::MenuItem::with_id(app, "exit", "退出", true, None::<&str>)?;
            let menu = tauri::menu::Menu::with_items(app, &[&show, &exit])?;
            // Generated RGBA icon avoids binary assets in source.
            let rgba = [40u8, 194, 163, 255].repeat(32 * 32);
            tauri::tray::TrayIconBuilder::new()
                .icon(tauri::image::Image::new_owned(rgba, 32, 32))
                .tooltip("MeshClip")
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    "exit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            snapshot,
            config,
            save_config,
            invitation,
            pair,
            revoke,
            decide,
            send_file,
            retry,
            cancel,
            clear_history,
            copy_text
        ])
        .run(tauri::generate_context!())
        .expect("MeshClip desktop failed");
}
