use std::path::PathBuf;
use std::sync::Mutex;

use tauri::webview::DownloadEvent;
use tauri::{Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};

const CLOUD_URL: &str = "https://tapxflow.com/?client=desktop";
const PROXY_SETTINGS_WINDOW: &str = "proxy-settings";

mod locale;
mod menu;
mod proxy;
mod updater;
use locale::DesktopLocale;
use proxy::ProxySettings;

#[derive(Default)]
struct ProxySettingsLock(Mutex<()>);

fn app_locale(app: &tauri::AppHandle) -> DesktopLocale {
    *app.state::<DesktopLocale>()
}

fn proxy_error(locale: DesktopLocale, message: String) -> String {
    if locale.code() == "zh-CN" {
        return message;
    }
    match message.as_str() {
        "请输入代理主机名或 IP 地址，不要包含协议、端口、路径或账号密码。" =>
            "Enter a proxy hostname or IP address without a scheme, port, path or credentials.",
        "代理端口必须在 1 到 65535 之间。" => "The proxy port must be between 1 and 65535.",
        "当前版本不支持 HTTP 代理的 80 端口，请改用其他 HTTP 代理端口或 SOCKS5。" =>
            "HTTP proxy port 80 is not supported in this version. Use another HTTP proxy port or SOCKS5.",
        "当前 macOS 客户端暂不支持 IPv6 代理地址，请改用 IPv4 地址或主机名。" =>
            "IPv6 proxy addresses are not supported on macOS yet. Use an IPv4 address or hostname.",
        "代理配置保存失败，请检查本地配置目录的写入权限后重试。" =>
            "Unable to save the proxy settings. Check write permissions for the local settings folder and try again.",
        "代理配置读取失败，请在“网络代理”中重新保存配置。" =>
            "Unable to read the proxy settings. Open Network Proxy and save your settings again.",
        "代理配置已损坏，请在“网络代理”中重新保存配置。" =>
            "The proxy settings file is invalid. Open Network Proxy and save your settings again.",
        _ => "Unable to use the proxy settings. Open Network Proxy and save your settings again.",
    }
    .to_string()
}

fn proxy_settings_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|dir| dir.join("proxy.json"))
        .map_err(|_| {
            app_locale(app)
                .text(
                    "无法读取本地配置目录，请重启应用后重试。",
                    "Unable to access the local settings folder. Restart the app and try again.",
                )
                .to_string()
        })
}

fn load_proxy_settings(app: &tauri::AppHandle) -> Result<ProxySettings, String> {
    proxy::load(&proxy_settings_path(app)?).map_err(|message| proxy_error(app_locale(app), message))
}

fn is_local_settings_url(url: &tauri::Url) -> bool {
    let bundled = (url.scheme() == "tauri" && url.host_str() == Some("localhost"))
        || (matches!(url.scheme(), "http" | "https") && url.host_str() == Some("tauri.localhost"));
    #[cfg(debug_assertions)]
    let development =
        url.scheme() == "http" && url.host_str() == Some("localhost") && url.port() == Some(1420);
    #[cfg(not(debug_assertions))]
    let development = false;
    bundled || development
}

// 远程业务页面不能读写本机代理配置。
fn require_settings_window(window: &WebviewWindow) -> Result<(), String> {
    if window.label() == PROXY_SETTINGS_WINDOW
        && window.url().is_ok_and(|url| is_local_settings_url(&url))
    {
        Ok(())
    } else {
        Err(app_locale(window.app_handle())
            .text(
                "请从桌面端的“设置 → 网络代理”打开配置窗口。",
                "Open Settings → Network Proxy from the desktop app menu.",
            )
            .to_string())
    }
}

#[tauri::command]
fn get_desktop_locale(
    app: tauri::AppHandle,
    window: WebviewWindow,
) -> Result<&'static str, String> {
    require_settings_window(&window)?;
    Ok(app_locale(&app).code())
}

#[tauri::command]
fn get_proxy_settings(
    app: tauri::AppHandle,
    window: WebviewWindow,
) -> Result<ProxySettings, String> {
    require_settings_window(&window)?;
    load_proxy_settings(&app)
}

#[tauri::command]
fn save_proxy_settings(
    app: tauri::AppHandle,
    window: WebviewWindow,
    state: tauri::State<'_, ProxySettingsLock>,
    settings: ProxySettings,
) -> Result<ProxySettings, String> {
    require_settings_window(&window)?;
    let _guard = state.0.lock().map_err(|_| {
        app_locale(&app)
            .text(
                "代理配置暂时无法保存，请重启应用后重试。",
                "Unable to save the proxy settings right now. Restart the app and try again.",
            )
            .to_string()
    })?;
    proxy::save(&proxy_settings_path(&app)?, settings)
        .map_err(|message| proxy_error(app_locale(&app), message))
}

fn show_proxy_settings(app: &tauri::AppHandle) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window(PROXY_SETTINGS_WINDOW) {
        window.show()?;
        window.unminimize()?;
        return window.set_focus();
    }
    let builder = WebviewWindowBuilder::new(
        app,
        PROXY_SETTINGS_WINDOW,
        WebviewUrl::App("index.html".into()),
    )
    .title(app_locale(app).text("网络代理 · 慧梦", "Network Proxy · Huimeng"))
    .inner_size(540.0, 680.0)
    .min_inner_size(400.0, 500.0)
    .on_navigation(is_local_settings_url);
    // WebView2 不允许同一数据目录的窗口使用不同代理参数。
    #[cfg(target_os = "windows")]
    let builder = builder.data_directory(
        app.path()
            .app_local_data_dir()?
            .join("proxy-settings-webview"),
    );
    builder.build()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::is_local_settings_url;

    #[test]
    fn settings_navigation_only_allows_local_app_origins() {
        for url in [
            "tauri://localhost/index.html",
            "http://tauri.localhost/index.html",
        ] {
            assert!(is_local_settings_url(&url.parse().unwrap()));
        }
        for url in [
            "https://tapxflow.com/",
            "https://tauri.localhost.attacker.example/",
            "tauri://attacker.example/",
            "file:///tmp/settings.html",
        ] {
            assert!(!is_local_settings_url(&url.parse().unwrap()));
        }
    }
}

fn build_main_window(app: &tauri::AppHandle, settings: &ProxySettings) -> Result<(), String> {
    let mut builder = WebviewWindowBuilder::new(
        app,
        "main",
        WebviewUrl::External(CLOUD_URL.parse().expect("valid cloud url")),
    )
    .title(app_locale(app).text("慧梦桌面端", "Huimeng"))
    .inner_size(1440.0, 900.0);
    if let Some(url) = settings
        .proxy_url()
        .map_err(|message| proxy_error(app_locale(app), message))?
    {
        builder = builder.proxy_url(url);
    }
    let dir = archive_dir().unwrap_or_else(|| PathBuf::from("."));
    builder
        .on_download(move |_webview, event| match event {
            DownloadEvent::Requested { url, destination } => {
                *destination = archive_destination(url.as_str(), &dir);
                true
            }
            DownloadEvent::Finished { path, success, .. } => {
                if success {
                    if let Some(p) = path {
                        println!("[archive] saved: {}", p.display());
                    }
                }
                true
            }
            _ => true,
        })
        .build()
        .map_err(|err| {
            eprintln!("[desktop] 创建主窗口失败：{err}");
            app_locale(app).text(
                "无法打开慧梦，请检查网络代理配置后重启应用。",
                "Unable to open Huimeng. Check your network proxy settings and restart the app.",
            ).to_string()
        })?;
    Ok(())
}

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
    let locale = DesktopLocale::detect();
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(locale)
        .manage(ProxySettingsLock::default())
        .manage(updater::UpdaterState::default())
        .invoke_handler(tauri::generate_handler![
            get_desktop_locale,
            get_proxy_settings,
            save_proxy_settings
        ])
        .menu(move |app| menu::build(app, locale))
        .on_menu_event(|app, event| {
            if event.id().as_ref() == PROXY_SETTINGS_WINDOW {
                let handle = app.clone();
                // Windows 的 WebView 创建不能阻塞菜单事件线程。
                tauri::async_runtime::spawn(async move {
                    if let Err(err) = show_proxy_settings(&handle) {
                        eprintln!("[desktop] 打开代理设置失败：{err}");
                        handle
                            .dialog()
                            .message(app_locale(&handle).text(
                                "无法打开网络代理设置，请重启应用后重试。",
                                "Unable to open Network Proxy. Restart the app and try again.",
                            ))
                            .kind(MessageDialogKind::Error)
                            .show(|_| {});
                    }
                });
            } else if event.id().as_ref() == "check-for-updates" {
                let handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    let locale = app_locale(&handle);
                    let settings = handle
                        .try_state::<ProxySettings>()
                        .map(|settings| settings.inner().clone())
                        .map(Ok)
                        .unwrap_or_else(|| load_proxy_settings(&handle));
                    match settings {
                        Ok(settings) => updater::check(handle, settings, locale, true).await,
                        Err(message) => handle
                            .dialog()
                            .message(message)
                            .title(locale.text("请检查网络代理", "Check Network Proxy"))
                            .kind(MessageDialogKind::Error)
                            .show(|_| {}),
                    }
                });
            }
        })
        .setup(|app| {
            // 本地设置不依赖云端，代理配置损坏或离线时仍可恢复。
            let settings = load_proxy_settings(app.handle());
            match settings.and_then(|settings| {
                build_main_window(app.handle(), &settings)?;
                Ok(settings)
            }) {
                Ok(settings) => {
                    app.manage(settings.clone());
                    let handle = app.handle().clone();
                    let locale = app_locale(&handle);
                    tauri::async_runtime::spawn(async move {
                        updater::check(handle, settings, locale, false).await;
                    });
                }
                Err(message) => {
                    show_proxy_settings(app.handle())?;
                    app.dialog()
                        .message(message)
                        .title(
                            app_locale(app.handle()).text("请检查网络代理", "Check Network Proxy"),
                        )
                        .kind(MessageDialogKind::Error)
                        .show(|_| {});
                }
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
