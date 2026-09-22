use std::path::PathBuf;
use std::sync::mpsc::channel;

use tauri::webview::DownloadEvent;
use tauri::{WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use tauri_plugin_updater::UpdaterExt;

const CLOUD_URL: &str = "https://tapxflow.com/?client=desktop";

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
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
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
                        _ => true,
                    })
                    .build()?;
            }

            #[cfg(not(desktop))]
            {
                let _ = builder;
            }

            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                check_for_updates(handle).await;
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

async fn check_for_updates(app: tauri::AppHandle) {
    let Ok(updater) = app.updater() else {
        return;
    };

    let update = match updater.check().await {
        Ok(Some(update)) => update,
        Ok(None) => return,
        Err(err) => {
            eprintln!("[updater] check failed: {err}");
            return;
        }
    };

    let current = update.current_version.clone();
    let latest = update.version.clone();
    let body = update.body.clone().unwrap_or_default();

    let (tx, rx) = channel();
    app.dialog()
        .message(format!(
            "发现新版本 v{latest}（当前 v{current}）。  {body}  是否立即下载并安装？"
        ))
        .title("发现新版本")
        .kind(MessageDialogKind::Info)
        .buttons(MessageDialogButtons::OkCancelCustom(
            "立即更新".to_string(),
            "稍后".to_string(),
        ))
        .show(move |answer| {
            let _ = tx.send(answer);
        });

    let Ok(confirmed) = rx.recv() else {
        return;
    };
    if !confirmed {
        return;
    }

    match update
        .download_and_install(|_chunk_length, _content_length| {}, || {})
        .await
    {
        Ok(()) => {
            app.restart();
        }
        Err(err) => {
            app.dialog()
                .message(format!("更新失败：{err}"))
                .title("更新失败")
                .kind(MessageDialogKind::Error)
                .buttons(MessageDialogButtons::Ok)
                .show(|_| {});
        }
    }
}
