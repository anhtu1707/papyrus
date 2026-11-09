//! Windows WebView2 integration. COM handles and callbacks stay on the UI thread.
use crate::{EventMessage, SharedPreferences, model::should_block};
use std::{cell::RefCell, rc::Rc};
use tao::event_loop::EventLoopProxy;
use webview2_com::{
    AcceleratorKeyPressedEventHandler, ClearBrowsingDataCompletedHandler,
    HistoryChangedEventHandler, IsDocumentPlayingAudioChangedEventHandler,
    IsMutedChangedEventHandler, Microsoft::Web::WebView2::Win32::*,
    NavigationCompletedEventHandler, NavigationStartingEventHandler,
    NewWindowRequestedEventHandler, ProcessFailedEventHandler, SourceChangedEventHandler,
    TrySuspendCompletedHandler, WebResourceRequestedEventHandler, WindowCloseRequestedEventHandler,
    take_pwstr,
};
use windows::{
    Win32::System::Com::IStream,
    core::{Interface, PWSTR, w},
};
use wry::{WebView, WebViewExtWindows};

use std::{cell::Cell, collections::VecDeque};
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, REG_SZ, RegCloseKey, RegCreateKeyW, RegSetValueExW,
};
use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
    Graphics::{
        Dwm::{DWMWINDOWATTRIBUTE, DwmExtendFrameIntoClientArea, DwmSetWindowAttribute},
        Gdi::{
            ClientToScreen, GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTONULL,
            MONITORINFO, MonitorFromRect, MonitorFromWindow, ScreenToClient,
        },
    },
    UI::{
        Controls::MARGINS,
        HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext},
        Input::KeyboardAndMouse::{
            HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, RegisterHotKey,
            UnregisterHotKey,
        },
        Shell::{DefSubclassProc, SetWindowSubclass, ShellExecuteW},
        WindowsAndMessaging::{
            AppendMenuW, CreatePopupMenu, DestroyMenu, EnumWindows, GWL_STYLE, GetClientRect,
            GetWindowLongW, GetWindowPlacement, GetWindowTextW, HTMAXBUTTON, IsIconic, IsZoomed,
            MF_GRAYED, MF_SEPARATOR, MF_STRING, NCCALCSIZE_PARAMS, PostMessageW, SC_RESTORE,
            SW_MAXIMIZE, SW_RESTORE, SW_SHOWNORMAL, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE,
            SWP_NOSIZE, SWP_NOZORDER, SetForegroundWindow, SetWindowLongW, SetWindowPos, ShowWindow,
            TPM_LEFTALIGN, TPM_RETURNCMD, TPM_TOPALIGN, TrackPopupMenuEx, WINDOWPLACEMENT,
            WM_ACTIVATE, WM_APP, WM_DPICHANGED, WM_HOTKEY, WM_NCCALCSIZE, WM_NCHITTEST,
            WM_NCLBUTTONDOWN, WM_NCLBUTTONUP, WM_SYSCOMMAND, WS_CAPTION, WS_MAXIMIZEBOX,
            WS_MINIMIZEBOX, WS_THICKFRAME,
        },
    },
};
use windows::core::PCWSTR;
const DISPATCH_MESSAGE: u32 = WM_APP + 0x321;

const HOTKEYS: &[(i32, HOT_KEY_MODIFIERS, u32)] = &[
    (1, HOT_KEY_MODIFIERS(MOD_CONTROL.0 | MOD_NOREPEAT.0), 0x48), // Ctrl + H (History)
    (2, HOT_KEY_MODIFIERS(MOD_CONTROL.0 | MOD_NOREPEAT.0), 0x4a), // Ctrl + J (Downloads)
    (3, HOT_KEY_MODIFIERS(MOD_CONTROL.0 | MOD_NOREPEAT.0), 0x54), // Ctrl + T (New Tab)
    (
        4,
        HOT_KEY_MODIFIERS(MOD_CONTROL.0 | MOD_SHIFT.0 | MOD_NOREPEAT.0),
        0x54,
    ), // Ctrl + Shift + T (Reopen Tab)
    (5, HOT_KEY_MODIFIERS(MOD_CONTROL.0 | MOD_NOREPEAT.0), 0x57), // Ctrl + W (Close Tab)
    (6, HOT_KEY_MODIFIERS(MOD_CONTROL.0 | MOD_NOREPEAT.0), 0x4c), // Ctrl + L (Focus Address)
    (7, HOT_KEY_MODIFIERS(MOD_ALT.0 | MOD_NOREPEAT.0), 0x44),     // Alt + D (Focus Address)
    (8, HOT_KEY_MODIFIERS(MOD_CONTROL.0 | MOD_NOREPEAT.0), 0x44), // Ctrl + D (Bookmark)
    (9, HOT_KEY_MODIFIERS(MOD_NOREPEAT.0), 0x7b),                 // F12 (DevTools)
    (
        10,
        HOT_KEY_MODIFIERS(MOD_CONTROL.0 | MOD_SHIFT.0 | MOD_NOREPEAT.0),
        0x49,
    ), // Ctrl + Shift + I (DevTools)
    (11, HOT_KEY_MODIFIERS(MOD_CONTROL.0 | MOD_NOREPEAT.0), 0x52), // Ctrl + R (Reload)
    (12, HOT_KEY_MODIFIERS(MOD_NOREPEAT.0), 0x74),                // F5 (Reload)
    (
        13,
        HOT_KEY_MODIFIERS(MOD_CONTROL.0 | MOD_SHIFT.0 | MOD_NOREPEAT.0),
        0x2e,
    ), // Ctrl + Shift + Del (Clear Data)
    (14, HOT_KEY_MODIFIERS(MOD_CONTROL.0 | MOD_NOREPEAT.0), 0x50), // Ctrl + P (Print)
    (15, HOT_KEY_MODIFIERS(MOD_CONTROL.0 | MOD_NOREPEAT.0), 0x4e), // Ctrl + N (New Window)
    (16, HOT_KEY_MODIFIERS(MOD_NOREPEAT.0), 0x7a),                // F11 (Fullscreen)
    (
        17,
        HOT_KEY_MODIFIERS(MOD_CONTROL.0 | MOD_SHIFT.0 | MOD_NOREPEAT.0),
        0x4f,
    ), // Ctrl + Shift + O (Bookmarks)
];

unsafe fn register_app_hotkeys(hwnd: HWND) {
    for &(id, mods, vk) in HOTKEYS {
        unsafe {
            let _ = RegisterHotKey(Some(hwnd), id, mods, vk);
        }
    }
}

unsafe fn unregister_app_hotkeys(hwnd: HWND) {
    for &(id, _, _) in HOTKEYS {
        unsafe {
            let _ = UnregisterHotKey(Some(hwnd), id);
        }
    }
}

fn hotkey_to_command(id: i32) -> Option<crate::model::Command> {
    match id {
        1 => Some(crate::model::Command::History),
        2 => Some(crate::model::Command::Downloads),
        3 => Some(crate::model::Command::NewTab),
        4 => Some(crate::model::Command::ReopenTab),
        5 => Some(crate::model::Command::CloseTab { id: 0 }),
        6 | 7 => Some(crate::model::Command::FocusAddress),
        8 => Some(crate::model::Command::Bookmark),
        9 | 10 => Some(crate::model::Command::DevTools),
        11 | 12 => Some(crate::model::Command::Reload),
        13 => Some(crate::model::Command::ClearData),
        14 => Some(crate::model::Command::Print),
        15 => Some(crate::model::Command::NewWindow),
        16 => Some(crate::model::Command::ToggleFullscreen),
        17 => Some(crate::model::Command::Bookmarks),
        _ => None,
    }
}

thread_local! {
    static TASKS: RefCell<VecDeque<Box<dyn FnOnce()>>> = RefCell::new(VecDeque::new());
    static DISPATCHING: Cell<bool> = const { Cell::new(false) };
    static EVENT_PROXY: RefCell<Option<EventLoopProxy<EventMessage>>> = const { RefCell::new(None) };
}

fn is_windows_11() -> bool {
    static IS_WIN11: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *IS_WIN11.get_or_init(|| {
        #[repr(C)]
        #[allow(non_snake_case, clippy::upper_case_acronyms)]
        struct OSVERSIONINFOW {
            dwOSVersionInfoSize: u32,
            dwMajorVersion: u32,
            dwMinorVersion: u32,
            dwBuildNumber: u32,
            dwPlatformId: u32,
            szCSDVersion: [u16; 128],
        }
        type RtlGetVersionFn = unsafe extern "system" fn(*mut OSVERSIONINFOW) -> i32;
        unsafe {
            use windows::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};
            use windows::core::s;
            if let Ok(h_ntdll) = GetModuleHandleA(s!("ntdll.dll"))
                && let Some(proc) = GetProcAddress(h_ntdll, s!("RtlGetVersion"))
            {
                let rtl_get_version: RtlGetVersionFn = std::mem::transmute(proc);
                let mut osinfo = std::mem::zeroed::<OSVERSIONINFOW>();
                osinfo.dwOSVersionInfoSize = std::mem::size_of::<OSVERSIONINFOW>() as u32;
                if rtl_get_version(&mut osinfo) == 0 {
                    return osinfo.dwBuildNumber >= 22000;
                }
            }
        }
        false
    })
}

unsafe extern "system" fn dispatcher(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _: usize,
    _: usize,
) -> LRESULT {
    match message {
        WM_NCHITTEST => {
            if is_windows_11() {
                let mut pt = POINT {
                    x: (lparam.0 & 0xffff) as i16 as i32,
                    y: ((lparam.0 >> 16) & 0xffff) as i16 as i32,
                };
                unsafe {
                    let _ = ScreenToClient(hwnd, &mut pt);
                }
                let mut rc = RECT::default();
                unsafe {
                    let _ = GetClientRect(hwnd, &mut rc);
                }
                let width = rc.right - rc.left;
                // Native Windows 11 Snap Layouts hover target on top right maximize button:
                if pt.y >= 0 && pt.y <= 36 && pt.x >= (width - 88) && pt.x <= (width - 44) {
                    return LRESULT(HTMAXBUTTON as isize);
                }
            }
        }
        WM_NCLBUTTONDOWN if wparam.0 == HTMAXBUTTON as usize => {
            return LRESULT(0);
        }
        WM_NCLBUTTONUP if wparam.0 == HTMAXBUTTON as usize => {
            let win_is_max = unsafe {
                let mut wp = WINDOWPLACEMENT {
                    length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
                    ..Default::default()
                };
                GetWindowPlacement(hwnd, &mut wp).is_ok() && wp.showCmd == SW_MAXIMIZE.0 as u32
            };
            unsafe {
                let _ = ShowWindow(hwnd, if win_is_max { SW_RESTORE } else { SW_MAXIMIZE });
            }
            return LRESULT(0);
        }
        WM_DPICHANGED => {
            let rect = unsafe { &*(lparam.0 as *const RECT) };
            unsafe {
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    rect.left,
                    rect.top,
                    rect.right - rect.left,
                    rect.bottom - rect.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
            return LRESULT(0);
        }
        WM_NCCALCSIZE => {
            if wparam.0 != 0 {
                let win_is_max = unsafe {
                    IsZoomed(hwnd).as_bool()
                        || {
                            let mut wp = WINDOWPLACEMENT {
                                length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
                                ..Default::default()
                            };
                            GetWindowPlacement(hwnd, &mut wp).is_ok()
                                && (wp.showCmd == SW_MAXIMIZE.0 as u32 || (wp.flags.0 & 2) != 0)
                        }
                };
                if win_is_max {
                    let hmon = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
                    let mut mi = MONITORINFO {
                        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                        ..Default::default()
                    };
                    if unsafe { GetMonitorInfoW(hmon, &mut mi).as_bool() } {
                        let params = unsafe { &mut *(lparam.0 as *mut NCCALCSIZE_PARAMS) };
                        params.rgrc[0] = mi.rcWork;
                    }
                }
                return LRESULT(0);
            }
        }
        WM_HOTKEY => {
            if let Some(cmd) = hotkey_to_command(wparam.0 as i32) {
                EVENT_PROXY.with(|p| {
                    if let Some(ref proxy) = *p.borrow() {
                        let _ = proxy.send_event(EventMessage::Command(cmd));
                    }
                });
                return LRESULT(0);
            }
        }
        WM_ACTIVATE => {
            let active = (wparam.0 & 0xffff) != 0;
            unsafe {
                if active {
                    register_app_hotkeys(hwnd);
                } else {
                    unregister_app_hotkeys(hwnd);
                }
            }
            if active {
                EVENT_PROXY.with(|p| {
                    if let Some(ref proxy) = *p.borrow() {
                        let _ = proxy.send_event(EventMessage::History);
                    }
                });
            }
        }
        WM_SYSCOMMAND if (wparam.0 & 0xfff0) == SC_RESTORE as usize => {
            EVENT_PROXY.with(|p| {
                if let Some(ref proxy) = *p.borrow() {
                    let _ = proxy.send_event(EventMessage::History);
                }
            });
        }
        _ if message == DISPATCH_MESSAGE => {
            if DISPATCHING.with(|running| running.replace(true)) {
                return LRESULT(0);
            }
            while let Some(task) = TASKS.with(|tasks| tasks.borrow_mut().pop_front()) {
                // Never let a Rust panic cross the Windows callback boundary.
                if std::panic::catch_unwind(std::panic::AssertUnwindSafe(task)).is_err() {
                    eprintln!("Papyrus: tác vụ giao diện gặp lỗi nội bộ.");
                }
            }
            DISPATCHING.with(|running| running.set(false));
            return LRESULT(0);
        }
        _ => {}
    }
    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}

pub fn install_dispatcher(
    hwnd: isize,
    proxy: EventLoopProxy<EventMessage>,
) -> windows::core::Result<()> {
    EVENT_PROXY.with(|p| *p.borrow_mut() = Some(proxy));
    // SAFETY: Called once on the owning UI thread, while the parent window lives.
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let win_hwnd = HWND(hwnd as *mut _);
        let style = GetWindowLongW(win_hwnd, GWL_STYLE);
        let new_style =
            style | (WS_THICKFRAME.0 | WS_MAXIMIZEBOX.0 | WS_MINIMIZEBOX.0 | WS_CAPTION.0) as i32;
        let _ = SetWindowLongW(win_hwnd, GWL_STYLE, new_style);

        let margins = MARGINS {
            cxLeftWidth: 1,
            cxRightWidth: 1,
            cyTopHeight: 1,
            cyBottomHeight: 1,
        };
        let _ = DwmExtendFrameIntoClientArea(win_hwnd, &margins);
        let _ = SetWindowPos(
            win_hwnd,
            None,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED,
        );

        register_app_hotkeys(win_hwnd);
        apply_window_theme(hwnd, false);
        SetWindowSubclass(win_hwnd, Some(dispatcher), DISPATCH_MESSAGE as usize, 0).ok()
    }
}

pub fn apply_window_theme(hwnd: isize, dark: bool) {
    unsafe {
        let win_hwnd = HWND(hwnd as *mut _);
        // DWMWA_USE_IMMERSIVE_DARK_MODE = 20
        let dark_val: i32 = if dark { 1 } else { 0 };
        let _ = DwmSetWindowAttribute(
            win_hwnd,
            DWMWINDOWATTRIBUTE(20),
            &dark_val as *const _ as *const _,
            std::mem::size_of::<i32>() as u32,
        );

        // DWMWA_WINDOW_CORNER_PREFERENCE = 33 (2 = DWMWCP_ROUND)
        let corner: u32 = 2;
        let _ = DwmSetWindowAttribute(
            win_hwnd,
            DWMWINDOWATTRIBUTE(33),
            &corner as *const _ as *const _,
            std::mem::size_of::<u32>() as u32,
        );

        // COLORREF format is 0x00BBGGRR:
        // Light paper: border #8C6941 -> 0x0041698C, caption #F2EEE5 -> 0x00E5EEF2, text #292621 -> 0x00212629
        // Dark espresso: border #536875 -> 0x00756853, caption #1E2124 -> 0x0024211E, text #F2EEE5 -> 0x00E5EEF2
        let (border_color, caption_color, text_color): (u32, u32, u32) = if dark {
            (0x00756853, 0x0024211E, 0x00E5EEF2)
        } else {
            (0x0041698C, 0x00E5EEF2, 0x00212629)
        };

        // DWMWA_BORDER_COLOR = 34
        let _ = DwmSetWindowAttribute(
            win_hwnd,
            DWMWINDOWATTRIBUTE(34),
            &border_color as *const _ as *const _,
            std::mem::size_of::<u32>() as u32,
        );

        // DWMWA_CAPTION_COLOR = 35
        let _ = DwmSetWindowAttribute(
            win_hwnd,
            DWMWINDOWATTRIBUTE(35),
            &caption_color as *const _ as *const _,
            std::mem::size_of::<u32>() as u32,
        );

        // DWMWA_TEXT_COLOR = 36
        let _ = DwmSetWindowAttribute(
            win_hwnd,
            DWMWINDOWATTRIBUTE(36),
            &text_color as *const _ as *const _,
            std::mem::size_of::<u32>() as u32,
        );
    }
}

pub fn snap_window(hwnd: isize, layout: &str) {
    unsafe {
        let win_hwnd = HWND(hwnd as *mut _);
        let hmon = MonitorFromWindow(win_hwnd, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(hmon, &mut mi).as_bool() {
            return;
        }
        let rc = mi.rcWork;
        let w = rc.right - rc.left;
        let h = rc.bottom - rc.top;

        let mut wp = WINDOWPLACEMENT {
            length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
            ..Default::default()
        };
        if GetWindowPlacement(win_hwnd, &mut wp).is_ok() && wp.showCmd == SW_MAXIMIZE.0 as u32 {
            let _ = ShowWindow(win_hwnd, SW_RESTORE);
        }

        let (x, y, tw, th) = match layout {
            "left" => (rc.left, rc.top, w / 2, h),
            "right" => (rc.left + w / 2, rc.top, w - w / 2, h),
            "top_left" => (rc.left, rc.top, w / 2, h / 2),
            "top_right" => (rc.left + w / 2, rc.top, w - w / 2, h / 2),
            "bottom_left" => (rc.left, rc.top + h / 2, w / 2, h - h / 2),
            "bottom_right" => (rc.left + w / 2, rc.top + h / 2, w - w / 2, h - h / 2),
            "left_third" => (rc.left, rc.top, w / 3, h),
            "center_third" => (rc.left + w / 3, rc.top, w / 3, h),
            "right_two_thirds" => (rc.left + w / 3, rc.top, w - w / 3, h),
            "maximize" => {
                let _ = ShowWindow(win_hwnd, SW_MAXIMIZE);
                return;
            }
            "restore" => {
                let _ = ShowWindow(win_hwnd, SW_RESTORE);
                return;
            }
            _ => return,
        };

        let _ = SetWindowPos(
            win_hwnd,
            None,
            x,
            y,
            tw,
            th,
            SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
    }
}

#[allow(dead_code)]
pub fn activate_existing_window(exclude_hwnd: isize) -> bool {
    use std::sync::atomic::{AtomicBool, Ordering};
    static ACTIVATED: AtomicBool = AtomicBool::new(false);
    ACTIVATED.store(false, Ordering::SeqCst);

    unsafe {
        use windows::core::BOOL;

        unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
            let exclude = HWND(lparam.0 as *mut _);
            if hwnd == exclude {
                return BOOL(1);
            }
            let mut text = [0u16; 256];
            let len = unsafe { GetWindowTextW(hwnd, &mut text) };
            if len > 0 {
                let title = String::from_utf16_lossy(&text[..len as usize]);
                if title.starts_with("Papyrus") {
                    unsafe {
                        let _ = ShowWindow(hwnd, SW_RESTORE);
                        let _ = SetForegroundWindow(hwnd);
                    }
                    ACTIVATED.store(true, Ordering::SeqCst);
                    return BOOL(0);
                }
            }
            BOOL(1)
        }

        let _ = EnumWindows(Some(enum_proc), LPARAM(exclude_hwnd));
    }
    ACTIVATED.load(Ordering::SeqCst)
}

pub fn show_main_menu(hwnd: isize, client_x: i32, client_y: i32) -> Option<crate::model::Command> {
    unsafe {
        let win_hwnd = HWND(hwnd as *mut _);
        let mut pt = POINT {
            x: client_x,
            y: client_y,
        };
        let _ = ClientToScreen(win_hwnd, &mut pt);

        let menu = CreatePopupMenu().ok()?;

        let items: &[(usize, &str)] = &[
            (15, "Mở Cửa sổ mới\tCtrl+N"),
            (1, "Mở Tab mới\tCtrl+T"),
            (2, "Khôi phục tab vừa đóng\tCtrl+Shift+T"),
            (0, ""), // separator
            (3, "Lịch sử duyệt web\tCtrl+H"),
            (4, "Tệp đã tải xuống\tCtrl+J"),
            (18, "Trình quản lý dấu trang\tCtrl+Shift+O"),
            (5, "Kho mật khẩu an toàn\tMật khẩu"),
            (6, "Kho tiện ích mở rộng\tStore"),
            (7, "Ghim trang hiện tại\tCtrl+D"),
            (0, ""), // separator
            (8, "Phóng to trang\tCtrl +"),
            (9, "Thu nhỏ trang\tCtrl -"),
            (10, "Thu phóng 100%\tCtrl 0"),
            (17, "Toàn màn hình\tF11"),
            (11, "In trang web\tCtrl+P"),
            (0, ""), // separator
            (12, "Công cụ nhà phát triển (DevTools)\tF12"),
            (13, "Cài đặt trình duyệt"),
            (16, "Đặt làm trình duyệt mặc định…"),
            (14, "Xóa dữ liệu duyệt web…\tCtrl+Shift+Del"),
        ];

        for &(id, label) in items {
            if id == 0 {
                let _ = AppendMenuW(menu, MF_SEPARATOR, 0, windows::core::PCWSTR::null());
            } else {
                let w_label: Vec<u16> = label.encode_utf16().chain(std::iter::once(0)).collect();
                let _ = AppendMenuW(menu, MF_STRING, id, windows::core::PCWSTR(w_label.as_ptr()));
            }
        }

        let cmd_id = TrackPopupMenuEx(
            menu,
            (TPM_LEFTALIGN | TPM_TOPALIGN | TPM_RETURNCMD).0,
            pt.x,
            pt.y,
            win_hwnd,
            None,
        )
        .0 as usize;

        let _ = DestroyMenu(menu);

        match cmd_id {
            15 => Some(crate::model::Command::NewWindow),
            1 => Some(crate::model::Command::NewTab),
            2 => Some(crate::model::Command::ReopenTab),
            3 => Some(crate::model::Command::History),
            4 => Some(crate::model::Command::Downloads),
            18 => Some(crate::model::Command::Bookmarks),
            5 => Some(crate::model::Command::Passwords),
            6 => Some(crate::model::Command::Extensions),
            7 => Some(crate::model::Command::Bookmark),
            8 => Some(crate::model::Command::ZoomIn),
            9 => Some(crate::model::Command::ZoomOut),
            10 => Some(crate::model::Command::ZoomReset),
            17 => Some(crate::model::Command::ToggleFullscreen),
            11 => Some(crate::model::Command::Print),
            12 => Some(crate::model::Command::DevTools),
            13 => Some(crate::model::Command::Settings),
            16 => Some(crate::model::Command::SetDefaultBrowser),
            14 => Some(crate::model::Command::ClearData),
            _ => None,
        }
    }
}

pub fn is_window_minimized(hwnd: isize) -> bool {
    unsafe { IsIconic(HWND(hwnd as *mut _)).as_bool() }
}

pub fn is_rect_visible_on_any_monitor(x: i32, y: i32, w: u32, h: u32) -> bool {
    let rect = RECT {
        left: x,
        top: y,
        right: x.saturating_add(w as i32),
        bottom: y.saturating_add(h as i32),
    };
    let hmon = unsafe { MonitorFromRect(&rect, MONITOR_DEFAULTTONULL) };
    !hmon.0.is_null()
}

#[allow(clippy::too_many_arguments)]
pub fn show_tab_menu(
    hwnd: isize,
    client_x: i32,
    client_y: i32,
    is_bar_menu: bool,
    can_close_others: bool,
    has_tabs_to_right: bool,
    has_closed_tabs: bool,
    is_pinned: bool,
    is_muted: bool,
) -> Option<usize> {
    unsafe {
        let win_hwnd = HWND(hwnd as *mut _);
        let mut pt = POINT {
            x: client_x,
            y: client_y,
        };
        let _ = ClientToScreen(win_hwnd, &mut pt);

        let menu = CreatePopupMenu().ok()?;

        let items: Vec<(usize, &str, bool)> = if is_bar_menu {
            vec![
                (10, "Mở Tab mới\tCtrl+T", true),
                (11, "Khôi phục tab vừa đóng\tCtrl+Shift+T", has_closed_tabs),
            ]
        } else {
            let pin_label = if is_pinned {
                "Bỏ ghim tab"
            } else {
                "Ghim tab"
            };
            let mute_label = if is_muted {
                "Bật tiếng tab\tCtrl+M"
            } else {
                "Tắt tiếng tab\tCtrl+M"
            };
            vec![
                (1, "Tải lại tab\tCtrl+R", true),
                (2, "Nhân bản tab", true),
                (7, pin_label, true),
                (8, mute_label, true),
                (6, "Ghim vào thanh dấu trang\tCtrl+D", true),
                (0, "", true), // separator
                (10, "Mở Tab mới\tCtrl+T", true),
                (11, "Khôi phục tab vừa đóng\tCtrl+Shift+T", has_closed_tabs),
                (0, "", true), // separator
                (3, "Đóng tab\tCtrl+W", true),
                (4, "Đóng các tab khác", can_close_others),
                (5, "Đóng các tab bên phải", has_tabs_to_right),
            ]
        };

        for &(id, label, enabled) in &items {
            if id == 0 {
                let _ = AppendMenuW(menu, MF_SEPARATOR, 0, windows::core::PCWSTR::null());
            } else {
                let w_label: Vec<u16> = label.encode_utf16().chain(std::iter::once(0)).collect();
                let flags = if enabled {
                    MF_STRING
                } else {
                    MF_STRING | MF_GRAYED
                };
                let _ = AppendMenuW(menu, flags, id, windows::core::PCWSTR(w_label.as_ptr()));
            }
        }

        let cmd_id = TrackPopupMenuEx(
            menu,
            (TPM_LEFTALIGN | TPM_TOPALIGN | TPM_RETURNCMD).0,
            pt.x,
            pt.y,
            win_hwnd,
            None,
        )
        .0 as usize;

        let _ = DestroyMenu(menu);

        if cmd_id > 0 { Some(cmd_id) } else { None }
    }
}

unsafe fn set_reg_str(
    hkey: HKEY,
    subkey: &str,
    name: Option<&str>,
    value: &str,
) -> windows::core::Result<()> {
    unsafe {
        let w_subkey: Vec<u16> = subkey.encode_utf16().chain(std::iter::once(0)).collect();
        let mut key = HKEY::default();
        RegCreateKeyW(hkey, PCWSTR(w_subkey.as_ptr()), &mut key).ok()?;
        let w_name: Vec<u16> = name
            .map(|n| n.encode_utf16().chain(std::iter::once(0)).collect())
            .unwrap_or_default();
        let name_ptr = if name.is_some() {
            PCWSTR(w_name.as_ptr())
        } else {
            PCWSTR::null()
        };
        let w_val: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
        let val_bytes = std::slice::from_raw_parts(
            w_val.as_ptr() as *const u8,
            w_val.len() * std::mem::size_of::<u16>(),
        );
        RegSetValueExW(key, name_ptr, None, REG_SZ, Some(val_bytes)).ok()?;
        let _ = RegCloseKey(key);
        Ok(())
    }
}

pub fn register_as_default_browser() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe_str = exe.to_str().ok_or("Đường dẫn exe không hợp lệ")?;
    let quoted_exe = format!("\"{exe_str}\"");
    let open_cmd = format!("\"{exe_str}\" \"%1\"");
    let ico_path = format!("\"{exe_str}\",0");

    unsafe {
        let root = "Software\\Clients\\StartMenuInternet\\Papyrus";
        set_reg_str(HKEY_CURRENT_USER, root, None, "Papyrus Browser").map_err(|e| e.to_string())?;
        set_reg_str(
            HKEY_CURRENT_USER,
            &format!("{root}\\DefaultIcon"),
            None,
            &ico_path,
        )
        .map_err(|e| e.to_string())?;
        set_reg_str(
            HKEY_CURRENT_USER,
            &format!("{root}\\shell\\open\\command"),
            None,
            &quoted_exe,
        )
        .map_err(|e| e.to_string())?;

        let cap = format!("{root}\\Capabilities");
        set_reg_str(HKEY_CURRENT_USER, &cap, Some("ApplicationName"), "Papyrus")
            .map_err(|e| e.to_string())?;
        set_reg_str(
            HKEY_CURRENT_USER,
            &cap,
            Some("ApplicationDescription"),
            "Papyrus Browser - 100% Rust & WebView2 Core",
        )
        .map_err(|e| e.to_string())?;
        set_reg_str(HKEY_CURRENT_USER, &cap, Some("ApplicationIcon"), &ico_path)
            .map_err(|e| e.to_string())?;

        let file_assoc = format!("{cap}\\FileAssociations");
        set_reg_str(HKEY_CURRENT_USER, &file_assoc, Some(".htm"), "PapyrusHTML")
            .map_err(|e| e.to_string())?;
        set_reg_str(HKEY_CURRENT_USER, &file_assoc, Some(".html"), "PapyrusHTML")
            .map_err(|e| e.to_string())?;
        set_reg_str(HKEY_CURRENT_USER, &file_assoc, Some(".pdf"), "PapyrusPDF")
            .map_err(|e| e.to_string())?;

        let url_assoc = format!("{cap}\\URLAssociations");
        set_reg_str(HKEY_CURRENT_USER, &url_assoc, Some("http"), "PapyrusURL")
            .map_err(|e| e.to_string())?;
        set_reg_str(HKEY_CURRENT_USER, &url_assoc, Some("https"), "PapyrusURL")
            .map_err(|e| e.to_string())?;

        set_reg_str(
            HKEY_CURRENT_USER,
            "Software\\RegisteredApplications",
            Some("Papyrus"),
            &cap,
        )
        .map_err(|e| e.to_string())?;

        for prog_id in &["PapyrusHTML", "PapyrusPDF", "PapyrusURL"] {
            let class_root = format!("Software\\Classes\\{prog_id}");
            set_reg_str(HKEY_CURRENT_USER, &class_root, None, "Papyrus Document")
                .map_err(|e| e.to_string())?;
            set_reg_str(
                HKEY_CURRENT_USER,
                &format!("{class_root}\\DefaultIcon"),
                None,
                &ico_path,
            )
            .map_err(|e| e.to_string())?;
            set_reg_str(
                HKEY_CURRENT_USER,
                &format!("{class_root}\\shell\\open\\command"),
                None,
                &open_cmd,
            )
            .map_err(|e| e.to_string())?;
        }

        let settings_url: Vec<u16> = "ms-settings:defaultapps?registeredAppMachineOrUser=Papyrus"
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let op: Vec<u16> = "open".encode_utf16().chain(std::iter::once(0)).collect();
        let res = ShellExecuteW(
            None,
            PCWSTR(op.as_ptr()),
            PCWSTR(settings_url.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        );
        if res.0 as usize <= 32 {
            let fallback: Vec<u16> = "ms-settings:defaultapps"
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
            let _ = ShellExecuteW(
                None,
                PCWSTR(op.as_ptr()),
                PCWSTR(fallback.as_ptr()),
                PCWSTR::null(),
                PCWSTR::null(),
                SW_SHOWNORMAL,
            );
        }
    }
    Ok(())
}

pub fn show_snap_menu(hwnd: isize, client_x: i32, client_y: i32) -> Option<String> {
    unsafe {
        let win_hwnd = HWND(hwnd as *mut _);
        let mut pt = POINT {
            x: client_x,
            y: client_y,
        };
        let _ = ClientToScreen(win_hwnd, &mut pt);

        let menu = CreatePopupMenu().ok()?;

        let items: &[(usize, &str)] = &[
            (1, "Chia đôi: Nửa trái (50%)\tWin+Left"),
            (2, "Chia đôi: Nửa phải (50%)\tWin+Right"),
            (0, ""), // separator
            (3, "Góc trên bên trái (25%)"),
            (4, "Góc trên bên phải (25%)"),
            (5, "Góc dưới bên trái (25%)"),
            (6, "Góc dưới bên phải (25%)"),
            (0, ""), // separator
            (7, "Chia ba: Cột bên trái (33%)"),
            (8, "Chia ba: Cột ở giữa (33%)"),
            (9, "Chia ba: Cột bên phải (33%)"),
        ];

        for &(id, label) in items {
            if id == 0 {
                let _ = AppendMenuW(menu, MF_SEPARATOR, 0, windows::core::PCWSTR::null());
            } else {
                let w_label: Vec<u16> = label.encode_utf16().chain(std::iter::once(0)).collect();
                let _ = AppendMenuW(menu, MF_STRING, id, windows::core::PCWSTR(w_label.as_ptr()));
            }
        }

        let cmd_id = TrackPopupMenuEx(
            menu,
            (TPM_LEFTALIGN | TPM_TOPALIGN | TPM_RETURNCMD).0,
            pt.x,
            pt.y,
            win_hwnd,
            None,
        )
        .0 as usize;

        let _ = DestroyMenu(menu);

        match cmd_id {
            1 => Some("left".into()),
            2 => Some("right".into()),
            3 => Some("top_left".into()),
            4 => Some("top_right".into()),
            5 => Some("bottom_left".into()),
            6 => Some("bottom_right".into()),
            7 => Some("third_left".into()),
            8 => Some("third_center".into()),
            9 => Some("third_right".into()),
            _ => None,
        }
    }
}

pub fn dispatch(hwnd: isize, task: impl FnOnce() + 'static) -> windows::core::Result<()> {
    // Post before enqueueing: same-thread dispatch cannot execute until we return.
    unsafe {
        PostMessageW(
            Some(HWND(hwnd as *mut _)),
            DISPATCH_MESSAGE,
            WPARAM(0),
            LPARAM(0),
        )?;
    }
    TASKS.with(|tasks| tasks.borrow_mut().push_back(Box::new(task)));
    Ok(())
}

pub struct PendingPopup {
    pub args: ICoreWebView2NewWindowRequestedEventArgs,
    pub deferral: ICoreWebView2Deferral,
    pub environment: ICoreWebView2Environment,
}
pub type Popups = Rc<RefCell<Vec<PendingPopup>>>;

pub fn core(view: &WebView) -> windows::core::Result<ICoreWebView2> {
    // SAFETY: Wry creates the controller on this UI thread; it outlives this call.
    unsafe { view.controller().CoreWebView2() }
}

pub fn environment(view: &WebView) -> windows::core::Result<ICoreWebView2Environment> {
    // SAFETY: Interface query and COM access are on the controller's owning thread.
    unsafe { core(view)?.cast::<ICoreWebView2_2>()?.Environment() }
}

pub fn source(core: &ICoreWebView2) -> windows::core::Result<String> {
    let mut uri = PWSTR::null();
    // SAFETY: WebView2 allocates the output; take_pwstr frees it with CoTaskMemFree.
    unsafe {
        core.Source(&mut uri)?;
    }
    Ok(take_pwstr(uri))
}

pub fn history(view: &WebView) -> windows::core::Result<(bool, bool)> {
    let (mut back, mut forward) = Default::default();
    // SAFETY: Both pointers refer to live BOOL values on the UI thread.
    unsafe {
        let core = core(view)?;
        core.CanGoBack(&mut back)?;
        core.CanGoForward(&mut forward)?;
    }
    Ok((back.as_bool(), forward.as_bool()))
}

pub fn navigate_history(view: &WebView, forward: bool) -> windows::core::Result<()> {
    // SAFETY: UI-thread COM methods, with capability checked before navigation.
    unsafe {
        let (back, next) = history(view)?;
        if forward && next {
            core(view)?.GoForward()?;
        } else if !forward && back {
            core(view)?.GoBack()?;
        }
    }
    Ok(())
}

pub fn stop(view: &WebView) -> windows::core::Result<()> {
    // SAFETY: UI-thread COM access.
    unsafe { core(view)?.Stop() }
}

pub fn reload(view: &WebView) -> windows::core::Result<()> {
    // SAFETY: UI-thread COM access.
    unsafe { core(view)?.Reload() }
}

pub fn close_view(view: &WebView) -> windows::core::Result<()> {
    // SAFETY: UI-thread COM invocation. Explicitly closing the controller destroys
    // the child visual tree and releases resources immediately.
    unsafe { view.controller().Close() }
}

pub fn suspend_view(view: &WebView) -> windows::core::Result<()> {
    // SAFETY: UI-thread COM invocation. Calling TrySuspend stops timers, frees renderer memory,
    // and suspends background tabs until they are brought to focus.
    unsafe {
        if let Ok(core) = core(view) {
            if let Ok(core3) = core.cast::<ICoreWebView2_3>() {
                let handler = TrySuspendCompletedHandler::create(Box::new(|_, _| Ok(())));
                let _ = core3.TrySuspend(&handler);
            }
        }
    }
    Ok(())
}

pub fn resume_view(view: &WebView) -> windows::core::Result<()> {
    // SAFETY: UI-thread COM invocation. Resumes suspended WebView2 instance instantly.
    unsafe {
        if let Ok(core) = core(view) {
            if let Ok(core3) = core.cast::<ICoreWebView2_3>() {
                let _ = core3.Resume();
            }
        }
    }
    Ok(())
}

pub fn open_devtools(view: &WebView) -> windows::core::Result<()> {
    // SAFETY: UI-thread COM access.
    unsafe { core(view)?.OpenDevToolsWindow() }
}

#[allow(dead_code)]
pub fn is_muted(view: &WebView) -> windows::core::Result<bool> {
    unsafe {
        let web8 = core(view)?.cast::<ICoreWebView2_8>()?;
        let mut muted = Default::default();
        web8.IsMuted(&mut muted)?;
        Ok(muted.as_bool())
    }
}

pub fn set_muted(view: &WebView, mute: bool) -> windows::core::Result<()> {
    unsafe {
        let web8 = core(view)?.cast::<ICoreWebView2_8>()?;
        web8.SetIsMuted(mute)
    }
}

#[allow(dead_code)]
pub fn is_document_playing_audio(view: &WebView) -> windows::core::Result<bool> {
    unsafe {
        let web8 = core(view)?.cast::<ICoreWebView2_8>()?;
        let mut playing = Default::default();
        web8.IsDocumentPlayingAudio(&mut playing)?;
        Ok(playing.as_bool())
    }
}

pub fn clear_data(
    view: &WebView,
    proxy: EventLoopProxy<EventMessage>,
) -> windows::core::Result<()> {
    // SAFETY: COM owns the callback until completion; proxy contains no COM handles.
    unsafe {
        core(view)?
            .cast::<ICoreWebView2_13>()?
            .Profile()?
            .cast::<ICoreWebView2Profile2>()?
            .ClearBrowsingDataAll(&ClearBrowsingDataCompletedHandler::create(Box::new(
                move |result| {
                    let result = result.map_err(|e| e.to_string());
                    let _ = proxy.send_event(EventMessage::ClearCompleted(result));
                    Ok(())
                },
            )))
    }
}

pub fn attach_accelerators(
    view: &WebView,
    id: u64,
    proxy: EventLoopProxy<EventMessage>,
) -> windows::core::Result<()> {
    unsafe {
        let key_proxy = proxy.clone();
        view.controller().add_AcceleratorKeyPressed(
            &AcceleratorKeyPressedEventHandler::create(Box::new(move |_, args| {
                let Some(args) = args else {
                    return Ok(());
                };
                let mut kind = COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN;
                args.KeyEventKind(&mut kind)?;
                if kind != COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN
                    && kind != COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN
                {
                    return Ok(());
                }
                let mut key = 0;
                args.VirtualKey(&mut key)?;
                use windows::Win32::UI::Input::KeyboardAndMouse::{
                    GetAsyncKeyState, GetKeyState, VK_CONTROL, VK_MENU, VK_SHIFT,
                };
                let ctrl = (GetKeyState(VK_CONTROL.0 as i32) < 0)
                    || (GetAsyncKeyState(VK_CONTROL.0 as i32) < 0);
                let alt =
                    (GetKeyState(VK_MENU.0 as i32) < 0) || (GetAsyncKeyState(VK_MENU.0 as i32) < 0);
                let shift = (GetKeyState(VK_SHIFT.0 as i32) < 0)
                    || (GetAsyncKeyState(VK_SHIFT.0 as i32) < 0);
                let command = match key {
                    0x4c if ctrl => Some(crate::model::Command::FocusAddress),
                    0x44 if alt => Some(crate::model::Command::FocusAddress),
                    0x75 => Some(crate::model::Command::FocusAddress),
                    0x54 if ctrl && shift => Some(crate::model::Command::ReopenTab),
                    0x54 if ctrl => Some(crate::model::Command::NewTab),
                    0x4e if ctrl => Some(crate::model::Command::NewWindow),
                    0x57 if ctrl => Some(crate::model::Command::CloseTab { id }),
                    0x44 if ctrl => Some(crate::model::Command::Bookmark),
                    0x4d if ctrl => Some(crate::model::Command::ToggleMuteTab { id }),
                    0x48 if ctrl => Some(crate::model::Command::History),
                    0x4a if ctrl => Some(crate::model::Command::Downloads),
                    0x50 if ctrl => Some(crate::model::Command::Print),
                    0x52 if ctrl => Some(crate::model::Command::Reload),
                    0x74 => Some(crate::model::Command::Reload), // F5
                    0x7a => Some(crate::model::Command::ToggleFullscreen), // F11
                    0x7b => Some(crate::model::Command::DevTools), // F12
                    0x49 if ctrl && shift => Some(crate::model::Command::DevTools), // Ctrl+Shift+I
                    0x4a if ctrl && shift => Some(crate::model::Command::DevTools), // Ctrl+Shift+J
                    0x43 if ctrl && shift => Some(crate::model::Command::DevTools), // Ctrl+Shift+C
                    0xbb if ctrl => Some(crate::model::Command::ZoomIn), // Ctrl + +
                    0x6b if ctrl => Some(crate::model::Command::ZoomIn), // Ctrl + Numpad +
                    0xbd if ctrl => Some(crate::model::Command::ZoomOut), // Ctrl + -
                    0x6d if ctrl => Some(crate::model::Command::ZoomOut), // Ctrl + Numpad -
                    0x30 if ctrl => Some(crate::model::Command::ZoomReset), // Ctrl + 0
                    0x60 if ctrl => Some(crate::model::Command::ZoomReset), // Ctrl + Numpad 0
                    0x09 if ctrl && shift => Some(crate::model::Command::PrevTab), // Ctrl+Shift+Tab
                    0x09 if ctrl => Some(crate::model::Command::NextTab), // Ctrl+Tab
                    0x21 if ctrl => Some(crate::model::Command::PrevTab), // Ctrl+PageUp
                    0x22 if ctrl => Some(crate::model::Command::NextTab), // Ctrl+PageDown
                    0x31..=0x38 if ctrl => Some(crate::model::Command::SelectTabByIndex {
                        index: (key - 0x31) as usize,
                    }),
                    0x39 if ctrl => Some(crate::model::Command::SelectLastTab),
                    0x2e if ctrl && shift => Some(crate::model::Command::ClearData), // Ctrl+Shift+Del
                    0x25 if alt => Some(crate::model::Command::Back),
                    0x27 if alt => Some(crate::model::Command::Forward),
                    _ => None,
                };
                if let Some(command) = command {
                    args.SetHandled(true)?;
                    let _ = key_proxy.send_event(EventMessage::Command(command));
                }
                Ok(())
            })),
            &mut 0,
        )?;
        let settings = core(view)?.Settings()?;
        if let Ok(settings3) = settings.cast::<ICoreWebView2Settings3>() {
            let _ = settings3.SetAreBrowserAcceleratorKeysEnabled(false);
        }
        Ok(())
    }
}

pub fn setup_header(
    view: &WebView,
    proxy: EventLoopProxy<EventMessage>,
) -> windows::core::Result<()> {
    attach_accelerators(view, 0, proxy)
}

pub fn map_resource_context(
    context: COREWEBVIEW2_WEB_RESOURCE_CONTEXT,
    uri: &str,
    top: &str,
) -> &'static str {
    match context {
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_IMAGE => "image",
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_SCRIPT => "script",
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_STYLESHEET => "stylesheet",
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_XML_HTTP_REQUEST
        | COREWEBVIEW2_WEB_RESOURCE_CONTEXT_FETCH
        | COREWEBVIEW2_WEB_RESOURCE_CONTEXT_EVENT_SOURCE => "xmlhttprequest",
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_MEDIA | COREWEBVIEW2_WEB_RESOURCE_CONTEXT_TEXT_TRACK => {
            "media"
        }
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_FONT => "font",
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_WEBSOCKET => "websocket",
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_PING => "ping",
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_CSP_VIOLATION_REPORT => "csp_report",
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_DOCUMENT => {
            if !top.is_empty() && top != "about:blank" && uri != top {
                "subdocument"
            } else {
                "main_frame"
            }
        }
        _ => "other",
    }
}

pub fn attach(
    view: &WebView,
    id: u64,
    prefs: SharedPreferences,
    popups: Popups,
    proxy: EventLoopProxy<EventMessage>,
) -> windows::core::Result<()> {
    let web = core(view)?;
    let env = environment(view)?;
    attach_accelerators(view, id, proxy.clone())?;
    // SAFETY: Every interface below belongs to this UI thread. WebView2 retains event
    // handlers; handlers use their sender instead of retaining their own WebView.
    unsafe {
        let settings = web.Settings()?;
        settings.SetIsWebMessageEnabled(false)?;
        settings.SetAreHostObjectsAllowed(false)?;
        settings.SetIsStatusBarEnabled(true)?;
        settings.SetAreDefaultContextMenusEnabled(true)?;
        if let Ok(web22) = web.cast::<ICoreWebView2_22>() {
            web22.AddWebResourceRequestedFilterWithRequestSourceKinds(
                w!("*"),
                COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
                COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_ALL,
            )?;
        } else {
            web.AddWebResourceRequestedFilter(w!("*"), COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL)?;
        }

        let resource_prefs = prefs.clone();
        let blocked_proxy = proxy.clone();
        web.add_WebResourceRequested(
            &WebResourceRequestedEventHandler::create(Box::new(move |sender, args| {
                if !resource_prefs.borrow().adblock {
                    return Ok(());
                }
                let (Some(sender), Some(args)) = (sender, args) else {
                    return Ok(());
                };
                let req = match args.Request() {
                    Ok(r) => r,
                    Err(_) => return Ok(()),
                };
                let mut raw_uri = PWSTR::null();
                if req.Uri(&mut raw_uri).is_err() {
                    return Ok(());
                }
                let uri = take_pwstr(raw_uri);
                if uri.is_empty() {
                    return Ok(());
                }

                let mut raw_method = PWSTR::null();
                let method = if req.Method(&mut raw_method).is_ok() {
                    let m = take_pwstr(raw_method);
                    if m.is_empty() { "GET".to_string() } else { m }
                } else {
                    "GET".to_string()
                };

                let top = source(&sender).unwrap_or_default();

                let mut raw_context = COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL;
                let _ = args.ResourceContext(&mut raw_context);
                let resource_type = map_resource_context(raw_context, &uri, &top);

                if should_block(&uri, &top, resource_type, &method, &resource_prefs.borrow()) {
                    let response = env.CreateWebResourceResponse(
                        None::<&IStream>,
                        403,
                        w!("Blocked by Papyrus Adblock"),
                        w!("Content-Type: text/plain\r\nCache-Control: no-store\r\n"),
                    )?;
                    args.SetResponse(&response)?;
                    let _ = blocked_proxy.send_event(EventMessage::Blocked(id));
                }
                Ok(())
            })),
            &mut 0,
        )?;

        let source_proxy = proxy.clone();
        web.add_SourceChanged(
            &SourceChangedEventHandler::create(Box::new(move |sender, _| {
                if let Some(sender) = sender {
                    let _ = source_proxy.send_event(EventMessage::Source(id, source(&sender)?));
                }
                Ok(())
            })),
            &mut 0,
        )?;

        let history_proxy = proxy.clone();
        web.add_HistoryChanged(
            &HistoryChangedEventHandler::create(Box::new(move |_, _| {
                let _ = history_proxy.send_event(EventMessage::History);
                Ok(())
            })),
            &mut 0,
        )?;

        let start_proxy = proxy.clone();
        web.add_NavigationStarting(
            &NavigationStartingEventHandler::create(Box::new(move |_, args| {
                if let Some(args) = args {
                    let mut canceled = Default::default();
                    args.Cancel(&mut canceled)?;
                    if !canceled.as_bool() {
                        let mut navigation = 0;
                        args.NavigationId(&mut navigation)?;
                        let _ = start_proxy.send_event(EventMessage::Loading(id, navigation));
                    }
                }
                Ok(())
            })),
            &mut 0,
        )?;
        let load_proxy = proxy.clone();
        web.add_NavigationCompleted(
            &NavigationCompletedEventHandler::create(Box::new(move |_, args| {
                if let Some(args) = args {
                    let mut success = Default::default();
                    args.IsSuccess(&mut success)?;
                    let error = if success.as_bool() {
                        None
                    } else {
                        let mut code = COREWEBVIEW2_WEB_ERROR_STATUS_UNKNOWN;
                        args.WebErrorStatus(&mut code)?;
                        if code == COREWEBVIEW2_WEB_ERROR_STATUS_OPERATION_CANCELED {
                            None
                        } else { Some(format!(
                            "Không tải được trang (WebView2 {}). Kiểm tra mạng hoặc thử tải lại.",
                            code.0
                        )) }
                    };
                    let mut navigation = 0;
                    args.NavigationId(&mut navigation)?;
                    let _ = load_proxy.send_event(EventMessage::Finished(id, navigation, error));
                }
                Ok(())
            })),
            &mut 0,
        )?;

        let popup_proxy = proxy.clone();
        web.add_NewWindowRequested(
            &NewWindowRequestedEventHandler::create(Box::new(move |sender, args| {
                let (Some(sender), Some(args)) = (sender, args) else {
                    return Ok(());
                };
                args.SetHandled(true)?;
                let mut user_initiated = Default::default();
                args.IsUserInitiated(&mut user_initiated)?;
                if prefs.borrow().block_popups && !user_initiated.as_bool() {
                    let _ = popup_proxy.send_event(EventMessage::Notice(
                        "Đã chặn cửa sổ bật lên tự động. Có thể cho phép trong Cài đặt.".into(),
                    ));
                    return Ok(());
                }
                let deferral = args.GetDeferral()?;
                let environment = sender.cast::<ICoreWebView2_2>()?.Environment()?;
                popups.borrow_mut().push(PendingPopup {
                    args,
                    deferral,
                    environment,
                });
                let _ = popup_proxy.send_event(EventMessage::Popup);
                Ok(())
            })),
            &mut 0,
        )?;

        let close_proxy = proxy.clone();
        web.add_WindowCloseRequested(
            &WindowCloseRequestedEventHandler::create(Box::new(move |_, _| {
                let _ = close_proxy.send_event(EventMessage::CloseTab(id));
                Ok(())
            })),
            &mut 0,
        )?;

        if let Ok(web8) = web.cast::<ICoreWebView2_8>() {
            let audio_proxy = proxy.clone();
            let _ = web8.add_IsDocumentPlayingAudioChanged(
                &IsDocumentPlayingAudioChangedEventHandler::create(Box::new(move |sender, _| {
                    if let Some(sender) = sender
                        && let Ok(web8) = sender.cast::<ICoreWebView2_8>()
                    {
                        let mut playing = Default::default();
                        let _ = web8.IsDocumentPlayingAudio(&mut playing);
                        let _ = audio_proxy
                            .send_event(EventMessage::AudioPlaying(id, playing.as_bool()));
                    }
                    Ok(())
                })),
                &mut 0,
            );

            let muted_proxy = proxy.clone();
            let _ = web8.add_IsMutedChanged(
                &IsMutedChangedEventHandler::create(Box::new(move |sender, _| {
                    if let Some(sender) = sender
                        && let Ok(web8) = sender.cast::<ICoreWebView2_8>()
                    {
                        let mut muted = Default::default();
                        let _ = web8.IsMuted(&mut muted);
                        let _ = muted_proxy.send_event(EventMessage::Muted(id, muted.as_bool()));
                    }
                    Ok(())
                })),
                &mut 0,
            );
        }

        let crash_proxy = proxy.clone();
        web.add_ProcessFailed(
            &ProcessFailedEventHandler::create(Box::new(move |_sender, args| {
                let mut kind = COREWEBVIEW2_PROCESS_FAILED_KIND_UNKNOWN_PROCESS_EXITED;
                if let Some(args) = &args {
                    let _ = args.ProcessFailedKind(&mut kind);
                }
                let (is_fatal, desc) = match kind {
                    COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED => {
                        (true, "Tiến trình WebView2 đã kết thúc đột ngột.")
                    }
                    COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED => (
                        true,
                        "Tiến trình hiển thị trang web đã bị dừng đột ngột (Crash).",
                    ),
                    COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_UNRESPONSIVE => {
                        (true, "Trang web không phản hồi (Unresponsive).")
                    }
                    COREWEBVIEW2_PROCESS_FAILED_KIND_FRAME_RENDER_PROCESS_EXITED => {
                        (true, "Khung hiển thị (iframe) của trang web bị lỗi dừng.")
                    }
                    COREWEBVIEW2_PROCESS_FAILED_KIND_GPU_PROCESS_EXITED => (
                        false,
                        "Tiến trình đồ họa (GPU) gặp sự cố và đang tự khôi phục.",
                    ),
                    COREWEBVIEW2_PROCESS_FAILED_KIND_UTILITY_PROCESS_EXITED => {
                        (false, "Tiến trình tiện ích WebView2 gặp sự cố.")
                    }
                    _ => (true, "Tiến trình nội bộ WebView2 gặp sự cố không xác định."),
                };

                let mut detail = String::new();
                if let Some(args) = args
                    && let Ok(args2) = args.cast::<ICoreWebView2ProcessFailedEventArgs2>()
                {
                    let mut code = 0i32;
                    if args2.ExitCode(&mut code).is_ok() {
                        detail.push_str(&format!(", mã thoát: 0x{code:08X}"));
                    }
                    let mut reason = COREWEBVIEW2_PROCESS_FAILED_REASON(0);
                    if args2.Reason(&mut reason).is_ok() {
                        let r_str = match reason {
                            COREWEBVIEW2_PROCESS_FAILED_REASON_CRASHED => "Sự cố crash",
                            COREWEBVIEW2_PROCESS_FAILED_REASON_OUT_OF_MEMORY => "Hết bộ nhớ (OOM)",
                            COREWEBVIEW2_PROCESS_FAILED_REASON_TERMINATED => "Bị dừng bởi hệ thống",
                            COREWEBVIEW2_PROCESS_FAILED_REASON_UNRESPONSIVE => {
                                "Treo / Không phản hồi"
                            }
                            _ => "Khác",
                        };
                        detail.push_str(&format!(", lý do: {r_str}"));
                    }
                }

                eprintln!(
                    "[WebView2 ProcessFailed] Tab {id}: {desc}{detail} (kind={})",
                    kind.0
                );

                if is_fatal {
                    let user_msg = format!("{desc} Nhấn Tải lại để khôi phục.");
                    let _ =
                        crash_proxy.send_event(EventMessage::ProcessFailed(id, kind.0, user_msg));
                }
                Ok(())
            })),
            &mut 0,
        )?;
    }
    Ok(())
}
