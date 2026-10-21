use std::marker::PhantomData;
use std::rc::Rc;

const FORCE_DARK: i32 = 2;

#[derive(Clone, Copy)]
struct ThemeApi {
    set_app_mode: unsafe extern "system" fn(i32) -> i32,
    flush_menu_themes: unsafe extern "system" fn(),
}

// 该序号在 Windows 10 1809 仍是另一种函数签名，必须先核对真实版本。
fn supports_preferred_app_mode(major: u32, minor: u32, build: u32) -> bool {
    major == 10 && minor == 0 && build >= 18362
}

/// 只在同步弹出菜单期间覆盖当前进程配色，关闭菜单后恢复原模式。
/// Rc 标记防止将这个主线程上的临时状态转交到其他线程。
pub(super) struct DarkPopupGuard {
    api: ThemeApi,
    previous_mode: i32,
    _main_thread: PhantomData<Rc<()>>,
}

impl DarkPopupGuard {
    #[cfg(windows)]
    pub(super) fn new() -> Option<Self> {
        native::available_api().map(Self::activate)
    }

    fn activate(api: ThemeApi) -> Self {
        let guard = Self {
            // 保留 API 返回的原始整数，不将外部返回值转换为 Rust enum。
            previous_mode: unsafe { (api.set_app_mode)(FORCE_DARK) },
            api,
            _main_thread: PhantomData,
        };
        unsafe { (api.flush_menu_themes)() };
        guard
    }
}

impl Drop for DarkPopupGuard {
    fn drop(&mut self) {
        unsafe {
            (self.api.set_app_mode)(self.previous_mode);
            (self.api.flush_menu_themes)();
        }
    }
}

#[cfg(windows)]
mod native {
    use super::{supports_preferred_app_mode, ThemeApi};
    use std::ffi::c_void;
    use std::mem::size_of;
    use std::ptr::{null_mut, NonNull};
    use std::sync::{Once, OnceLock};

    const LOAD_LIBRARY_SEARCH_SYSTEM32: u32 = 0x0000_0800;
    const SPI_GETHIGHCONTRAST: u32 = 0x0042;
    const HCF_HIGHCONTRASTON: u32 = 0x0000_0001;

    #[repr(C)]
    struct OsVersionInfo {
        size: u32,
        major: u32,
        minor: u32,
        build: u32,
        platform: u32,
        service_pack: [u16; 128],
    }

    #[repr(C)]
    struct HighContrast {
        size: u32,
        flags: u32,
        default_scheme: *mut u16,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn LoadLibraryExW(name: *const u16, file: *mut c_void, flags: u32) -> *mut c_void;
        fn GetProcAddress(module: *mut c_void, name: *const u8) -> *const c_void;
        fn FreeLibrary(module: *mut c_void) -> i32;
    }

    #[link(name = "ntdll")]
    extern "system" {
        fn RtlGetVersion(version: *mut OsVersionInfo) -> i32;
    }

    #[link(name = "user32")]
    extern "system" {
        fn SystemParametersInfoW(
            action: u32,
            parameter: u32,
            value: *mut c_void,
            flags: u32,
        ) -> i32;
    }

    fn retain_system_colors(reason: &'static str) {
        static NOTICE: Once = Once::new();
        NOTICE.call_once(|| {
            eprintln!("[desktop] 保留系统菜单配色：{reason}");
        });
    }

    pub(super) fn available_api() -> Option<ThemeApi> {
        // 每次展开都重新检查，尊重用户在应用运行期间启用的高对比度设置。
        let mut contrast = HighContrast {
            size: size_of::<HighContrast>() as u32,
            flags: 0,
            default_scheme: null_mut(),
        };
        let queried = unsafe {
            SystemParametersInfoW(
                SPI_GETHIGHCONTRAST,
                contrast.size,
                (&mut contrast as *mut HighContrast).cast(),
                0,
            )
        };
        if queried == 0 {
            retain_system_colors("无法读取高对比度设置");
            return None;
        }
        if contrast.flags & HCF_HIGHCONTRASTON != 0 {
            return None;
        }

        static API: OnceLock<Result<ThemeApi, &'static str>> = OnceLock::new();
        match API.get_or_init(load_api) {
            Ok(api) => Some(*api),
            Err(reason) => {
                retain_system_colors(reason);
                None
            }
        }
    }

    fn load_api() -> Result<ThemeApi, &'static str> {
        let mut version = OsVersionInfo {
            size: size_of::<OsVersionInfo>() as u32,
            major: 0,
            minor: 0,
            build: 0,
            platform: 0,
            service_pack: [0; 128],
        };
        if unsafe { RtlGetVersion(&mut version) } != 0 {
            return Err("无法核对 Windows 版本");
        }
        if !supports_preferred_app_mode(version.major, version.minor, version.build) {
            return Err("当前 Windows 版本不支持菜单深色配色接口");
        }

        let name: Vec<u16> = "uxtheme.dll\0".encode_utf16().collect();
        let module = NonNull::new(unsafe {
            LoadLibraryExW(name.as_ptr(), null_mut(), LOAD_LIBRARY_SEARCH_SYSTEM32)
        })
        .ok_or("无法读取系统主题库")?;

        // 与 Tao 相同的按序号加载策略；PowerToys 也用 135/136 设置弹出菜单主题：
        // https://github.com/microsoft/PowerToys/blob/main/src/modules/ZoomIt/ZoomIt/Utility.cpp
        let set_mode = unsafe { GetProcAddress(module.as_ptr(), 135usize as *const u8) };
        let flush = unsafe { GetProcAddress(module.as_ptr(), 136usize as *const u8) };
        if set_mode.is_null() || flush.is_null() {
            unsafe { FreeLibrary(module.as_ptr()) };
            return Err("系统主题库缺少弹出菜单配色接口");
        }

        // 接口已按真实 OS 版本和非空地址验证。系统库保留到进程退出，
        // 以确保 OnceLock 保存的函数指针始终有效。
        Ok(ThemeApi {
            set_app_mode: unsafe {
                std::mem::transmute::<*const c_void, unsafe extern "system" fn(i32) -> i32>(
                    set_mode,
                )
            },
            flush_menu_themes: unsafe {
                std::mem::transmute::<*const c_void, unsafe extern "system" fn()>(flush)
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicI32, AtomicUsize, Ordering};
    use std::sync::Mutex;

    static MODE: AtomicI32 = AtomicI32::new(0);
    static FLUSHES: AtomicUsize = AtomicUsize::new(0);
    static LOCK: Mutex<()> = Mutex::new(());

    unsafe extern "system" fn set_mode(mode: i32) -> i32 {
        MODE.swap(mode, Ordering::SeqCst)
    }

    unsafe extern "system" fn flush() {
        FLUSHES.fetch_add(1, Ordering::SeqCst);
    }

    fn api() -> ThemeApi {
        ThemeApi {
            set_app_mode: set_mode,
            flush_menu_themes: flush,
        }
    }

    #[test]
    fn rejects_versions_with_the_old_ordinal_135_signature() {
        for (major, minor, build) in [(6, 3, 9600), (10, 0, 17763), (10, 0, 18361)] {
            assert!(!supports_preferred_app_mode(major, minor, build));
        }
        for build in [18362, 19045, 22000, 26100] {
            assert!(supports_preferred_app_mode(10, 0, build));
        }
        assert!(!supports_preferred_app_mode(11, 0, 30000));
    }

    #[test]
    fn restores_the_previous_mode_after_nested_menus_close() {
        let _lock = LOCK.lock().unwrap();
        MODE.store(1, Ordering::SeqCst);
        FLUSHES.store(0, Ordering::SeqCst);
        {
            let _outer = DarkPopupGuard::activate(api());
            assert_eq!(MODE.load(Ordering::SeqCst), FORCE_DARK);
            {
                let _inner = DarkPopupGuard::activate(api());
                assert_eq!(MODE.load(Ordering::SeqCst), FORCE_DARK);
            }
            assert_eq!(MODE.load(Ordering::SeqCst), FORCE_DARK);
        }
        assert_eq!(MODE.load(Ordering::SeqCst), 1);
        assert_eq!(FLUSHES.load(Ordering::SeqCst), 4);
    }

    #[test]
    fn restores_mode_even_when_the_popup_callback_unwinds() {
        let _lock = LOCK.lock().unwrap();
        MODE.store(3, Ordering::SeqCst);
        FLUSHES.store(0, Ordering::SeqCst);
        let result = std::panic::catch_unwind(|| {
            let _guard = DarkPopupGuard::activate(api());
            panic!("模拟菜单回调提前结束");
        });
        assert!(result.is_err());
        assert_eq!(MODE.load(Ordering::SeqCst), 3);
        assert_eq!(FLUSHES.load(Ordering::SeqCst), 2);
    }
}
