//! Desktop window policy is fixed at launch. Preference changes take effect
//! after restart, so an in-flight session never changes renderer ownership.
use std::sync::atomic::{AtomicBool, Ordering};
static LOW_MEMORY_ACTIVE: AtomicBool = AtomicBool::new(false);
pub(crate) fn active() -> bool {
    LOW_MEMORY_ACTIVE.load(Ordering::Relaxed)
}

#[cfg(not(mobile))]
pub(crate) fn configure(context: &mut tauri::Context<tauri::Wry>, enabled: bool) {
    LOW_MEMORY_ACTIVE.store(enabled, Ordering::Relaxed);
    if enabled {
        for window in &mut context.config_mut().app.windows {
            if matches!(window.label.as_str(), "main" | "capsule" | "capsule-rail") {
                window.create = false;
            }
        }
    }
}

#[cfg(not(mobile))]
#[derive(Default)]
pub(crate) struct MainWindowState {
    pub closing: bool,
    pub reopen_pending: bool,
    pub size: Option<tauri::PhysicalSize<u32>>,
    pub position: Option<tauri::PhysicalPosition<i32>>,
    pub maximized: bool,
}

#[cfg(not(mobile))]
pub(crate) type MainWindowLifecycle = parking_lot::Mutex<MainWindowState>;

#[cfg(not(mobile))]
pub(crate) fn initialize_main<R: tauri::Runtime>(window: &tauri::WebviewWindow<R>) {
    #[cfg(target_os = "macos")]
    {
        use window_vibrancy::{apply_vibrancy, NSVisualEffectMaterial, NSVisualEffectState};
        let _ = window.set_decorations(true);
        if let Err(error) = apply_vibrancy(
            window,
            NSVisualEffectMaterial::HudWindow,
            Some(NSVisualEffectState::Active),
            Some(20.0),
        ) {
            log::warn!("[main] vibrancy failed: {error}");
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Err(error) = window_vibrancy::apply_mica(window, None) {
            log::warn!("[main] mica failed: {error}");
        }
        crate::apply_windows_caption_theme(window, false);
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let _ = window;
}

#[cfg(not(mobile))]
pub(crate) fn ensure_main<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> tauri::Result<Option<tauri::WebviewWindow<R>>> {
    use tauri::Manager;
    if let Some(lifecycle) = app.try_state::<MainWindowLifecycle>() {
        let mut state = lifecycle.lock();
        if state.closing {
            state.reopen_pending = true;
            return Ok(None);
        }
    }
    if let Some(window) = app.get_webview_window("main") {
        return Ok(Some(window));
    }
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|window| window.label == "main")
        .ok_or_else(|| tauri::Error::WindowNotFound)?;
    let window = tauri::WebviewWindowBuilder::from_config(app, config)?.build()?;
    initialize_main(&window);
    if let Some(lifecycle) = app.try_state::<MainWindowLifecycle>() {
        let state = lifecycle.lock();
        if let Some(size) = state.size {
            let _ = window.set_size(size);
        }
        if let Some(position) = state.position {
            let _ = window.set_position(position);
        }
        if state.maximized {
            let _ = window.maximize();
        }
    }
    Ok(Some(window))
}

#[cfg(not(mobile))]
pub(crate) fn close_main<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    use tauri::Manager;
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    if let Some(lifecycle) = app.try_state::<MainWindowLifecycle>() {
        let mut state = lifecycle.lock();
        if state.closing {
            return;
        }
        state.closing = true;
        state.maximized = window.is_maximized().unwrap_or(false);
        // Keep the last normal bounds: reopening a maximized window must not
        // overwrite its restore size with the monitor's dimensions.
        if !state.maximized {
            state.size = window.inner_size().ok();
            state.position = window.outer_position().ok();
        }
    }
    if let Err(error) = window.destroy() {
        if let Some(lifecycle) = app.try_state::<MainWindowLifecycle>() {
            lifecycle.lock().closing = false;
        }
        log::warn!("[main] destroy failed: {error}");
    }
}

#[cfg(not(mobile))]
pub(crate) fn main_destroyed<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    use tauri::Manager;
    let reopen = app
        .try_state::<MainWindowLifecycle>()
        .is_some_and(|lifecycle| {
            let mut state = lifecycle.lock();
            state.closing = false;
            std::mem::take(&mut state.reopen_pending)
        });
    if reopen {
        crate::show_main_window(app);
    }
}

/// Error/correction cards keep their existing UI, but their WebView is created
/// only while a card is on screen. No recording capsule/rail WebView is needed.
#[cfg(not(mobile))]
pub(crate) fn ensure_card_window<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> tauri::Result<()> {
    use tauri::Manager;
    if app.get_webview_window("capsule").is_some() {
        return Ok(());
    }
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|window| window.label == "capsule")
        .ok_or(tauri::Error::WindowNotFound)?;
    let window = tauri::WebviewWindowBuilder::from_config(app, config)?.build()?;
    #[cfg(target_os = "macos")]
    {
        use objc2::{
            msg_send,
            runtime::{AnyClass, AnyObject, Bool, ClassBuilder, Sel},
            sel, ClassType,
        };
        use objc2_app_kit::NSPanel;
        unsafe extern "C" fn cannot_focus(_: &AnyObject, _: Sel) -> Bool {
            Bool::NO
        }
        extern "C" {
            fn object_setClass(object: *mut AnyObject, class: *const AnyClass) -> *const AnyClass;
            fn class_getInstanceSize(class: *const AnyClass) -> usize;
        }
        unsafe {
            let panel = if let Some(class) = AnyClass::get("OpenLessAuxiliaryPanel") {
                class
            } else {
                let mut builder = ClassBuilder::new("OpenLessAuxiliaryPanel", NSPanel::class())
                    .expect("panel class registration runs only on the main thread");
                builder.add_method(
                    sel!(canBecomeKeyWindow),
                    cannot_focus as unsafe extern "C" fn(_, _) -> _,
                );
                builder.add_method(
                    sel!(canBecomeMainWindow),
                    cannot_focus as unsafe extern "C" fn(_, _) -> _,
                );
                builder.register()
            };
            let raw = window.ns_window()? as *mut AnyObject;
            let original: *const AnyClass = msg_send![raw, class];
            if class_getInstanceSize(panel) > class_getInstanceSize(original) {
                let _ = window.destroy();
                return Err(tauri::Error::Anyhow(anyhow::anyhow!(
                    "auxiliary panel exceeds native window allocation"
                )));
            }
            object_setClass(raw, panel);
            let style: usize = msg_send![raw, styleMask];
            let _: () = msg_send![raw, setStyleMask: style | (1 << 7)];
            let _: () = msg_send![raw, setLevel: 25isize];
            let _: () = msg_send![raw, setCollectionBehavior: (1usize << 0) | (1usize << 8)];
            let _: () = msg_send![raw, setHidesOnDeactivate: false];
        }
        // Do not put disposable panels in tauri-nspanel's retained registry:
        // that registry has no removal API and would keep the content alive.
    }
    window.set_ignore_cursor_events(false)?;
    Ok(())
}

#[tauri::command]
pub(crate) fn desktop_low_memory_active() -> bool {
    active()
}
