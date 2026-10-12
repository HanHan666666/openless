//! Native window behavior, kept out of the shared capsule painter.
use eframe::egui;

pub fn configure_window(viewport: egui::ViewportBuilder) -> egui::ViewportBuilder {
    viewport
}

pub fn configure_event_loop(builder: &mut winit::event_loop::EventLoopBuilder<eframe::UserEvent>) {
    #[cfg(target_os = "macos")]
    {
        use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
        builder
            .with_activation_policy(ActivationPolicy::Accessory)
            .with_activate_ignoring_other_apps(false)
            .with_default_menu(false);
    }
    #[cfg(not(target_os = "macos"))]
    let _ = builder;
}

pub fn configure_native(
    cc: &eframe::CreationContext<'_>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    #[cfg(windows)]
    {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        use windows::Win32::{
            Foundation::HWND,
            UI::WindowsAndMessaging::{
                GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_NOACTIVATE,
                WS_EX_TOOLWINDOW,
            },
        };
        if let RawWindowHandle::Win32(handle) = cc.window_handle()?.as_raw() {
            let window = HWND(handle.hwnd.get() as *mut _);
            unsafe {
                let style = GetWindowLongPtrW(window, GWL_EXSTYLE);
                SetWindowLongPtrW(
                    window,
                    GWL_EXSTYLE,
                    style | WS_EX_NOACTIVATE.0 as isize | WS_EX_TOOLWINDOW.0 as isize,
                );
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        use objc2::{
            msg_send,
            runtime::{AnyClass, AnyObject, Bool, ClassBuilder, Sel},
            sel, ClassType,
        };
        use objc2_app_kit::NSPanel;
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        extern "C" {
            fn object_setClass(object: *mut AnyObject, class: *const AnyClass) -> *const AnyClass;
            fn class_getInstanceSize(class: *const AnyClass) -> usize;
        }
        unsafe extern "C" fn cannot_focus(_: &AnyObject, _: Sel) -> Bool {
            Bool::NO
        }
        if let RawWindowHandle::AppKit(handle) = cc.window_handle()?.as_raw() {
            unsafe {
                let view = handle.ns_view.as_ptr() as *mut AnyObject;
                let window: *mut AnyObject = msg_send![view, window];
                let original: *const AnyClass = msg_send![window, class];
                // Pinned winit 0.30's WinitWindow subclass has no ivars. Keep
                // its allocated layout intact and reject an incompatible SDK
                // rather than silently corrupting a larger NSPanel object.
                let mut builder = ClassBuilder::new("OpenLessNativeCapsulePanel", NSPanel::class())
                    .ok_or("native panel class already registered")?;
                builder.add_method(
                    sel!(canBecomeKeyWindow),
                    cannot_focus as unsafe extern "C" fn(_, _) -> _,
                );
                builder.add_method(
                    sel!(canBecomeMainWindow),
                    cannot_focus as unsafe extern "C" fn(_, _) -> _,
                );
                let panel = builder.register();
                if class_getInstanceSize(panel) > class_getInstanceSize(original) {
                    return Err("NSPanel layout exceeds the native window allocation".into());
                }
                object_setClass(window, panel);
                // NSWindowStyleMaskNonactivatingPanel, above menu-bar level,
                // CanJoinAllSpaces | FullScreenAuxiliary.
                let style: usize = msg_send![window, styleMask];
                let _: () = msg_send![window, setStyleMask: style | (1 << 7)];
                let _: () = msg_send![window, setLevel: 25isize];
                let _: () = msg_send![window, setCollectionBehavior: (1usize << 0) | (1usize << 8)];
                let _: () = msg_send![window, setHidesOnDeactivate: false];
            }
        }
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    let _ = cc;
    Ok(())
}

pub fn place(window: &winit::window::Window, bounds: [i32; 4]) {
    window.set_outer_position(winit::dpi::PhysicalPosition::new(bounds[0], bounds[1]));
    let _ = window.request_inner_size(winit::dpi::PhysicalSize::new(
        bounds[2].max(1) as u32,
        bounds[3].max(1) as u32,
    ));
}
