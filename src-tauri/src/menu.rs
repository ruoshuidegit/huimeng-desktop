use tauri::menu::{
    AboutMetadata, Menu, MenuItem, PredefinedMenuItem, Submenu, HELP_SUBMENU_ID, WINDOW_SUBMENU_ID,
};

use crate::locale::DesktopLocale;

pub fn build(app: &tauri::AppHandle, locale: DesktopLocale) -> tauri::Result<Menu<tauri::Wry>> {
    let text = |chinese, english| locale.text(chinese, english);
    let app_name = text("慧梦", "Huimeng");
    let check_updates = MenuItem::with_id(
        app,
        "check-for-updates",
        text("检查更新…", "Check for Updates…"),
        true,
        None::<&str>,
    )?;
    let maximize_title = if cfg!(target_os = "macos") {
        text("缩放", "Zoom")
    } else {
        text("最大化", "Maximize")
    };
    let metadata = AboutMetadata {
        name: Some(app_name.to_string()),
        version: Some(app.package_info().version.to_string()),
        copyright: app.config().bundle.copyright.clone(),
        authors: app.config().bundle.publisher.clone().map(|name| vec![name]),
        ..Default::default()
    };

    // 保持 Tauri 默认菜单的原生动作、快捷键及平台差异，仅显式提供本地化标题。
    let menu = Menu::new(app)?;
    #[cfg(target_os = "macos")]
    menu.append(&Submenu::with_items(
        app,
        app_name,
        true,
        &[
            &PredefinedMenuItem::about(
                app,
                Some(text("关于慧梦", "About Huimeng")),
                Some(metadata.clone()),
            )?,
            &check_updates,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::services(app, Some(text("服务", "Services")))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::hide(app, Some(text("隐藏慧梦", "Hide Huimeng")))?,
            &PredefinedMenuItem::hide_others(app, Some(text("隐藏其他", "Hide Others")))?,
            &PredefinedMenuItem::show_all(app, Some(text("显示全部", "Show All")))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::quit(app, Some(text("退出慧梦", "Quit Huimeng")))?,
        ],
    )?)?;

    #[cfg(not(any(
        target_os = "linux",
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd"
    )))]
    menu.append(&Submenu::with_items(
        app,
        text("文件", "File"),
        true,
        &[
            &PredefinedMenuItem::close_window(app, Some(text("关闭窗口", "Close Window")))?,
            #[cfg(not(target_os = "macos"))]
            &PredefinedMenuItem::quit(app, Some(text("退出慧梦", "Quit Huimeng")))?,
        ],
    )?)?;

    menu.append(&Submenu::with_items(
        app,
        text("编辑", "Edit"),
        true,
        &[
            &PredefinedMenuItem::undo(app, Some(text("撤销", "Undo")))?,
            &PredefinedMenuItem::redo(app, Some(text("重做", "Redo")))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, Some(text("剪切", "Cut")))?,
            &PredefinedMenuItem::copy(app, Some(text("复制", "Copy")))?,
            &PredefinedMenuItem::paste(app, Some(text("粘贴", "Paste")))?,
            &PredefinedMenuItem::select_all(app, Some(text("全选", "Select All")))?,
        ],
    )?)?;

    #[cfg(target_os = "macos")]
    menu.append(&Submenu::with_items(
        app,
        text("显示", "View"),
        true,
        &[&PredefinedMenuItem::fullscreen(
            app,
            Some(text("切换全屏", "Toggle Full Screen")),
        )?],
    )?)?;

    menu.append(&Submenu::with_id_and_items(
        app,
        WINDOW_SUBMENU_ID,
        text("窗口", "Window"),
        true,
        &[
            &PredefinedMenuItem::minimize(app, Some(text("最小化", "Minimize")))?,
            &PredefinedMenuItem::maximize(app, Some(maximize_title))?,
            #[cfg(target_os = "macos")]
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::close_window(app, Some(text("关闭窗口", "Close Window")))?,
        ],
    )?)?;

    menu.append(&Submenu::with_id_and_items(
        app,
        HELP_SUBMENU_ID,
        text("帮助", "Help"),
        true,
        &[
            #[cfg(not(target_os = "macos"))]
            &PredefinedMenuItem::about(
                app,
                Some(text("关于慧梦", "About Huimeng")),
                Some(metadata),
            )?,
            #[cfg(not(target_os = "macos"))]
            &check_updates,
        ],
    )?)?;

    let proxy_item = MenuItem::with_id(
        app,
        "proxy-settings",
        text("网络代理…", "Network Proxy…"),
        true,
        Some("CmdOrCtrl+,"),
    )?;
    menu.append(&Submenu::with_items(
        app,
        text("设置", "Settings"),
        true,
        &[&proxy_item],
    )?)?;
    Ok(menu)
}
