export type DesktopLocale = "zh-CN" | "en";

export function initialLocale(): DesktopLocale {
  const languages = navigator.languages.length ? navigator.languages : [navigator.language];
  for (const language of languages) {
    if (/^zh(?:-|$)/i.test(language)) return "zh-CN";
    if (/^en(?:-|$)/i.test(language)) return "en";
  }
  return "en";
}

export const messages = {
  "zh-CN": {
    title: "网络代理 · 慧梦",
    eyebrow: "慧梦 · 桌面设置",
    heading: "网络代理",
    description: "设置桌面端访问网络的方式。",
    connection: "连接方式",
    system: "跟随系统",
    systemDescription: "使用电脑的网络代理设置",
    manual: "手动代理",
    manualDescription: "仅为慧梦指定代理服务器",
    protocol: "代理协议",
    protocolHelp: "HTTP 代理也适用于 HTTPS 网站。",
    host: "代理主机",
    port: "端口",
    hostHelp: "只填主机名或 IP 地址，不含协议和路径。暂不支持账号密码认证。",
    restartNote: "配置仅保存在本机，重启后生效。重启前请先保存正在编辑的内容。",
    loading: "正在读取配置…",
    saving: "正在保存…",
    save: "保存配置",
    saved: "配置已保存，请退出慧梦后重新打开，使配置生效。",
    invalidHost: "请填写有效的代理主机，例如 127.0.0.1；不要填写协议、账号或路径。",
    invalidPort: "代理端口须为 1–65535 之间的整数。",
    settingsLoadError: "无法读取已保存的代理配置。请重新设置并保存，或选择跟随系统恢复连接。",
    localeLoadError: "无法读取桌面端语言设置，请重新读取后再保存配置。",
    saveError: "保存失败，请稍后重试。",
    retry: "重新读取",
  },
  en: {
    title: "Network Proxy · Huimeng",
    eyebrow: "Huimeng · Desktop Settings",
    heading: "Network Proxy",
    description: "Choose how the desktop app connects to the network.",
    connection: "Connection",
    system: "Use system settings",
    systemDescription: "Use your computer's proxy settings",
    manual: "Manual proxy",
    manualDescription: "Set a proxy for Huimeng only",
    protocol: "Proxy protocol",
    protocolHelp: "An HTTP proxy also works with HTTPS websites.",
    host: "Proxy host",
    port: "Port",
    hostHelp: "Enter a hostname or IP address without a protocol or path. Username and password authentication is not supported yet.",
    restartNote: "Settings are stored on this computer and take effect after restarting. Save your work before quitting.",
    loading: "Loading settings…",
    saving: "Saving…",
    save: "Save settings",
    saved: "Settings saved. Quit and reopen Huimeng to apply the changes.",
    invalidHost: "Enter a valid proxy host, such as 127.0.0.1, without a protocol, credentials, or path.",
    invalidPort: "The proxy port must be an integer between 1 and 65535.",
    settingsLoadError: "Saved proxy settings could not be read. Enter and save them again, or choose system settings to restore the connection.",
    localeLoadError: "The desktop language could not be read. Retry before saving your settings.",
    saveError: "Settings could not be saved. Please try again.",
    retry: "Retry",
  },
} as const;
