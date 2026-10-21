#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DesktopLocale {
    Chinese,
    English,
}

impl DesktopLocale {
    pub fn detect() -> Self {
        Self::from_preferred_languages(preferred_languages())
    }

    pub fn code(self) -> &'static str {
        match self {
            Self::Chinese => "zh-CN",
            Self::English => "en",
        }
    }

    pub fn text(self, chinese: &'static str, english: &'static str) -> &'static str {
        match self {
            Self::Chinese => chinese,
            Self::English => english,
        }
    }

    fn from_preferred_languages(languages: impl IntoIterator<Item = impl AsRef<str>>) -> Self {
        for language in languages {
            let language = language.as_ref().trim();
            let identifier = language.split(['.', '@']).next().unwrap_or("");
            let mut subtags = identifier.split(['-', '_']);
            let primary = subtags.next().unwrap_or("");
            if subtags.any(|subtag| {
                subtag.is_empty()
                    || subtag.len() > 8
                    || !subtag
                        .bytes()
                        .all(|character| character.is_ascii_alphanumeric())
            }) {
                continue;
            }
            if primary.eq_ignore_ascii_case("zh") {
                return Self::Chinese;
            }
            if primary.eq_ignore_ascii_case("en") {
                return Self::English;
            }
        }
        Self::English
    }
}

#[cfg(target_os = "macos")]
fn preferred_languages() -> Vec<String> {
    use objc2_foundation::{NSLocale, NSString, NSUserDefaults};

    // NSUserDefaults 先查当前应用域，兼容 macOS 的“应用程序语言”和启动参数覆盖。
    let defaults = NSUserDefaults::standardUserDefaults();
    let languages = defaults
        .stringArrayForKey(&NSString::from_str("AppleLanguages"))
        .filter(|languages| !languages.is_empty())
        .unwrap_or_else(NSLocale::preferredLanguages);
    languages
        .to_vec()
        .into_iter()
        .map(|language| language.to_string())
        .collect()
}

#[cfg(target_os = "windows")]
fn preferred_languages() -> Vec<String> {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetUserPreferredUILanguages(
            flags: u32,
            number_of_languages: *mut u32,
            buffer: *mut u16,
            buffer_size: *mut u32,
        ) -> i32;
        fn GetUserDefaultUILanguage() -> u16;
    }

    const MUI_LANGUAGE_NAME: u32 = 0x8;
    let mut count = 0;
    let mut length = 0;
    // 按 Win32 两次调用约定先查询 UTF-16 缓冲区长度，再读取有序的 UI 语言列表。
    let queried = unsafe {
        GetUserPreferredUILanguages(
            MUI_LANGUAGE_NAME,
            &mut count,
            std::ptr::null_mut(),
            &mut length,
        )
    };
    if queried != 0 && length > 0 {
        let mut buffer = vec![0u16; length as usize];
        let read = unsafe {
            GetUserPreferredUILanguages(
                MUI_LANGUAGE_NAME,
                &mut count,
                buffer.as_mut_ptr(),
                &mut length,
            )
        };
        if read != 0 {
            return buffer
                .split(|character| *character == 0)
                .filter(|language| !language.is_empty())
                .map(String::from_utf16_lossy)
                .collect();
        }
    }

    // 该 API 返回显示语言，而非数字、日期使用的区域格式。
    let language = unsafe { GetUserDefaultUILanguage() };
    vec![if language & 0x03ff == 0x04 {
        "zh".to_string()
    } else {
        "en".to_string()
    }]
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn preferred_languages() -> Vec<String> {
    let mut languages = Vec::new();
    if let Ok(preferred) = std::env::var("LANGUAGE") {
        languages.extend(preferred.split(':').map(str::to_owned));
    }
    for variable in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(language) = std::env::var(variable) {
            if !language.is_empty() {
                languages.push(language);
                break;
            }
        }
    }
    languages
}

#[cfg(test)]
mod tests {
    use super::DesktopLocale;

    #[test]
    fn follows_supported_ui_language_priority_instead_of_region() {
        assert_eq!(
            DesktopLocale::from_preferred_languages(["en-CN", "zh-Hans-CN"]),
            DesktopLocale::English
        );
        assert_eq!(
            DesktopLocale::from_preferred_languages(["zh-Hans-US", "en-US"]),
            DesktopLocale::Chinese
        );
        assert_eq!(
            DesktopLocale::from_preferred_languages(["fr-FR", "zh-Hant-TW", "en-US"]),
            DesktopLocale::Chinese
        );
        assert_eq!(
            DesktopLocale::from_preferred_languages(["", "unknown", "EN_gb.UTF-8"]),
            DesktopLocale::English
        );
        assert_eq!(
            DesktopLocale::from_preferred_languages(["unknown", " zh_Hans_CN "]),
            DesktopLocale::Chinese
        );
        assert_eq!(
            DesktopLocale::from_preferred_languages(["zh-???", "en-US"]),
            DesktopLocale::English
        );
        assert_eq!(
            DesktopLocale::from_preferred_languages(["fr-FR", "ja-JP"]),
            DesktopLocale::English
        );
    }
}
