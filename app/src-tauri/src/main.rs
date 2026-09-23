// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{
    env,
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
};

struct SingleInstanceGuard {
    path: PathBuf,
}

impl Drop for SingleInstanceGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn is_process_running(pid: u32) -> bool {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::Threading::{
            GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
        };
        unsafe {
            match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
                Ok(handle) => {
                    let mut exit_code = 0u32;
                    let alive = GetExitCodeProcess(handle, &mut exit_code).is_ok()
                        && exit_code == 259; // STILL_ACTIVE
                    let _ = CloseHandle(handle);
                    alive
                }
                Err(_) => false,
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        // POSIX: `kill -0 <pid>` exits 0 iff the process exists and is signalable
        // by us. No signal is actually sent. Missing `kill` or any error → treat as
        // not running, so a genuinely stale lock still gets cleared.
        std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
}

fn try_acquire_single_instance() -> Result<SingleInstanceGuard, String> {
    let home = dirs::home_dir().ok_or("Could not determine home directory")?;
    let snap_dir = home.join(".snap");
    fs::create_dir_all(&snap_dir).map_err(|e| format!("Failed to create ~/.snap: {}", e))?;

    let lock_path = snap_dir.join("snap-tray.lock");

    loop {
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
        {
            Ok(mut file) => {
                let _ = writeln!(file, "{}", std::process::id());
                return Ok(SingleInstanceGuard { path: lock_path });
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                let stale = fs::read_to_string(&lock_path)
                    .ok()
                    .and_then(|s| s.trim().parse::<u32>().ok())
                    .map(|pid| !is_process_running(pid))
                    .unwrap_or(true); // unreadable or unparseable → treat as stale

                if !stale {
                    return Err("another instance is already running".to_string());
                }

                snap_lib::log_event("removing stale lock file");
                let _ = fs::remove_file(&lock_path);
                // loop to retry
            }
            Err(e) => return Err(format!("failed to create lock file: {}", e)),
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();

    #[cfg(target_os = "macos")]
    let use_overlay = args.contains(&"--overlay-mode".to_string());

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let use_overlay = env::var("WAYLAND_DISPLAY").is_ok()
        || args.contains(&"--overlay-mode".to_string());

    #[cfg(target_os = "windows")]
    let use_overlay = args.contains(&"--overlay-mode".to_string());

    if use_overlay {
        run_overlay_mode();
    } else {
        run_tray_mode();
    }
}

/// A monitor's size in logical pixels (what the webview's innerWidth/Height report).
fn monitor_logical_size(m: &tauri::Monitor) -> (f64, f64) {
    let size = m.size();
    let scale = m.scale_factor();
    snap_lib::log_event(&format!(
        "monitor {:?}: {}x{} physical, scale {}",
        m.name(),
        size.width,
        size.height,
        scale
    ));
    (size.width as f64 / scale, size.height as f64 / scale)
}

/// Single-shot mode: open overlay window, user annotates, save, exit.
/// Used on Wayland where global hotkeys require the DE to trigger us.
///
/// The window is created here, not in tauri.conf.json: the config declares no
/// windows so that tray mode can build the overlay on demand. The frontend
/// shows the window and switches it to fullscreen once the capture is loaded,
/// so it starts hidden. The app exits when the frontend destroys the window.
fn run_overlay_mode() {
    snap_lib::log_event("snap starting (overlay mode)");

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(snap_lib::invoke_handler())
        .setup(|app| {
            let handle = app.handle().clone();
            // On Wayland the compositor ignores fullscreen requests for this
            // window (both the builder flag and the frontend's setFullscreen),
            // so the window must be given the monitor's logical size explicitly
            // or it stays at the 800x600 default. primary_monitor() is None on
            // Wayland, so enumerate monitors and take the first.
            let monitor_size = handle
                .primary_monitor()
                .ok()
                .flatten()
                .or_else(|| handle.available_monitors().ok().and_then(|m| m.into_iter().next()))
                .map(|m| monitor_logical_size(&m));

            let mut builder = snap_lib::overlay_window_builder(&handle)
                .visible(false)
                .transparent(false)
                .fullscreen(true)
                .position(0.0, 0.0);
            if let Some((w, h)) = monitor_size {
                // GDK under XWayland can report the monitor already scaled
                // (a 3840x2160 panel at scale 2 comes back as 7680x4320), so
                // this may be double the true logical size. That is harmless:
                // the compositor clamps the window to the screen, which is
                // exactly what we want.
                snap_lib::log_event(&format!("sizing overlay to monitor: {}x{} logical", w, h));
                builder = builder.inner_size(w, h);
            }
            let window = builder.build()?;

            // Last resort: the monitor is only known once the window exists.
            if monitor_size.is_none() {
                if let Ok(Some(m)) = window.current_monitor() {
                    let (w, h) = monitor_logical_size(&m);
                    snap_lib::log_event(&format!(
                        "sizing overlay to current monitor: {}x{} logical",
                        w, h
                    ));
                    let _ = window.set_size(tauri::LogicalSize::new(w, h));
                } else {
                    snap_lib::log_event("WARNING: no monitor found; overlay may not cover the screen");
                }
            }

            if let (Ok(size), Ok(fs)) = (window.inner_size(), window.is_fullscreen()) {
                snap_lib::log_event(&format!(
                    "overlay window created {}x{} physical, fullscreen={}",
                    size.width, size.height, fs
                ));
            }
            snap_lib::log_event("overlay mode ready");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running snap");
}

fn run_tray_mode() {
    use std::sync::atomic::Ordering;
    use tauri::{
        image::Image,
        menu::{Menu, MenuItem},
        tray::TrayIconBuilder,
        Emitter,
        Manager,
    };
    use tauri_plugin_global_shortcut::{
        Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState,
    };

    let _instance_guard = match try_acquire_single_instance() {
        Ok(guard) => guard,
        Err(e) => {
            snap_lib::log_event(&format!("tray start skipped: {}", e));
            return;
        }
    };

    snap_lib::log_event("snap starting (tray mode)");

    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(snap_lib::invoke_handler())
        .on_window_event(|window, event| {
            // The frontend normally clears OVERLAY_ACTIVE via mark_overlay_closed,
            // but if the overlay dies any other way (window-manager close, webview
            // crash) the flag would stay set and the hotkey would be dead until
            // restart. Destroyed is the one event that fires in every case.
            if window.label() == "overlay" && matches!(event, tauri::WindowEvent::Destroyed) {
                if snap_lib::OVERLAY_ACTIVE.swap(false, Ordering::SeqCst) {
                    snap_lib::log_event("overlay destroyed without close signal; hotkey re-armed");
                }
            }
        })
        .setup(|app| {
            #[cfg(target_os = "macos")]
            {
                let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            }

            // ---- System tray ----
            let open_logs =
                MenuItem::with_id(app, "open_logs", "Open Logs", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit Snap", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open_logs, &quit])?;

            let icon = {
                let png_bytes = include_bytes!("../icons/icon.png");
                let decoder = png::Decoder::new(std::io::Cursor::new(png_bytes));
                match decoder.read_info() {
                    Ok(mut reader) => {
                        let mut buf = vec![0u8; reader.output_buffer_size()];
                        if let Ok(info) = reader.next_frame(&mut buf) {
                            buf.truncate(info.buffer_size());
                            Image::new_owned(buf, info.width, info.height)
                        } else {
                            Image::new_owned(vec![255u8, 255, 255, 200], 1, 1)
                        }
                    }
                    Err(_) => Image::new_owned(vec![255u8, 255, 255, 200], 1, 1),
                }
            };

            let (shortcut_mods, shortcut_label) = (Modifiers::CONTROL | Modifiers::SHIFT, "Ctrl+Shift+S");

            TrayIconBuilder::new()
                .icon(icon)
                .tooltip(&format!("Snap \u{2014} {} to annotate", shortcut_label))
                .menu(&menu)
                .on_menu_event(|app, event| {
                    if event.id() == "open_logs" {
                        let snap_dir = dirs::home_dir()
                            .map(|h| h.join(".snap"))
                            .unwrap_or_else(|| std::env::temp_dir());

                        #[cfg(target_os = "windows")]
                        {
                            let _ = std::process::Command::new("explorer")
                                .arg(&snap_dir)
                                .spawn();
                        }

                        #[cfg(target_os = "macos")]
                        {
                            let _ = std::process::Command::new("open")
                                .arg(&snap_dir)
                                .spawn();
                        }

                        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
                        {
                            let _ = std::process::Command::new("xdg-open")
                                .arg(&snap_dir)
                                .spawn();
                        }
                        return;
                    }

                    if event.id() == "quit" {
                        snap_lib::log_event("quit from tray");
                        app.exit(0);
                    }
                })
                .build(app)?;

            // ---- Global shortcut ----
            let shortcut = Shortcut::new(Some(shortcut_mods), Code::KeyS);

            let app_handle = app.handle().clone();

            app.global_shortcut().on_shortcut(
                shortcut,
                move |_app, _shortcut, event| {
                    if event.state != ShortcutState::Pressed {
                        return;
                    }

                    if snap_lib::OVERLAY_ACTIVE.load(Ordering::SeqCst) {
                        return;
                    }
                    snap_lib::OVERLAY_ACTIVE.store(true, Ordering::SeqCst);

                    snap_lib::log_event("hotkey triggered");

                    let handle = app_handle.clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_millis(50));

                        snap_lib::capture_and_store_window_context();

                        #[cfg(target_os = "macos")]
                        {
                            match snap_lib::capture_screen_interactive() {
                                Ok(()) => {}
                                Err(e) => {
                                    snap_lib::log_event(&format!(
                                        "capture cancelled: {}",
                                        e
                                    ));
                                    snap_lib::OVERLAY_ACTIVE.store(false, Ordering::SeqCst);
                                    return;
                                }
                            }
                        }

                        let handle_inner = handle.clone();
                        let dispatch_result = handle.run_on_main_thread(move || {
                            #[cfg(target_os = "macos")]
                            {
                                if let Some(window) = handle_inner.get_webview_window("overlay") {
                                    let _ = window.show();
                                    let _ = window.set_focus();
                                    let _ = window.emit("snap://start", ());
                                    snap_lib::log_event("overlay window reused");
                                    return;
                                }
                            }

                            #[cfg(not(target_os = "macos"))]
                            if let Some(window) = handle_inner.get_webview_window("overlay") {
                                let _ = window.destroy();
                            }

                            let mut builder = snap_lib::overlay_window_builder(&handle_inner);

                            #[cfg(target_os = "macos")]
                            {
                                // Keep mac startup conservative to avoid WebKit display-link
                                // crashes observed with hidden + transparent initialization.
                                builder = builder.visible(true).transparent(false);
                            }

                            #[cfg(not(target_os = "macos"))]
                            {
                                builder = builder.visible(false).transparent(true).fullscreen(true);
                            }

                            match builder.build() {
                                Ok(_) => snap_lib::log_event("overlay window created"),
                                Err(e) => {
                                    snap_lib::log_event(&format!(
                                        "failed to create overlay: {}",
                                        e
                                    ));
                                    snap_lib::OVERLAY_ACTIVE.store(false, Ordering::SeqCst);
                                }
                            }
                        });

                        if let Err(e) = dispatch_result {
                            snap_lib::log_event(&format!(
                                "failed to schedule overlay creation on main thread: {}",
                                e
                            ));
                            snap_lib::OVERLAY_ACTIVE.store(false, Ordering::SeqCst);
                        }
                    });
                },
            )?;

            snap_lib::log_event(&format!("global shortcut registered: {}", shortcut_label));
            snap_lib::log_event("snap ready — waiting for hotkey");

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("failed to build app")
        .run(|_app, event| {
            // keep alive if no windows
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                api.prevent_exit();
            }
        });
}
