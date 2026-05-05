use crate::{TrayCommand, TrayLabels};
use std::{
    mem::size_of,
    ptr::{null, null_mut},
    sync::{
        Arc,
        atomic::{AtomicIsize, Ordering},
        mpsc::{self, Receiver, Sender, SyncSender},
    },
    thread::{self, JoinHandle},
};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM},
    System::LibraryLoader::GetModuleHandleW,
    UI::{
        Shell::{
            NIF_ICON, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
            NIM_SETVERSION, NOTIFYICON_VERSION_4, NOTIFYICONDATAW, Shell_NotifyIconW,
        },
        WindowsAndMessaging::{
            AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu,
            DestroyWindow, DispatchMessageW, GWLP_USERDATA, GetCursorPos, GetMessageW,
            GetWindowLongPtrW, HMENU, IDI_APPLICATION, LoadIconW, MF_SEPARATOR, MF_STRING, MSG,
            PostMessageW, PostQuitMessage, RegisterClassW, SetForegroundWindow, SetWindowLongPtrW,
            TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu, TranslateMessage, WM_APP, WM_COMMAND,
            WM_CONTEXTMENU, WM_DESTROY, WM_LBUTTONDBLCLK, WM_LBUTTONUP, WM_RBUTTONUP, WNDCLASSW,
            WS_OVERLAPPED,
        },
    },
};

const TRAY_ID: u32 = 1;
const WM_TRAY_ICON: u32 = WM_APP + 1;
const WM_TRAY_CONTROL: u32 = WM_APP + 2;

const CMD_OPEN: usize = 1001;
const CMD_SETTINGS: usize = 1002;
const CMD_HOTKEYS: usize = 1003;
const CMD_UTILITIES: usize = 1004;
const CMD_CAPTURE: usize = 1005;
const CMD_SYNC: usize = 1006;
const CMD_NEW_CLIP: usize = 1007;
const CMD_DELETE_NON_PINNED: usize = 1008;
const CMD_TOGGLE_CAPTURE: usize = 1009;
const CMD_EXIT: usize = 1010;

pub(crate) struct WindowsTrayHandle {
    control_tx: Sender<WindowsTrayControl>,
    hwnd: Arc<AtomicIsize>,
    thread: Option<JoinHandle<()>>,
}

impl WindowsTrayHandle {
    pub(crate) fn spawn(
        sender: Sender<TrayCommand>,
        labels: TrayLabels,
        capture_enabled: bool,
    ) -> Result<Self, String> {
        let (control_tx, control_rx) = mpsc::channel();
        let hwnd = Arc::new(AtomicIsize::new(0));
        let thread_hwnd = Arc::clone(&hwnd);
        let (init_tx, init_rx) = mpsc::sync_channel(1);

        let thread = thread::Builder::new()
            .name("yank-windows-tray".to_owned())
            .spawn(move || {
                run_tray_thread(
                    sender,
                    control_rx,
                    labels,
                    capture_enabled,
                    thread_hwnd,
                    init_tx,
                );
            })
            .map_err(|error| error.to_string())?;

        match init_rx.recv().map_err(|error| error.to_string())? {
            Ok(()) => Ok(Self {
                control_tx,
                hwnd,
                thread: Some(thread),
            }),
            Err(error) => {
                let _ = thread.join();
                Err(error)
            }
        }
    }

    pub(crate) fn update(&self, labels: TrayLabels, capture_enabled: bool) {
        let _ = self.control_tx.send(WindowsTrayControl::Update {
            labels: Box::new(labels),
            capture_enabled,
        });
        self.post_control_message();
    }

    pub(crate) fn shutdown(mut self) {
        self.request_shutdown();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }

    fn request_shutdown(&self) {
        let _ = self.control_tx.send(WindowsTrayControl::Shutdown);
        self.post_control_message();
    }

    fn post_control_message(&self) {
        let hwnd = self.hwnd.load(Ordering::SeqCst);
        if hwnd != 0 {
            unsafe {
                PostMessageW(hwnd as HWND, WM_TRAY_CONTROL, 0, 0);
            }
        }
    }
}

impl Drop for WindowsTrayHandle {
    fn drop(&mut self) {
        self.request_shutdown();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

enum WindowsTrayControl {
    Update {
        labels: Box<TrayLabels>,
        capture_enabled: bool,
    },
    Shutdown,
}

struct WindowsTrayRuntime {
    sender: Sender<TrayCommand>,
    control_rx: Receiver<WindowsTrayControl>,
    labels: TrayLabels,
    capture_enabled: bool,
}

fn run_tray_thread(
    sender: Sender<TrayCommand>,
    control_rx: Receiver<WindowsTrayControl>,
    labels: TrayLabels,
    capture_enabled: bool,
    hwnd_slot: Arc<AtomicIsize>,
    init_tx: SyncSender<Result<(), String>>,
) {
    let class_name = to_wide("YankTrayWindow");
    let window_name = to_wide(&labels.title);
    let hinstance = unsafe { GetModuleHandleW(null()) };
    if hinstance.is_null() {
        let _ = init_tx.send(Err("GetModuleHandleW failed".to_owned()));
        return;
    }

    let icon = unsafe { LoadIconW(null_mut(), IDI_APPLICATION) };
    let window_class = WNDCLASSW {
        lpfnWndProc: Some(tray_wnd_proc),
        hInstance: hinstance,
        hIcon: icon,
        lpszClassName: class_name.as_ptr(),
        ..Default::default()
    };
    unsafe {
        RegisterClassW(&window_class);
    }

    let hwnd = unsafe {
        CreateWindowExW(
            0,
            class_name.as_ptr(),
            window_name.as_ptr(),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            null_mut(),
            null_mut(),
            hinstance,
            null(),
        )
    };
    if hwnd.is_null() {
        let _ = init_tx.send(Err("CreateWindowExW failed".to_owned()));
        return;
    }

    let runtime = Box::new(WindowsTrayRuntime {
        sender,
        control_rx,
        labels,
        capture_enabled,
    });
    let runtime_ptr = Box::into_raw(runtime);
    unsafe {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, runtime_ptr as isize);
    }

    let title = unsafe { (*runtime_ptr).labels.title.clone() };
    if !add_tray_icon(hwnd, icon, &title) {
        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            let _ = Box::from_raw(runtime_ptr);
            DestroyWindow(hwnd);
        }
        let _ = init_tx.send(Err("Shell_NotifyIconW failed".to_owned()));
        return;
    }

    hwnd_slot.store(hwnd as isize, Ordering::SeqCst);
    let _ = init_tx.send(Ok(()));

    let mut msg = MSG::default();
    while unsafe { GetMessageW(&mut msg, null_mut(), 0, 0) } > 0 {
        unsafe {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    hwnd_slot.store(0, Ordering::SeqCst);
    unsafe {
        remove_tray_icon(hwnd);
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowsTrayRuntime;
        if !ptr.is_null() {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            let _ = Box::from_raw(ptr);
        }
    }
}

unsafe extern "system" fn tray_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_TRAY_ICON => {
            let event = lparam as u32;
            if event == WM_LBUTTONUP || event == WM_LBUTTONDBLCLK {
                if let Some(runtime) = unsafe { runtime_mut(hwnd) } {
                    let _ = runtime.sender.send(TrayCommand::Open);
                }
                0
            } else if event == WM_CONTEXTMENU || event == WM_RBUTTONUP {
                if let Some(runtime) = unsafe { runtime_mut(hwnd) } {
                    show_context_menu(hwnd, runtime);
                }
                0
            } else {
                0
            }
        }
        WM_TRAY_CONTROL => {
            if let Some(runtime) = unsafe { runtime_mut(hwnd) } {
                drain_control_messages(hwnd, runtime);
            }
            0
        }
        WM_COMMAND => {
            if let Some(runtime) = unsafe { runtime_mut(hwnd) } {
                dispatch_menu_command(wparam & 0xffff, runtime);
            }
            0
        }
        WM_DESTROY => {
            unsafe {
                PostQuitMessage(0);
            }
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

unsafe fn runtime_mut(hwnd: HWND) -> Option<&'static mut WindowsTrayRuntime> {
    let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut WindowsTrayRuntime;
    if ptr.is_null() {
        None
    } else {
        Some(unsafe { &mut *ptr })
    }
}

fn drain_control_messages(hwnd: HWND, runtime: &mut WindowsTrayRuntime) {
    while let Ok(message) = runtime.control_rx.try_recv() {
        match message {
            WindowsTrayControl::Update {
                labels,
                capture_enabled,
            } => {
                runtime.labels = *labels;
                runtime.capture_enabled = capture_enabled;
                update_tray_tip(hwnd, &runtime.labels.title);
            }
            WindowsTrayControl::Shutdown => unsafe {
                DestroyWindow(hwnd);
                break;
            },
        }
    }
}

fn show_context_menu(hwnd: HWND, runtime: &mut WindowsTrayRuntime) {
    let menu = unsafe { CreatePopupMenu() };
    if menu.is_null() {
        return;
    }

    append_menu_item(menu, CMD_OPEN, &runtime.labels.open);
    append_menu_item(menu, CMD_SETTINGS, &runtime.labels.options);
    append_menu_item(menu, CMD_HOTKEYS, &runtime.labels.hotkeys);
    append_menu_item(menu, CMD_UTILITIES, &runtime.labels.utilities);
    append_separator(menu);
    append_menu_item(menu, CMD_CAPTURE, &runtime.labels.capture);
    append_menu_item(menu, CMD_SYNC, &runtime.labels.sync);
    append_menu_item(menu, CMD_NEW_CLIP, &runtime.labels.new_clip);
    append_menu_item(
        menu,
        CMD_DELETE_NON_PINNED,
        &runtime.labels.delete_non_pinned,
    );
    let toggle_label = if runtime.capture_enabled {
        &runtime.labels.pause
    } else {
        &runtime.labels.resume
    };
    append_menu_item(menu, CMD_TOGGLE_CAPTURE, toggle_label);
    append_separator(menu);
    append_menu_item(menu, CMD_EXIT, &runtime.labels.exit);

    let mut point = POINT { x: 0, y: 0 };
    unsafe {
        GetCursorPos(&mut point);
        SetForegroundWindow(hwnd);
        let selected = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON,
            point.x,
            point.y,
            0,
            hwnd,
            null(),
        );
        if selected > 0 {
            dispatch_menu_command(selected as usize, runtime);
        }
        DestroyMenu(menu);
    }
}

fn dispatch_menu_command(command_id: usize, runtime: &WindowsTrayRuntime) {
    let command = match command_id {
        CMD_OPEN => Some(TrayCommand::Open),
        CMD_SETTINGS => Some(TrayCommand::Settings),
        CMD_HOTKEYS => Some(TrayCommand::KeyboardSettings),
        CMD_UTILITIES => Some(TrayCommand::Utilities),
        CMD_CAPTURE => Some(TrayCommand::CaptureNow),
        CMD_SYNC => Some(TrayCommand::SyncNow),
        CMD_NEW_CLIP => Some(TrayCommand::NewClip),
        CMD_DELETE_NON_PINNED => Some(TrayCommand::DeleteNonPinned),
        CMD_TOGGLE_CAPTURE => Some(TrayCommand::ToggleCapture),
        CMD_EXIT => Some(TrayCommand::Exit),
        _ => None,
    };
    if let Some(command) = command {
        let _ = runtime.sender.send(command);
    }
}

fn append_menu_item(menu: HMENU, command_id: usize, label: &str) {
    let label = to_wide(label);
    unsafe {
        AppendMenuW(menu, MF_STRING, command_id, label.as_ptr());
    }
}

fn append_separator(menu: HMENU) {
    unsafe {
        AppendMenuW(menu, MF_SEPARATOR, 0, null());
    }
}

fn add_tray_icon(
    hwnd: HWND,
    icon: windows_sys::Win32::UI::WindowsAndMessaging::HICON,
    title: &str,
) -> bool {
    let mut data = notify_icon_data(hwnd);
    data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP;
    data.uCallbackMessage = WM_TRAY_ICON;
    data.hIcon = icon;
    set_wide_buf(&mut data.szTip, title);
    let added = unsafe { Shell_NotifyIconW(NIM_ADD, &data) != 0 };
    if added {
        unsafe {
            data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
            Shell_NotifyIconW(NIM_SETVERSION, &data);
        }
    }
    added
}

fn update_tray_tip(hwnd: HWND, title: &str) {
    let mut data = notify_icon_data(hwnd);
    data.uFlags = NIF_TIP | NIF_SHOWTIP;
    set_wide_buf(&mut data.szTip, title);
    unsafe {
        Shell_NotifyIconW(NIM_MODIFY, &data);
    }
}

unsafe fn remove_tray_icon(hwnd: HWND) {
    let data = notify_icon_data(hwnd);
    unsafe {
        Shell_NotifyIconW(NIM_DELETE, &data);
    }
}

fn notify_icon_data(hwnd: HWND) -> NOTIFYICONDATAW {
    NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: TRAY_ID,
        ..Default::default()
    }
}

fn set_wide_buf<const N: usize>(buf: &mut [u16; N], text: &str) {
    let wide = to_wide(text);
    let len = wide.len().saturating_sub(1).min(N.saturating_sub(1));
    buf[..len].copy_from_slice(&wide[..len]);
    if len < N {
        buf[len] = 0;
    }
}

fn to_wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
