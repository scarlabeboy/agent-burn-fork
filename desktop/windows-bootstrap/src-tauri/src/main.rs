#![forbid(unsafe_code)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(not(all(target_os = "windows", target_arch = "x86_64")))]
compile_error!("ABF Windows Bootstrap supports Windows x64 only");

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
    // Apply lexical checks before framework/window initialization. Application code does not
    // explicitly create directories; Tauri/WebView2 may create missing profile directories.
    let profile = runtime_directory()?;
    tauri::Builder::default()
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
