use std::path::PathBuf;

use tauri::webview::DownloadEvent;
use tauri::{WebviewUrl, WebviewWindowBuilder};

const CLOUD_URL: &str = "https://tapxflow.com";

fn archive_dir() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    let dir = PathBuf::from(home).join("Movies").join("Huimeng");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

fn sanitize_filename(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "download".to_string()
    } else {
        cleaned
    }
}

fn archive_destination(url: &str, base: &PathBuf) -> PathBuf {
    let raw_name = url.rsplit('/').next().unwrap_or("");
    let raw_name = raw_name.split('?').next().unwrap_or("");
    let name = sanitize_filename(raw_name);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    base.join(format!("{}_{}", stamp, name))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let builder = WebviewWindowBuilder::new(
                app,
                "main",
                WebviewUrl::External(CLOUD_URL.parse().expect("valid cloud url")),
            )
            .title("慧梦桌面端")
            .inner_size(1440.0, 900.0);

            #[cfg(desktop)]
            {
                let dir = archive_dir().unwrap_or_else(|| PathBuf::from("."));
                builder
                    .on_download(move |_webview, event| match event {
                        DownloadEvent::Requested { url, destination } => {
                            *destination = archive_destination(url.as_str(), &dir);
                            true
                        }
                        DownloadEvent::Finished {
                            url: _,
                            path,
                            success,
                        } => {
                            if success {
                                if let Some(p) = path {
                                    println!("[archive] saved: {}", p.display());
                                }
                            }
                            true
                        }
                        _ => true
                    })
                    .build()?;
            }

            #[cfg(not(desktop))]
            {
                let _ = builder;
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}