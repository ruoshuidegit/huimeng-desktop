use serde::{Deserialize, Serialize};
use tauri::webview::{DownloadEvent, WebviewBuilder};
use tauri::window::WindowBuilder;
use tauri::{
    LogicalPosition, LogicalSize, Manager, PhysicalSize, Rect, Webview, WebviewUrl, Window,
};

use crate::proxy::ProxySettings;
use crate::{app_locale, archive_destination, archive_dir, is_local_settings_url, proxy_error};

#[cfg_attr(not(windows), allow(dead_code))]
#[path = "windows_menu_theme.rs"]
mod windows_menu_theme;

pub const CHROME_WEBVIEW: &str = "desktop-chrome";
const CLOUD_WEBVIEW: &str = "cloud";
const CHROME_HEIGHT: f64 = 44.0;

#[derive(Debug, PartialEq)]
struct Layout {
    width: f64,
    chrome_height: f64,
    cloud_height: f64,
}

fn layout(size: PhysicalSize<u32>, scale_factor: f64) -> Layout {
    let size = size.to_logical::<f64>(scale_factor);
    let chrome_height = size.height.min(CHROME_HEIGHT);
    Layout {
        width: size.width,
        chrome_height,
        cloud_height: (size.height - chrome_height).max(0.0),
    }
}

fn update_layout(window: &Window, size: PhysicalSize<u32>, scale: f64) -> tauri::Result<()> {
    let bounds = layout(size, scale);
    if let Some(chrome) = window.app_handle().get_webview(CHROME_WEBVIEW) {
        chrome.set_bounds(Rect {
            position: LogicalPosition::new(0.0, 0.0).into(),
            size: LogicalSize::new(bounds.width, bounds.chrome_height).into(),
        })?;
    }
    if let Some(cloud) = window.app_handle().get_webview(CLOUD_WEBVIEW) {
        cloud.set_bounds(Rect {
            position: LogicalPosition::new(0.0, bounds.chrome_height).into(),
            size: LogicalSize::new(bounds.width, bounds.cloud_height).into(),
        })?;
    }
    Ok(())
}

// 保持云端业务与本地窗口操作隔离，云端页面不需要任何新增本机权限。
pub fn build_window(app: &tauri::AppHandle, settings: &ProxySettings) -> Result<(), String> {
    let public_error =
        || {
            app_locale(app).text(
        "无法打开慧梦，请检查网络代理配置后重启应用。",
        "Unable to open Huimeng. Check your network proxy settings and restart the app.",
    ).to_string()
        };
    let proxy_url = settings
        .proxy_url()
        .map_err(|message| proxy_error(app_locale(app), message))?;
    let window = WindowBuilder::new(app, "main")
        .title(app_locale(app).text("慧梦桌面端", "Huimeng"))
        .inner_size(1440.0, 900.0)
        .min_inner_size(900.0, 600.0)
        .theme(Some(tauri::Theme::Dark))
        .decorations(false)
        .shadow(true)
        .visible(false)
        .build()
        .map_err(|error| {
            eprintln!("[desktop] 创建桌面窗口失败：{error}");
            public_error()
        })?;

    let result = (|| -> tauri::Result<()> {
        // 隐藏菜单排，但保留原生菜单动作和快捷键注册。
        window.hide_menu()?;
        let size = window.inner_size()?;
        let bounds = layout(size, window.scale_factor()?);
        let chrome_builder = WebviewBuilder::new(
            CHROME_WEBVIEW,
            WebviewUrl::App("index.html?view=chrome".into()),
        )
        .on_navigation(is_local_settings_url)
        // WebView2 同一数据目录不能使用不同代理参数；业务页保留原来的登录目录。
        .data_directory(
            app.path()
                .app_local_data_dir()?
                .join("desktop-chrome-webview"),
        );
        window.add_child(
            chrome_builder,
            LogicalPosition::new(0.0, 0.0),
            LogicalSize::new(bounds.width, bounds.chrome_height),
        )?;

        let mut cloud_builder = WebviewBuilder::new(
            CLOUD_WEBVIEW,
            WebviewUrl::External(crate::CLOUD_URL.parse().expect("valid cloud url")),
        );
        if let Some(url) = proxy_url {
            cloud_builder = cloud_builder.proxy_url(url);
        }
        let directory = archive_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
        let cloud = window.add_child(
            cloud_builder.on_download(move |_webview, event| match event {
                DownloadEvent::Requested { url, destination } => {
                    *destination = archive_destination(url.as_str(), &directory);
                    true
                }
                DownloadEvent::Finished { path, success, .. } => {
                    if success {
                        if let Some(path) = path {
                            println!("[archive] saved: {}", path.display());
                        }
                    }
                    true
                }
                _ => true,
            }),
            LogicalPosition::new(0.0, bounds.chrome_height),
            LogicalSize::new(bounds.width, bounds.cloud_height),
        )?;
        let resize_window = window.clone();
        window.on_window_event(move |event| {
            let result = match event {
                tauri::WindowEvent::Resized(size) => resize_window
                    .scale_factor()
                    .and_then(|scale| update_layout(&resize_window, *size, scale)),
                tauri::WindowEvent::ScaleFactorChanged {
                    scale_factor,
                    new_inner_size,
                    ..
                } => update_layout(&resize_window, *new_inner_size, *scale_factor),
                _ => Ok(()),
            };
            if let Err(error) = result {
                eprintln!("[desktop] 更新桌面布局失败：{error}");
            }
        });
        window.show()?;
        cloud.set_focus()?;
        Ok(())
    })();
    result.map_err(|error| {
        eprintln!("[desktop] 创建桌面内容失败：{error}");
        if let Err(error) = window.destroy() {
            eprintln!("[desktop] 清理未完成的窗口失败：{error}");
        }
        public_error()
    })
}

fn is_chrome_origin(window: &str, webview: &str, url: &tauri::Url) -> bool {
    window == "main" && webview == CHROME_WEBVIEW && is_local_settings_url(url)
}

fn require_chrome(webview: &Webview) -> Result<Window, String> {
    let window = webview.window();
    if webview
        .url()
        .is_ok_and(|url| is_chrome_origin(window.label(), webview.label(), &url))
    {
        Ok(window)
    } else {
        Err(app_locale(webview.app_handle())
            .text(
                "请从桌面端顶部操作窗口。",
                "Use the desktop title bar to control this window.",
            )
            .to_string())
    }
}

#[derive(Serialize)]
pub struct ChromeState {
    locale: &'static str,
    maximized: bool,
    fullscreen: bool,
    focused: bool,
}

fn action_error(window: &Window, error: tauri::Error) -> String {
    eprintln!("[desktop] 桌面窗口操作失败：{error}");
    app_locale(window.app_handle())
        .text(
            "窗口操作暂时失败，请重试。",
            "Unable to control the window right now. Please try again.",
        )
        .to_string()
}

#[tauri::command]
pub fn get_chrome_state(webview: Webview) -> Result<ChromeState, String> {
    let window = require_chrome(&webview)?;
    let state = (|| -> tauri::Result<ChromeState> {
        Ok(ChromeState {
            locale: app_locale(window.app_handle()).code(),
            maximized: window.is_maximized()?,
            fullscreen: window.is_fullscreen()?,
            focused: window.is_focused()?,
        })
    })();
    state.map_err(|error| action_error(&window, error))
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WindowAction {
    Minimize,
    ToggleMaximize,
    Close,
    StartDragging,
    Menu,
}

#[tauri::command]
pub async fn desktop_window_action(webview: Webview, action: WindowAction) -> Result<(), String> {
    let window = require_chrome(&webview)?;
    let result = match action {
        WindowAction::Minimize => window.minimize(),
        WindowAction::ToggleMaximize => window.is_maximized().and_then(|maximized| {
            if maximized {
                window.unmaximize()
            } else {
                window.maximize()
            }
        }),
        WindowAction::Close => window.close(),
        WindowAction::StartDragging => window.start_dragging(),
        WindowAction::Menu => {
            // 原生编辑动作需要操作最后正在编辑的业务页。
            if let Some(cloud) = window.app_handle().get_webview(CLOUD_WEBVIEW) {
                cloud
                    .set_focus()
                    .map_err(|error| action_error(&window, error))?;
            }
            let menu = window.menu().ok_or_else(|| {
                app_locale(window.app_handle())
                    .text(
                        "暂时无法打开菜单，请重启应用后重试。",
                        "Unable to open the menu. Restart the app and try again.",
                    )
                    .to_string()
            })?;
            popup_menu(&window, menu).await
        }
    };
    result.map_err(|error| action_error(&window, error))
}

async fn popup_menu(window: &Window, menu: tauri::menu::Menu<tauri::Wry>) -> tauri::Result<()> {
    let (sender, mut receiver) = tauri::async_runtime::channel(1);
    let popup_window = window.clone();
    window.run_on_main_thread(move || {
        // TrackPopupMenu 在主线程同步运行；临时主题须覆盖根菜单及所有二级菜单，
        // 并在菜单关闭后恢复，不能在派发主线程任务之前就释放 guard。
        let result = {
            #[cfg(windows)]
            let _theme_guard = windows_menu_theme::DarkPopupGuard::new();
            popup_window.popup_menu_at(&menu, LogicalPosition::new(116.0, CHROME_HEIGHT))
        };
        if sender.try_send(result).is_err() {
            eprintln!("[desktop] 菜单操作结果接收端已关闭");
        }
    })?;
    receiver
        .recv()
        .await
        .ok_or(tauri::Error::FailedToReceiveMessage)?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_keeps_title_bar_fixed_across_size_and_dpi_changes() {
        for (width, height, scale) in [(1440, 900, 1.0), (1800, 1200, 2.0), (2160, 1350, 1.5)] {
            let bounds = layout(PhysicalSize::new(width, height), scale);
            assert_eq!(bounds.chrome_height, 44.0);
            assert_eq!(bounds.width, width as f64 / scale);
            assert_eq!(
                bounds.cloud_height + bounds.chrome_height,
                height as f64 / scale
            );
        }
        let minimized = layout(PhysicalSize::new(0, 0), 1.0);
        assert_eq!(
            minimized,
            Layout {
                width: 0.0,
                chrome_height: 0.0,
                cloud_height: 0.0
            }
        );
    }

    #[test]
    fn only_local_chrome_in_main_can_control_windows() {
        for url in [
            "tauri://localhost/index.html?view=chrome",
            "http://tauri.localhost/index.html?view=chrome",
        ] {
            assert!(is_chrome_origin(
                "main",
                CHROME_WEBVIEW,
                &url.parse().unwrap()
            ));
        }
        for (window, webview, url) in [
            ("main", "cloud", "https://tapxflow.com/"),
            ("main", CHROME_WEBVIEW, "https://tapxflow.com/"),
            (
                "proxy-settings",
                CHROME_WEBVIEW,
                "tauri://localhost/index.html",
            ),
            ("main", "cloud", "tauri://localhost/index.html"),
            (
                "main",
                CHROME_WEBVIEW,
                "https://tauri.localhost.attacker.example/",
            ),
        ] {
            assert!(!is_chrome_origin(window, webview, &url.parse().unwrap()));
        }
    }

    #[test]
    fn chrome_permissions_do_not_apply_to_cloud_or_entire_main_window() {
        let capability: serde_json::Value =
            serde_json::from_str(include_str!("../capabilities/desktop-chrome.json")).unwrap();
        assert!(capability.get("windows").is_none());
        assert!(capability.get("remote").is_none());
        assert_eq!(capability["webviews"], serde_json::json!([CHROME_WEBVIEW]));
        let defaults: serde_json::Value =
            serde_json::from_str(include_str!("../capabilities/default.json")).unwrap();
        assert!(defaults.get("windows").is_none());
        assert_eq!(defaults["webviews"], serde_json::json!(["main"]));
    }
}
