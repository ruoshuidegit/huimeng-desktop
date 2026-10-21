use std::fs::OpenOptions;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tauri::Manager;
use tauri_plugin_dialog::{
    DialogExt, MessageDialogBuilder, MessageDialogButtons, MessageDialogKind,
};
use tauri_plugin_updater::{Error, UpdaterExt};

use crate::locale::DesktopLocale;
use crate::proxy::ProxySettings;

const CHECK_TIMEOUT: Duration = Duration::from_secs(30);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const READ_TIMEOUT: Duration = Duration::from_secs(30);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(300);
const MAX_LOG_BYTES: u64 = 1024 * 1024;

#[derive(Default)]
pub struct UpdaterState {
    active: AtomicBool,
}

impl UpdaterState {
    fn try_start(&self) -> Option<UpdateGuard<'_>> {
        self.active
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| UpdateGuard { state: self })
    }
}

struct UpdateGuard<'a> {
    state: &'a UpdaterState,
}

impl Drop for UpdateGuard<'_> {
    fn drop(&mut self) {
        self.state.active.store(false, Ordering::Release);
    }
}

#[derive(Clone, Copy)]
enum FailureStage {
    Check,
    Download,
    Install,
}

fn error_category(error: &Error) -> &'static str {
    match error {
        Error::Reqwest(error) if error.is_timeout() => "timeout",
        Error::Reqwest(error) if error.is_connect() => "connection",
        Error::Reqwest(_) | Error::Network(_) => "network",
        Error::Minisign(_)
        | Error::Base64(_)
        | Error::SignatureUtf8(_)
        | Error::SignedVersionMismatch { .. }
        | Error::MissingSignedVersion => "signature",
        Error::EmptyEndpoints | Error::InsecureTransportProtocol | Error::UrlParse(_) => {
            "configuration"
        }
        Error::TargetNotFound(_) | Error::TargetsNotFound(_) => "platform",
        Error::ReleaseNotFound | Error::Serialization(_) | Error::Semver(_) => "manifest",
        Error::Io(_) | Error::TempDirNotFound | Error::TempDirNotOnSameMountPoint => "filesystem",
        Error::AuthenticationFailed => "authorization",
        Error::InvalidUpdaterFormat | Error::BinaryNotFoundInArchive => "package",
        _ => "updater",
    }
}

fn failure_message(locale: DesktopLocale, stage: FailureStage, category: &str) -> &'static str {
    match stage {
        FailureStage::Check => locale.text(
            "暂时无法检查更新，请检查网络或代理设置，再从菜单中重新检查更新。",
            "Unable to check for updates right now. Check your network or proxy settings, then choose Check for Updates from the menu.",
        ),
        FailureStage::Download if category == "signature" => locale.text(
            "更新包校验失败，请稍后重新检查更新。当前版本未被替换。",
            "The update could not be verified. Check for updates again later. Your current version has not been replaced.",
        ),
        FailureStage::Download => locale.text(
            "更新下载失败，请检查网络或代理设置，再从菜单中重新检查更新。当前版本未被替换。",
            "The update could not be downloaded. Check your network or proxy settings, then check for updates again. Your current version has not been replaced.",
        ),
        FailureStage::Install => locale.text(
            "更新安装失败，请关闭慧梦后重试，或下载新版安装包重新安装。",
            "The update could not be installed. Quit Huimeng and try again, or download and install the new version manually.",
        ),
    }
}

fn message_dialog(app: &tauri::AppHandle, message: &str) -> MessageDialogBuilder<tauri::Wry> {
    let dialog = app.dialog().message(message);
    // macOS 无父窗口时会走全局用户通知，显式绑定窗口以使用可见的应用内提示。
    match app
        .get_webview_window("main")
        .or_else(|| app.get_webview_window("proxy-settings"))
    {
        Some(window) => dialog.parent(&window),
        None => dialog,
    }
}

fn show_message(
    app: &tauri::AppHandle,
    locale: DesktopLocale,
    title: &str,
    message: &str,
    kind: MessageDialogKind,
) {
    message_dialog(app, message)
        .title(title)
        .kind(kind)
        .buttons(MessageDialogButtons::OkCustom(
            locale.text("确定", "OK").to_string(),
        ))
        .show(|_| {});
}

// Windows 图形应用没有控制台，保留不含地址、凭据或上游原文的阶段日志用于排障。
fn record(app: &tauri::AppHandle, phase: &str, target_version: Option<&str>, reason: Option<&str>) {
    let event = serde_json::json!({
        "at_unix": SystemTime::now().duration_since(UNIX_EPOCH).map(|time| time.as_secs()).unwrap_or(0),
        "phase": phase,
        "current_version": app.package_info().version.to_string(),
        "target_version": target_version,
        "reason": reason,
    });
    eprintln!("[updater] {event}");

    let written = (|| -> std::io::Result<()> {
        let directory = app
            .path()
            .app_log_dir()
            .map_err(|_| std::io::Error::other("无法定位更新日志目录"))?;
        std::fs::create_dir_all(&directory)?;
        let path = directory.join("updater.jsonl");
        let truncate = match std::fs::metadata(&path) {
            Ok(metadata) => metadata.len() >= MAX_LOG_BYTES,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => return Err(error),
        };
        let mut options = OpenOptions::new();
        options.create(true).write(true);
        if truncate {
            options.truncate(true);
        } else {
            options.append(true);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        writeln!(options.open(path)?, "{event}")
    })();
    if written.is_err() {
        eprintln!("[updater] 更新阶段日志无法写入");
    }
}

pub async fn check(
    app: tauri::AppHandle,
    settings: ProxySettings,
    locale: DesktopLocale,
    manual: bool,
) {
    let state = app.state::<UpdaterState>();
    let Some(_guard) = state.try_start() else {
        record(&app, "already-running", None, None);
        if manual {
            show_message(
                &app,
                locale,
                locale.text("正在更新", "Update in Progress"),
                locale.text(
                    "正在检查、下载或安装更新，请稍候。",
                    "An update is being checked, downloaded or installed. Please wait.",
                ),
                MessageDialogKind::Info,
            );
        }
        return;
    };

    record(&app, "check-started", None, None);
    let mut builder = app
        .updater_builder()
        .timeout(CHECK_TIMEOUT)
        .configure_client(|client| {
            client
                .connect_timeout(CONNECT_TIMEOUT)
                .read_timeout(READ_TIMEOUT)
        });
    match settings.proxy_url() {
        Ok(Some(url)) => builder = builder.proxy(url),
        Ok(None) => {}
        Err(_) => {
            record(&app, "check-failed", None, Some("proxy-settings"));
            if manual {
                show_message(
                    &app,
                    locale,
                    locale.text("检查更新失败", "Update Check Failed"),
                    failure_message(locale, FailureStage::Check, "proxy-settings"),
                    MessageDialogKind::Error,
                );
            }
            return;
        }
    }

    let updater = match builder.build() {
        Ok(updater) => updater,
        Err(error) => {
            record(&app, "check-failed", None, Some(error_category(&error)));
            if manual {
                show_message(
                    &app,
                    locale,
                    locale.text("检查更新失败", "Update Check Failed"),
                    failure_message(locale, FailureStage::Check, error_category(&error)),
                    MessageDialogKind::Error,
                );
            }
            return;
        }
    };

    let mut update = match updater.check().await {
        Ok(Some(update)) => update.restart_after_install(true),
        Ok(None) => {
            record(&app, "up-to-date", None, None);
            if manual {
                show_message(
                    &app,
                    locale,
                    locale.text("已是最新版本", "Up to Date"),
                    &format!(
                        "{} v{}",
                        locale.text("当前已是最新版本", "You are running the latest version"),
                        app.package_info().version
                    ),
                    MessageDialogKind::Info,
                );
            }
            return;
        }
        Err(error) => {
            record(&app, "check-failed", None, Some(error_category(&error)));
            if manual {
                show_message(
                    &app,
                    locale,
                    locale.text("检查更新失败", "Update Check Failed"),
                    failure_message(locale, FailureStage::Check, error_category(&error)),
                    MessageDialogKind::Error,
                );
            }
            return;
        }
    };

    // 插件 2.12 的检查超时不会继承到 Update，下载阶段需单独设置。
    update.timeout = Some(DOWNLOAD_TIMEOUT);
    let current = update.current_version.clone();
    let latest = update.version.clone();
    let notes: String = update
        .body
        .as_deref()
        .unwrap_or_default()
        .chars()
        .filter(|character| !character.is_control() || *character == '\n')
        .take(2000)
        .collect();
    record(&app, "update-available", Some(&latest), None);
    let message = if locale.code() == "zh-CN" {
        format!("发现新版本 v{latest}（当前 v{current}）。\n\n{notes}\n\n确认后将下载更新并静默安装、重启慧梦。请先保存正在编辑的内容。")
    } else {
        format!("Version v{latest} is available (current: v{current}).\n\n{notes}\n\nConfirm to download, install quietly and restart Huimeng. Save your work first.")
    };

    let (sender, mut receiver) = tauri::async_runtime::channel(1);
    let confirmation_app = app.clone();
    let confirmation_version = latest.clone();
    message_dialog(&app, &message)
        .title(locale.text("发现新版本", "Update Available"))
        .kind(MessageDialogKind::Info)
        .buttons(MessageDialogButtons::OkCancelCustom(
            locale.text("立即更新", "Update Now").to_string(),
            locale.text("稍后", "Later").to_string(),
        ))
        .show(move |answer| {
            if sender.try_send(answer).is_err() {
                record(
                    &confirmation_app,
                    "confirmation-closed",
                    Some(&confirmation_version),
                    None,
                );
            }
        });
    if receiver.recv().await != Some(true) {
        record(&app, "update-deferred", Some(&latest), None);
        return;
    }

    record(&app, "download-started", Some(&latest), None);
    let bytes = match update.download(|_, _| {}, || {}).await {
        Ok(bytes) => bytes,
        Err(error) => {
            let category = error_category(&error);
            record(&app, "download-failed", Some(&latest), Some(category));
            show_message(
                &app,
                locale,
                locale.text("更新下载失败", "Update Download Failed"),
                failure_message(locale, FailureStage::Download, category),
                MessageDialogKind::Error,
            );
            return;
        }
    };
    record(&app, "download-verified", Some(&latest), None);
    record(&app, "install-started", Some(&latest), None);

    // 解包和替换文件是同步操作，放入专用阻塞线程，保留主事件循环与其他异步工作。
    match tauri::async_runtime::spawn_blocking(move || update.install(bytes)).await {
        Ok(Ok(())) => {
            record(&app, "install-completed", Some(&latest), None);
            // Windows 安装器由 /R 负责重启，插件成功启动安装器后会退出当前进程。
            #[cfg(not(target_os = "windows"))]
            app.restart();
        }
        Ok(Err(error)) => {
            let category = error_category(&error);
            record(&app, "install-failed", Some(&latest), Some(category));
            show_message(
                &app,
                locale,
                locale.text("更新安装失败", "Update Installation Failed"),
                failure_message(locale, FailureStage::Install, category),
                MessageDialogKind::Error,
            );
        }
        Err(_) => {
            record(&app, "install-failed", Some(&latest), Some("runtime"));
            show_message(
                &app,
                locale,
                locale.text("更新安装失败", "Update Installation Failed"),
                failure_message(locale, FailureStage::Install, "runtime"),
                MessageDialogKind::Error,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_overlapping_updates_and_allows_retry_after_exit() {
        let state = UpdaterState::default();
        let active = state.try_start().expect("首次更新应能开始");
        assert!(state.try_start().is_none());
        drop(active);
        assert!(state.try_start().is_some());
    }

    #[test]
    fn failure_messages_do_not_include_raw_network_or_signature_details() {
        let errors = [
            Error::Network("https://private-route.example/sentinel-secret".to_string()),
            Error::SignatureUtf8("sentinel-private-signature".to_string()),
        ];
        for error in errors {
            let category = error_category(&error);
            for locale in [DesktopLocale::Chinese, DesktopLocale::English] {
                for stage in [
                    FailureStage::Check,
                    FailureStage::Download,
                    FailureStage::Install,
                ] {
                    let message = failure_message(locale, stage, category);
                    assert!(!message.contains("sentinel"));
                    assert!(!message.contains("private-route"));
                    assert!(!message.contains("https://"));
                    assert!(!message.is_empty());
                }
            }
        }
    }
}
