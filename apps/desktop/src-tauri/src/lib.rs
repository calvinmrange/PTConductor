use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AppHealth {
    name: &'static str,
    version: &'static str,
    status: &'static str,
}

#[tauri::command]
fn health() -> AppHealth {
    AppHealth {
        name: "PTConductor",
        version: env!("CARGO_PKG_VERSION"),
        status: "engine ready",
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![health])
        .run(tauri::generate_context!())
        .expect("failed to run PTConductor");
}

