#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod cleanup;
mod model;
mod native;
mod permissions;
mod scan;

use model::*;
use serde::Serialize;
use std::sync::atomic::Ordering;
use tauri::{ipc::Channel, AppHandle, Manager, State};

#[derive(Serialize)]
struct Bootstrap {
    settings: Settings,
    history: Vec<Record>,
}

#[tauri::command]
fn bootstrap(state: State<AppState>) -> Result<Bootstrap, String> {
    let _guard = state.start()?;
    let mut store = state.store.lock().map_err(|e| e.to_string())?;
    cleanup::reconcile(&mut store.saved.history, &state.home);
    state.save(&store.saved)?;
    Ok(Bootstrap {
        settings: store.saved.settings.clone(),
        history: store.saved.history.clone(),
    })
}

#[tauri::command]
async fn scan(app: AppHandle, on_event: Channel<scan::ScanEvent>) -> Result<scan::Summary, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let _guard = state.start()?;
        state.cancel.store(false, Ordering::SeqCst);
        scan::run(&state, |event| {
            let _ = on_event.send(event);
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn cancel_scan(state: State<AppState>) {
    state.cancel.store(true, Ordering::SeqCst);
}

#[tauri::command]
fn choose_root(state: State<AppState>) -> Result<Settings, String> {
    let _guard = state.start()?;
    let folder = native::choose_folder()?;
    let mut store = state.store.lock().map_err(|e| e.to_string())?;
    if let Some(folder) = folder {
        let root = validate_root(&folder, &state.home)?;
        if !store
            .saved
            .settings
            .roots
            .iter()
            .any(|r| root.starts_with(r))
        {
            store.saved.settings.roots.retain(|r| !r.starts_with(&root));
            store.saved.settings.roots.push(root);
            state.save(&store.saved)?;
        }
    }
    Ok(store.saved.settings.clone())
}

#[tauri::command]
fn remove_root(state: State<AppState>, index: usize) -> Result<Settings, String> {
    let _guard = state.start()?;
    let mut store = state.store.lock().map_err(|e| e.to_string())?;
    if index >= store.saved.settings.roots.len() {
        return Err("Folder no longer exists in settings".into());
    }
    store.saved.settings.roots.remove(index);
    store.items.clear();
    state.save(&store.saved)?;
    Ok(store.saved.settings.clone())
}

#[tauri::command]
fn set_appearance(state: State<AppState>, appearance: String) -> Result<Settings, String> {
    if !["system", "light", "dark"].contains(&appearance.as_str()) {
        return Err("Unknown appearance".into());
    }
    let mut store = state.store.lock().map_err(|e| e.to_string())?;
    store.saved.settings.appearance = appearance;
    state.save(&store.saved)?;
    Ok(store.saved.settings.clone())
}

#[tauri::command]
async fn trash_items(app: AppHandle, ids: Vec<String>) -> Result<Vec<Record>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let _guard = state.start()?;
        cleanup::trash_selected(&state, &ids)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn restore_item(app: AppHandle, id: String) -> Result<Vec<Record>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let _guard = state.start()?;
        cleanup::restore(&state, &id)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn reveal_item(state: State<AppState>, id: String) -> Result<(), String> {
    let store = state.store.lock().map_err(|e| e.to_string())?;
    if let Some(item) = store.items.get(&id) {
        return native::reveal(&item.path);
    }
    if let Some(record) = store.saved.history.iter().find(|r| r.id == id) {
        let path = if record.status == "restored" {
            &record.original
        } else {
            record
                .trash_path
                .as_ref()
                .ok_or("No Trash location is recorded")?
        };
        return native::reveal(path);
    }
    Err("Item is no longer available; scan again".into())
}

#[tauri::command]
fn open_trash() -> Result<(), String> {
    native::open_trash()
}

#[tauri::command]
fn open_update() -> Result<(), String> {
    native::open_update()
}

#[tauri::command]
fn open_privacy_settings(full_disk_access: bool) -> Result<(), String> {
    native::open_privacy_settings(full_disk_access)
}

#[tauri::command]
async fn check_permissions(app: AppHandle) -> Result<Vec<permissions::AccessCheck>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let _guard = state.start()?;
        let roots = state
            .store
            .lock()
            .map_err(|e| e.to_string())?
            .saved
            .settings
            .roots
            .clone();
        Ok(permissions::check_access(&state.home, &roots))
    })
    .await
    .map_err(|e| e.to_string())?
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let app = tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            bootstrap,
            scan,
            cancel_scan,
            choose_root,
            remove_root,
            set_appearance,
            trash_items,
            restore_item,
            reveal_item,
            open_trash,
            open_update,
            open_privacy_settings,
            check_permissions
        ])
        .build(tauri::generate_context!())?;
    let state = AppState::new(app.path().home_dir()?, app.path().app_data_dir()?)
        .map_err(std::io::Error::other)?;
    app.manage(state);
    app.run(|_, _| {});
    Ok(())
}
