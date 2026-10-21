use std::fs::{self, OpenOptions};
use std::io::Write;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

const INVALID_HOST: &str = "请输入代理主机名或 IP 地址，不要包含协议、端口、路径或账号密码。";
const SAVE_ERROR: &str = "代理配置保存失败，请检查本地配置目录的写入权限后重试。";
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ProxyMode {
    #[default]
    System,
    Manual,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ProxyProtocol {
    #[default]
    Http,
    Socks5,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProxySettings {
    pub mode: ProxyMode,
    pub protocol: ProxyProtocol,
    pub host: String,
    pub port: u16,
}

impl Default for ProxySettings {
    fn default() -> Self {
        Self {
            mode: ProxyMode::System,
            protocol: ProxyProtocol::Http,
            host: "127.0.0.1".into(),
            port: 7897,
        }
    }
}

impl ProxySettings {
    pub fn normalize(mut self) -> Result<Self, String> {
        if self.mode == ProxyMode::System {
            return Ok(Self::default());
        }
        if self.port == 0 {
            return Err("代理端口必须在 1 到 65535 之间。".into());
        }
        // 当前 Tauri 运行时会丢弃 URL 的默认端口，不能正确传递 HTTP 80。
        if self.protocol == ProxyProtocol::Http && self.port == 80 {
            return Err(
                "当前版本不支持 HTTP 代理的 80 端口，请改用其他 HTTP 代理端口或 SOCKS5。".into(),
            );
        }
        self.host = normalize_host(&self.host)?;
        Ok(self)
    }

    pub fn proxy_url(&self) -> Result<Option<tauri::Url>, String> {
        let settings = self.clone().normalize()?;
        if settings.mode == ProxyMode::System {
            return Ok(None);
        }
        let protocol = match settings.protocol {
            ProxyProtocol::Http => "http",
            ProxyProtocol::Socks5 => "socks5",
        };
        let host = if settings.host.contains(':') {
            format!("[{}]", settings.host)
        } else {
            settings.host
        };
        tauri::Url::parse(&format!("{protocol}://{host}:{}", settings.port))
            .map(Some)
            .map_err(|_| INVALID_HOST.into())
    }
}

fn normalize_host(host: &str) -> Result<String, String> {
    if host.is_empty() || host.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(INVALID_HOST.into());
    }
    let ip_host = host
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .unwrap_or(host);
    if let Ok(ip) = ip_host.parse::<IpAddr>() {
        // 当前 macOS WebView 会把带方括号的 IPv6 主机传给原生代理接口，导致连接失败。
        #[cfg(target_os = "macos")]
        if ip.is_ipv6() {
            return Err(
                "当前 macOS 客户端暂不支持 IPv6 代理地址，请改用 IPv4 地址或主机名。".into(),
            );
        }
        return Ok(ip.to_string());
    }
    if host
        .chars()
        .any(|c| matches!(c, ':' | '/' | '\\' | '@' | '?' | '#' | '%' | '[' | ']'))
    {
        return Err(INVALID_HOST.into());
    }
    let parsed =
        tauri::Url::parse(&format!("http://{host}/")).map_err(|_| INVALID_HOST.to_string())?;
    let normalized = parsed.host_str().ok_or(INVALID_HOST)?.to_string();
    let domain = normalized.strip_suffix('.').unwrap_or(&normalized);
    if domain.len() > 253
        || domain.split('.').any(|label| {
            label.is_empty()
                || label.len() > 63
                || label.starts_with('-')
                || label.ends_with('-')
                || !label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
    {
        return Err(INVALID_HOST.into());
    }
    // URL 会把 127.1、十六进制等写法转换为另一 IP，要求用户填写完整地址。
    if normalized.parse::<IpAddr>().is_ok() {
        return Err(INVALID_HOST.into());
    }
    Ok(normalized)
}

pub fn load(path: &Path) -> Result<ProxySettings, String> {
    let content = match fs::read(path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ProxySettings::default());
        }
        Err(_) => return Err("代理配置读取失败，请在“网络代理”中重新保存配置。".into()),
    };
    serde_json::from_slice::<ProxySettings>(&content)
        .map_err(|_| "代理配置已损坏，请在“网络代理”中重新保存配置。".to_string())?
        .normalize()
}

pub fn save(path: &Path, settings: ProxySettings) -> Result<ProxySettings, String> {
    let settings = settings.normalize()?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|_| SAVE_ERROR.to_string())?;
    let data = serde_json::to_vec_pretty(&settings).map_err(|_| SAVE_ERROR.to_string())?;
    let (temporary, mut file) = create_temporary(parent)?;
    let written = file.write_all(&data).and_then(|_| file.sync_all());
    // Windows 上替换文件前必须释放写入句柄。
    drop(file);
    if written.is_err() || fs::rename(&temporary, path).is_err() {
        if let Err(error) = fs::remove_file(&temporary) {
            if error.kind() != std::io::ErrorKind::NotFound {
                eprintln!("[proxy] 无法清理未保存的配置临时文件");
            }
        }
        return Err(SAVE_ERROR.into());
    }
    Ok(settings)
}

fn create_temporary(parent: &Path) -> Result<(PathBuf, fs::File), String> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    for _ in 0..8 {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let temporary = parent.join(format!(".proxy-{}-{sequence}.tmp", std::process::id()));
        match options.open(&temporary) {
            Ok(file) => return Ok((temporary, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return Err(SAVE_ERROR.into()),
        }
    }
    Err(SAVE_ERROR.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manual(host: &str, port: u16, protocol: ProxyProtocol) -> ProxySettings {
        ProxySettings {
            mode: ProxyMode::Manual,
            protocol,
            host: host.into(),
            port,
        }
    }

    #[test]
    fn normalizes_hosts_and_preserves_explicit_proxy_port() {
        let settings = manual("Proxy.Example.com", 7890, ProxyProtocol::Http)
            .normalize()
            .unwrap();
        assert_eq!(settings.host, "proxy.example.com");
        let url = settings.proxy_url().unwrap().unwrap();
        assert_eq!(url.scheme(), "http");
        assert_eq!(url.port(), Some(7890));
    }

    #[test]
    #[cfg(not(target_os = "macos"))]
    fn normalizes_ipv6_proxy_address() {
        let settings = manual("[::1]", 1080, ProxyProtocol::Socks5)
            .normalize()
            .unwrap();
        assert_eq!(settings.host, "::1");
        assert_eq!(
            settings.proxy_url().unwrap().unwrap().as_str(),
            "socks5://[::1]:1080"
        );
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn rejects_ipv6_proxy_literals_on_macos() {
        for host in ["::1", "[::1]", "2001:db8::1", "[::ffff:127.0.0.1]"] {
            for protocol in [ProxyProtocol::Http, ProxyProtocol::Socks5] {
                let settings = manual(host, 7890, protocol);
                assert_eq!(
                    settings.clone().normalize().unwrap_err(),
                    "当前 macOS 客户端暂不支持 IPv6 代理地址，请改用 IPv4 地址或主机名。"
                );
                assert!(settings.proxy_url().is_err());
            }
        }
    }

    #[test]
    fn rejects_url_components_credentials_and_ambiguous_hosts() {
        for host in [
            "",
            "http://localhost",
            "user:password@localhost",
            "localhost:7890",
            "localhost/path",
            "localhost?query",
            "localhost#fragment",
            "localhost\\path",
            " local.test",
            "local\t.test",
            "local\n.test",
            "local%2etest",
            "-local.test",
            "local..test",
            "127.1",
            "0x7f000001",
        ] {
            assert!(
                manual(host, 7890, ProxyProtocol::Http).normalize().is_err(),
                "应拒绝主机输入：{host:?}"
            );
        }
        assert!(manual("localhost", 0, ProxyProtocol::Http)
            .normalize()
            .is_err());
        assert!(manual("localhost", 80, ProxyProtocol::Http)
            .normalize()
            .is_err());
    }

    #[test]
    fn system_mode_discards_inactive_manual_fields() {
        let settings = ProxySettings {
            mode: ProxyMode::System,
            protocol: ProxyProtocol::Socks5,
            host: "invalid://credentials@host".into(),
            port: 0,
        };
        assert_eq!(
            settings.clone().normalize().unwrap(),
            ProxySettings::default()
        );
        assert!(settings.proxy_url().unwrap().is_none());
    }

    #[test]
    fn persists_replaces_and_recovers_from_corrupt_configuration() {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let directory = std::env::temp_dir().join(format!(
            "huimeng-proxy-test-{}-{sequence}",
            std::process::id()
        ));
        let path = directory.join("nested").join("proxy.json");
        assert_eq!(load(&path).unwrap(), ProxySettings::default());
        let settings = manual("127.0.0.1", 7890, ProxyProtocol::Http);
        assert_eq!(save(&path, settings.clone()).unwrap(), settings);
        assert_eq!(load(&path).unwrap(), settings);
        let previous = fs::read(&path).unwrap();
        assert!(save(&path, manual("bad/host", 7890, ProxyProtocol::Http)).is_err());
        assert_eq!(fs::read(&path).unwrap(), previous);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        save(&path, ProxySettings::default()).unwrap();
        assert_eq!(load(&path).unwrap(), ProxySettings::default());
        fs::write(&path, b"invalid json").unwrap();
        assert!(load(&path).unwrap_err().contains("已损坏"));
        save(&path, settings.clone()).unwrap();
        assert_eq!(load(&path).unwrap(), settings);
        fs::remove_dir_all(&directory).unwrap();
    }
}
