#![deny(unsafe_code)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(not(all(target_os = "windows", target_arch = "x86_64")))]
compile_error!("ABF Windows Bootstrap supports Windows x64 only");

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
mod claude_copy_activity;
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
mod claude_copy_read;
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
mod fictional_claude_summary;
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
mod fictional_file_payload;
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
mod fictional_file_read;

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
#[tauri::command]
async fn abf_read_fictional_claude_file_once(
    state: tauri::State<'_, std::sync::Arc<fictional_file_read::Diagnostic>>,
) -> Result<String, &'static str> {
    let diagnostic = std::sync::Arc::clone(state.inner());
    tauri::async_runtime::spawn_blocking(move || diagnostic.run())
        .await
        .map_err(|_| "Fictional file diagnostic failed; no result.")?
}
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
#[tauri::command]
async fn abf_read_claude_copy_once(
    state: tauri::State<'_, std::sync::Arc<claude_copy_read::CopyReader>>,
    relative_path: String,
) -> Result<String, &'static str> {
    let reader = std::sync::Arc::clone(state.inner());
    tauri::async_runtime::spawn_blocking(move || reader.run(&relative_path))
        .await
        .map_err(|_| "COPY_READ_FAILED")?
}
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
#[tauri::command]
async fn abf_choose_claude_jsonl_once(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, std::sync::Arc<claude_copy_read::CopyReader>>,
) -> Result<Option<String>, &'static str> {
    let reader = std::sync::Arc::clone(state.inner());
    let owner = window.hwnd().map_err(|_| "COPY_DIALOG_FAILED")?.0 as usize;
    tauri::async_runtime::spawn_blocking(move || reader.pick(owner))
        .await
        .map_err(|_| "COPY_READ_FAILED")?
}
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
#[tauri::command]
async fn abf_choose_claude_folder_once(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, std::sync::Arc<claude_copy_read::CopyReader>>,
) -> Result<Option<usize>, &'static str> {
    let reader = std::sync::Arc::clone(state.inner());
    let owner = window.hwnd().map_err(|_| "COPY_DIALOG_FAILED")?.0 as usize;
    tauri::async_runtime::spawn_blocking(move || reader.pick_folder(owner))
        .await
        .map_err(|_| "COPY_READ_FAILED")?
}
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
#[tauri::command]
async fn abf_analyze_claude_folder_once(
    state: tauri::State<'_, std::sync::Arc<claude_copy_read::CopyReader>>,
) -> Result<String, &'static str> {
    let reader = std::sync::Arc::clone(state.inner());
    tauri::async_runtime::spawn_blocking(move || reader.analyze_folder())
        .await
        .map_err(|_| "COPY_READ_FAILED")?
}
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
#[tauri::command]
fn abf_clear_claude_folder(state: tauri::State<'_, std::sync::Arc<claude_copy_read::CopyReader>>) {
    state.clear_folder();
}
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
mod synthetic_snapshot;

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn runtime_directory() -> Result<std::path::PathBuf, std::io::Error> {
    use std::io::{Error, ErrorKind};
    use std::path::{Component, PathBuf, Prefix};

    let root = PathBuf::from(
        std::env::var_os("ABF_BOOTSTRAP_RUNTIME_ROOT").ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidInput,
                "ABF_BOOTSTRAP_RUNTIME_ROOT is required",
            )
        })?,
    );
    let mut components = root.components();
    // Lexical drive syntax only: require a component below the drive root.
    // This does not establish volume identity, freshness or filesystem isolation.
    if !root.is_absolute()
        || !matches!(components.next(), Some(Component::Prefix(prefix)) if matches!(prefix.kind(), Prefix::Disk(_)))
        || !matches!(components.next(), Some(Component::RootDir))
        || !matches!(components.next(), Some(Component::Normal(_)))
        || components.any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "ABF_BOOTSTRAP_RUNTIME_ROOT must be an absolute Windows drive path below the drive root without traversal",
        ));
    }
    Ok(root.join("webview2"))
}

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Capture the explicitly configured root before framework/window initialization.
    // The manual diagnostic alone creates its fixed child; WebView2 owns its profile.
    let profile = runtime_directory()?;
    // Optional, separate read-only copy root. Capture only; no startup filesystem read.
    let copy_reader = std::sync::Arc::new(claude_copy_read::CopyReader::new(
        std::env::var_os("ABF_BOOTSTRAP_CLAUDE_COPY_ROOT").map(std::path::PathBuf::from),
    ));
    let diagnostic = std::sync::Arc::new(fictional_file_read::Diagnostic::new(
        profile
            .parent()
            .expect("validated profile parent")
            .to_path_buf(),
    ));
    tauri::Builder::default()
        .manage(diagnostic)
        .manage(copy_reader)
        .invoke_handler(tauri::generate_handler![
            synthetic_snapshot::abf_get_synthetic_snapshot,
            fictional_claude_summary::abf_get_fictional_claude_summary,
            abf_read_fictional_claude_file_once,
            abf_read_claude_copy_once,
            abf_choose_claude_jsonl_once,
            abf_choose_claude_folder_once,
            abf_analyze_claude_folder_once,
            abf_clear_claude_folder
        ])
        .setup(move |app| {
            tauri::WebviewWindowBuilder::new(
                app,
                "main",
                tauri::WebviewUrl::App("index.html".into()),
            )
            .title("ABF Windows Bootstrap")
            .inner_size(960.0, 640.0)
            .data_directory(profile.clone())
            .devtools(false)
            .disable_drag_drop_handler()
            .on_navigation(|url| {
                url.scheme() == "http"
                    && url.host_str() == Some("tauri.localhost")
                    && url.port().is_none()
                    && url.username().is_empty()
                    && url.password().is_none()
            })
            .on_new_window(|_, _| tauri::webview::NewWindowResponse::Deny)
            .build()?;
            Ok(())
        })
        .run(tauri::generate_context!())?;
    Ok(())
}
