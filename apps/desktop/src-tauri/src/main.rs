#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use meshclip_core::web::{WebBridge, WebInvitation};
use meshclip_core::{
    model::{Config, Status, Ticket},
    Service,
};
use std::sync::{Arc, Mutex};
#[derive(Default)]
struct MobileWeb(Mutex<Option<Arc<WebBridge>>>);
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
#[tauri::command]
async fn start_mobile(
    s: State<'_, Backend>,
    web: State<'_, MobileWeb>,
    address: String,
) -> Result<WebInvitation, String> {
    if web.inner().0.lock().unwrap().is_some() {
        return Err("请先关闭已有网页共享".into());
    }
    let bridge = WebBridge::start(s.inner().clone(), &address)
        .await
        .map_err(|e| e.to_string())?;
    let invite = bridge.invitation().map_err(|e| e.to_string())?;
    let mut current = web.inner().0.lock().unwrap();
    if current.is_some() {
        bridge.stop();
        return Err("网页共享已启动".into());
    }
    *current = Some(bridge);
    Ok(invite)
}
#[tauri::command]
fn mobile_invitation(web: State<MobileWeb>) -> Result<WebInvitation, String> {
    web.inner()
        .0
        .lock()
        .unwrap()
        .as_ref()
        .ok_or("请先开启网页共享")?
        .invitation()
        .map_err(|e| e.to_string())
}
#[tauri::command]
fn stop_mobile(web: State<MobileWeb>) {
    if let Some(b) = web.inner().0.lock().unwrap().take() {
        b.stop();
    }
}
#[tauri::command]
async fn mobile_share(web: State<'_, MobileWeb>, path: Option<String>) -> Result<(), String> {
    let bridge = web
        .inner()
        .0
        .lock()
        .unwrap()
        .clone()
        .ok_or("请先开启网页共享")?;
    tokio::task::spawn_blocking(move || {
        if let Some(path) = path {
            bridge.share_file(path.into()).map(|_| ())
        } else {
            bridge.share_clipboard()
        }
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
}
#[tauri::command]
async fn network_status(
    runtime: State<'_, tokio::sync::Mutex<meshclip_core::network::NetworkRuntime>>,
) -> Result<meshclip_core::network::NetworkStatus, String> {
    Ok(runtime.lock().await.status())
}
#[tauri::command]
async fn network_start(
    app: tauri::AppHandle,
    s: State<'_, Backend>,
    runtime: State<'_, tokio::sync::Mutex<meshclip_core::network::NetworkRuntime>>,
    options: meshclip_core::network::StartOptions,
) -> Result<(), String> {
    let kernel = app
        .path()
        .resource_dir()
        .map_err(|e| e.to_string())?
        .join("easytier/easytier-core.exe");
    runtime
        .lock()
        .await
        .start(s.inner().clone(), options, &kernel)
        .await
        .map_err(|e| e.to_string())
}
#[tauri::command]
async fn network_stop(
    s: State<'_, Backend>,
    web: State<'_, MobileWeb>,
    runtime: State<'_, tokio::sync::Mutex<meshclip_core::network::NetworkRuntime>>,
) -> Result<(), String> {
    if let Some(b) = web.inner().0.lock().unwrap().take() {
        b.stop();
    }
    runtime
        .lock()
        .await
        .stop(s.inner())
        .await
        .map_err(|e| e.to_string())
}
fn main() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();
    tauri::Builder::default()
        .manage(tokio::sync::Mutex::new(
            meshclip_core::network::NetworkRuntime::default(),
        ))
        .manage(MobileWeb::default())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            let service = Service::open(&dir)?;
            app.manage(service.clone());
            tauri::async_runtime::spawn(async move {
                service.start_clipboard();
                service.start_monitor();
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
                    "exit" => {
                        let app=app.clone();
                        tauri::async_runtime::spawn(async move {
                            let s=app.state::<Backend>();
                            let runtime=app.state::<tokio::sync::Mutex<meshclip_core::network::NetworkRuntime>>();
                            let _=runtime.lock().await.stop(s.inner()).await;
                            app.exit(0);
                        });
                    },
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
            network_status,
            network_start,
            network_stop,
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
            copy_text,
            start_mobile,
            stop_mobile,
            mobile_invitation,
            mobile_share
        ])
        .run(tauri::generate_context!())
        .expect("MeshClip desktop failed");
}
