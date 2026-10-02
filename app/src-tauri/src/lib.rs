use base64::Engine;
use serde::Serialize;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

pub static OVERLAY_ACTIVE: AtomicBool = AtomicBool::new(false);
pub static PRE_CAPTURED: AtomicBool = AtomicBool::new(false);

// ----- Capture file path -----

fn capture_temp_path() -> PathBuf {
    std::env::temp_dir().join("snap-capture.png")
}

// ----- Screen Capture -----

#[cfg(target_os = "macos")]
pub fn capture_screen_interactive() -> Result<(), String> {
    let path = capture_temp_path();
    let _ = fs::remove_file(&path);

    let output = Command::new("screencapture")
        .args(["-i", "-s", path.to_str().unwrap_or("/tmp/snap-capture.png")])
        .output()
        .map_err(|e| format!("screencapture failed to launch: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "screencapture failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    match fs::metadata(&path) {
        Ok(meta) if meta.len() > 0 => {
            log_event("region captured interactively");
            PRE_CAPTURED.store(true, Ordering::SeqCst);
            Ok(())
        }
        _ => Err("cancelled".to_string()),
    }
}

#[tauri::command]
fn capture_screen() -> Result<String, String> {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    let path = capture_temp_path();

    #[cfg(target_os = "windows")]
    {
        use screenshots::Screen;

        let _ = fs::remove_file(&path);

        let screens = Screen::all().map_err(|e| format!("Failed to list screens: {}", e))?;

        let screen = screens
            .iter()
            .find(|s| s.display_info.is_primary)
            .or_else(|| screens.first())
            .ok_or("No screens found")?;

        let image = screen
            .capture()
            .map_err(|e| format!("Screen capture failed: {}", e))?;

        image
            .save(&path)
            .map_err(|e| format!("Failed to save capture: {}", e))?;

        match fs::metadata(&path) {
            Ok(meta) if meta.len() > 0 => {
                log_event("screen captured via screenshots crate");
                return Ok(path.to_string_lossy().to_string());
            }
            _ => return Err("Screen capture produced empty file".to_string()),
        }
    }

    #[cfg(target_os = "macos")]
    {
        if PRE_CAPTURED.swap(false, Ordering::SeqCst) {
            return Ok(path.to_string_lossy().to_string());
        }

        // fallback: full-screen silent capture
        let _ = fs::remove_file(&path);
        let output = Command::new("screencapture")
            .args(["-x", path.to_str().unwrap_or("/tmp/snap-capture.png")])
            .output()
            .map_err(|e| format!("screencapture failed to launch: {}", e))?;

        if !output.status.success() {
            return Err(format!(
                "screencapture failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        match fs::metadata(&path) {
            Ok(meta) if meta.len() > 0 => {
                log_event("screen captured via screencapture");
                return Ok(path.to_string_lossy().to_string());
            }
            _ => return Err("screencapture produced empty file".to_string()),
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        // Overlay mode starts the capture at process start (start_precapture);
        // use that result if it is there, otherwise capture now.
        let pending = PRECAPTURE.lock().ok().and_then(|mut slot| slot.take());
        if let Some(handle) = pending {
            return handle
                .join()
                .unwrap_or_else(|_| Err("capture thread panicked".to_string()));
        }
        capture_screen_linux()
    }
}

/// Handle of a capture started before the webview asked for one (Linux).
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
static PRECAPTURE: Mutex<Option<std::thread::JoinHandle<Result<String, String>>>> =
    Mutex::new(None);

/// Start the screen capture in the background so it overlaps with webview
/// start-up instead of running after it. Cuts hotkey-to-overlay latency by
/// roughly the capture time, and grabs the screen closer to the keypress.
/// `capture_screen` picks the result up. No-op off Linux, where capture is
/// either interactive (macOS) or already fast (Windows).
pub fn start_precapture() {
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        if let Ok(mut slot) = PRECAPTURE.lock() {
            if slot.is_none() {
                *slot = Some(std::thread::spawn(capture_screen_linux));
            }
        }
    }
}

/// Linux: try capture tools in order of preference.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn capture_screen_linux() -> Result<String, String> {
    let path = capture_temp_path();
    {
        let path_str = path.to_string_lossy().to_string();
        let _ = fs::remove_file(&path);

        let methods: Vec<(&str, Vec<&str>)> = if std::env::var("WAYLAND_DISPLAY").is_ok() {
            vec![
                // GNOME Wayland: gnome-screenshot is the most reliable
                ("gnome-screenshot", vec!["--file", &path_str]),
                // wlroots-based compositors (Sway, Hyprland, etc.)
                ("grim", vec![&path_str]),
                // Fallback: scrot might work via XWayland
                ("scrot", vec!["--overwrite", &path_str]),
            ]
        } else {
            vec![
                ("scrot", vec!["--overwrite", &path_str]),
                ("gnome-screenshot", vec!["--file", &path_str]),
            ]
        };

        let mut last_error = String::from("No screenshot tool found");
        let started = std::time::Instant::now();

        // Wayland: the desktop portal first. It is the only silent route on
        // GNOME 50+, where gnome-screenshot is no longer allowed to capture.
        if std::env::var("WAYLAND_DISPLAY").is_ok() {
            match capture_via_portal(&path) {
                Ok(()) => {
                    log_event(&format!(
                        "screen captured via portal in {}ms",
                        started.elapsed().as_millis()
                    ));
                    return Ok(path_str.clone());
                }
                Err(e) => {
                    log_event(&format!("portal capture failed: {}; trying capture tools", e));
                    last_error = format!("portal: {}", e);
                }
            }
        }

        for (tool, args) in &methods {
            match run_capture_tool(tool, args) {
                Some(Ok(())) => match fs::metadata(&path) {
                    Ok(meta) if meta.len() > 0 => {
                        log_event(&format!(
                            "screen captured via {} in {}ms",
                            tool,
                            started.elapsed().as_millis()
                        ));
                        return Ok(path_str.clone());
                    }
                    _ => {
                        last_error = format!("{} produced empty file", tool);
                        continue;
                    }
                },
                Some(Err(e)) => {
                    last_error = e;
                    continue;
                }
                None => continue, // tool not installed, try next
            }
        }

        log_event(&format!("screen capture failed: {}", last_error));
        Err(format!(
            "Screen capture failed: {}. Install one of: sudo apt install gnome-screenshot grim scrot",
            last_error
        ))
    }
}

/// Run a capture tool, killing it if it does not finish in time. Returns None
/// when the tool is not installed. The timeout matters: on GNOME 50
/// gnome-screenshot neither captures nor exits, and an overlay stuck waiting
/// on it holds the hotkey lock until someone kills it.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn run_capture_tool(tool: &str, args: &[&str]) -> Option<Result<(), String>> {
    use std::io::Read;
    use std::process::Stdio;
    use std::time::{Duration, Instant};

    const TIMEOUT: Duration = Duration::from_secs(10);

    let mut child = Command::new(tool)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + TIMEOUT;

    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Some(Ok(())),
            Ok(Some(_)) => {
                let mut stderr = String::new();
                if let Some(mut pipe) = child.stderr.take() {
                    let _ = pipe.read_to_string(&mut stderr);
                }
                return Some(Err(format!("{} failed: {}", tool, stderr.trim())));
            }
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(20));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Some(Err(format!(
                    "{} timed out after {}s",
                    tool,
                    TIMEOUT.as_secs()
                )));
            }
            Err(e) => return Some(Err(format!("{} failed: {}", tool, e))),
        }
    }
}

/// Capture the screen through the XDG desktop portal
/// (org.freedesktop.portal.Screenshot) and move the result to `dest`.
/// The portal replies asynchronously: the method call returns a request
/// handle, and the file's URI arrives later in a Response signal on it.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn capture_via_portal(dest: &std::path::Path) -> Result<(), String> {
    use gio::prelude::*;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    const PORTAL: &str = "org.freedesktop.portal.Desktop";
    // Generous: the first request may put up a permission prompt.
    const TIMEOUT: Duration = Duration::from_secs(30);

    // The Response signal is delivered to whichever main context is the
    // thread default at subscribe time. Use a private one so this works from
    // the precapture thread and never touches the GTK main loop.
    let ctx = glib::MainContext::new();
    ctx.with_thread_default(|| -> Result<(), String> {
        let conn = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE)
            .map_err(|e| format!("session bus unavailable: {}", e))?;
        let name = conn.unique_name().ok_or("no D-Bus unique name")?;
        let token = format!("snap{}", std::process::id());
        let handle = format!(
            "/org/freedesktop/portal/desktop/request/{}/{}",
            name.trim_start_matches(':').replace('.', "_"),
            token
        );

        // Subscribe before calling so the Response cannot be missed.
        let response: Arc<Mutex<Option<(u32, Option<String>)>>> = Arc::new(Mutex::new(None));
        let slot = response.clone();
        let subscription = conn.signal_subscribe(
            Some(PORTAL),
            Some("org.freedesktop.portal.Request"),
            Some("Response"),
            Some(&handle),
            None,
            gio::DBusSignalFlags::NONE,
            move |_, _, _, _, _, params| {
                if let Some((code, results)) = params.get::<(u32, glib::VariantDict)>() {
                    let uri = results.lookup::<String>("uri").ok().flatten();
                    if let Ok(mut slot) = slot.lock() {
                        *slot = Some((code, uri));
                    }
                }
            },
        );

        let options = glib::VariantDict::new(None);
        options.insert_value("handle_token", &token.to_variant());
        options.insert_value("interactive", &false.to_variant());
        let args = glib::Variant::tuple_from_iter(["".to_variant(), options.end()]);

        let outcome = conn
            .call_sync(
                Some(PORTAL),
                "/org/freedesktop/portal/desktop",
                "org.freedesktop.portal.Screenshot",
                "Screenshot",
                Some(&args),
                None,
                gio::DBusCallFlags::NONE,
                5000,
                gio::Cancellable::NONE,
            )
            .map_err(|e| format!("Screenshot call failed: {}", e))
            .and_then(|_| {
                let deadline = Instant::now() + TIMEOUT;
                loop {
                    while ctx.iteration(false) {}
                    if let Some(reply) = response.lock().ok().and_then(|mut r| r.take()) {
                        break Ok(reply);
                    }
                    if Instant::now() >= deadline {
                        let _ = conn.call_sync(
                            Some(PORTAL),
                            &handle,
                            "org.freedesktop.portal.Request",
                            "Close",
                            None,
                            None,
                            gio::DBusCallFlags::NONE,
                            1000,
                            gio::Cancellable::NONE,
                        );
                        break Err(format!("no response after {}s", TIMEOUT.as_secs()));
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
            });

        conn.signal_unsubscribe(subscription);
        while ctx.iteration(false) {}

        let (code, uri) = outcome?;
        if code != 0 {
            return Err(format!("request denied or cancelled (response {})", code));
        }
        let uri = uri.ok_or("response carried no file")?;
        let src = gio::File::for_uri(&uri)
            .path()
            .ok_or_else(|| format!("not a local file: {}", uri))?;

        // The portal saves into the user's Pictures folder. Move the file out
        // so captures do not pile up there; rename fails across filesystems.
        if fs::rename(&src, dest).is_err() {
            fs::copy(&src, dest).map_err(|e| format!("cannot copy {}: {}", src.display(), e))?;
            let _ = fs::remove_file(&src);
        }
        Ok(())
    })
    .map_err(|e| format!("cannot acquire a main context: {}", e))?
}

// ----- Window Context -----

#[derive(Serialize, Clone)]
struct WindowContext {
    window_title: Option<String>,
    url: Option<String>,
    window_class: Option<String>,
    pid: Option<u32>,
    /// Which display server / platform produced this context. Lets agents
    /// tell "no focused window" apart from "this platform can't report one"
    /// (Wayland has no portable way to query the focused window).
    session_type: String,
}

impl WindowContext {
    fn empty() -> Self {
        WindowContext {
            window_title: None,
            url: None,
            window_class: None,
            pid: None,
            session_type: session_type().to_string(),
        }
    }
}

/// "wayland" | "x11" | "macos" | "windows"
pub fn session_type() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "macos"
    }
    #[cfg(target_os = "windows")]
    {
        "windows"
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        if std::env::var("WAYLAND_DISPLAY").is_ok() {
            "wayland"
        } else {
            "x11"
        }
    }
}

static PRE_CAPTURED_CONTEXT: Mutex<Option<WindowContext>> = Mutex::new(None);

pub fn capture_and_store_window_context() {
    #[cfg(target_os = "windows")]
    {
        let ctx = get_active_window_context_windows();
        if let Ok(mut lock) = PRE_CAPTURED_CONTEXT.lock() {
            *lock = Some(ctx);
        }
    }

    #[cfg(target_os = "macos")]
    {
        let ctx = get_active_window_context_macos().unwrap_or_else(|_| WindowContext::empty());
        if let Ok(mut lock) = PRE_CAPTURED_CONTEXT.lock() {
            *lock = Some(ctx);
        }
    }
}

#[tauri::command]
fn get_active_window_context() -> Result<WindowContext, String> {
    #[cfg(target_os = "windows")]
    {
        if let Ok(mut lock) = PRE_CAPTURED_CONTEXT.lock() {
            if let Some(ctx) = lock.take() {
                return Ok(ctx);
            }
        }
        return Ok(get_active_window_context_windows());
    }

    #[cfg(target_os = "macos")]
    {
        if let Ok(mut lock) = PRE_CAPTURED_CONTEXT.lock() {
            if let Some(ctx) = lock.take() {
                return Ok(ctx);
            }
        }
        return get_active_window_context_macos();
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        // Only works on X11
        if std::env::var("WAYLAND_DISPLAY").is_ok() {
            return Ok(WindowContext::empty());
        }

        let title = run_xdotool(&["getactivewindow", "getwindowname"]);
        let class = run_xdotool(&["getactivewindow", "getwindowclassname"]);
        let pid_str = run_xdotool(&["getactivewindow", "getwindowpid"]);
        let pid = pid_str.as_ref().and_then(|s| s.trim().parse::<u32>().ok());

        // Try to infer URL from browser window titles
        let url = title.as_ref().and_then(|t| {
            let is_browser = class
                .as_ref()
                .map(|c| {
                    let lower = c.to_lowercase();
                    lower.contains("brave")
                        || lower.contains("firefox")
                        || lower.contains("chrom")
                        || lower.contains("webkit")
                })
                .unwrap_or(false);

            if is_browser {
                Some(t.clone())
            } else {
                None
            }
        });

        Ok(WindowContext {
            window_title: title,
            url,
            window_class: class,
            pid,
            session_type: "x11".to_string(),
        })
    }
}

#[cfg(target_os = "windows")]
fn get_active_window_context_windows() -> WindowContext {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetClassNameW, GetForegroundWindow, GetWindowTextLengthW, GetWindowTextW,
        GetWindowThreadProcessId,
    };

    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return WindowContext::empty();
        }

        // Window title
        let title_len = GetWindowTextLengthW(hwnd);
        let window_title = if title_len > 0 {
            let mut buf = vec![0u16; (title_len + 1) as usize];
            GetWindowTextW(hwnd, &mut buf);
            buf.truncate(title_len as usize);
            Some(String::from_utf16_lossy(&buf))
        } else {
            None
        };

        // Window class name (identifies the application type)
        let mut class_buf = [0u16; 256];
        let class_len = GetClassNameW(hwnd, &mut class_buf);
        let window_class = if class_len > 0 {
            Some(String::from_utf16_lossy(&class_buf[..class_len as usize]))
        } else {
            None
        };

        // Process ID
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));

        // Browsers: Chrome, Edge, Firefox, Brave — infer URL from title bar
        let is_browser = window_class
            .as_ref()
            .map(|c| {
                let lower = c.to_lowercase();
                lower.contains("chrome")
                    || lower.contains("msedge")
                    || lower.contains("firefox")
                    || lower.contains("brave")
                    || lower.contains("opera")
            })
            .unwrap_or(false);

        let url = if is_browser {
            window_title.clone()
        } else {
            None
        };

        WindowContext {
            window_title,
            url,
            window_class,
            pid: if pid > 0 { Some(pid) } else { None },
            session_type: "windows".to_string(),
        }
    }
}

#[cfg(target_os = "macos")]
fn get_active_window_context_macos() -> Result<WindowContext, String> {
    fn osascript(script: &str) -> Option<String> {
        Command::new("osascript")
            .args(["-e", script])
            .output()
            .ok()
            .and_then(|o| {
                if o.status.success() {
                    let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
                    if s.is_empty() { None } else { Some(s) }
                } else {
                    None
                }
            })
    }

    // Get frontmost application name
    let app_name = osascript(
        "tell application \"System Events\" to get name of first process where frontmost is true",
    );

    // Get frontmost window title
    let window_title = app_name.as_deref().and_then(|name| {
        osascript(&format!(
            "tell application \"{}\" to get name of front window",
            name
        ))
    });

    let is_browser = app_name.as_deref().map(|n| {
        let lower = n.to_lowercase();
        lower.contains("safari")
            || lower.contains("firefox")
            || lower.contains("chrome")
            || lower.contains("brave")
            || lower.contains("webkit")
    }).unwrap_or(false);

    let url = if is_browser {
        app_name.as_deref().and_then(|name| {
            let lower = name.to_lowercase();
            let script = if lower.contains("safari") {
                "tell application \"Safari\" to get URL of current tab of front window".to_string()
            } else {
                format!(
                    "tell application \"{}\" to get URL of active tab of front window",
                    name
                )
            };
            osascript(&script)
        })
    } else {
        None
    };

    Ok(WindowContext {
        window_title: window_title.or_else(|| app_name.clone()),
        url,
        window_class: app_name,
        pid: None,
        session_type: "macos".to_string(),
    })
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn run_xdotool(args: &[&str]) -> Option<String> {
    Command::new("xdotool")
        .args(args)
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
            } else {
                None
            }
        })
}

// ----- Save Annotation -----

#[tauri::command]
fn save_annotation(metadata_json: String, image_base64: Option<String>) -> Result<String, String> {
    let inbox = inbox_dir()?;

    let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S%.3f").to_string();
    let timestamp = timestamp.replace('.', "-");
    let png_name = format!("snap-{}.png", timestamp);
    let json_name = format!("snap-{}.json", timestamp);

    let png_path = inbox.join(&png_name);
    let json_path = inbox.join(&json_name);

    // If base64 image data provided, decode and write it
    // Otherwise, copy the screen capture file directly
    if let Some(b64) = image_base64 {
        let image_bytes = base64::engine::general_purpose::STANDARD
            .decode(&b64)
            .map_err(|e| format!("Base64 decode failed: {}", e))?;
        fs::write(&png_path, &image_bytes).map_err(|e| format!("Failed to write PNG: {}", e))?;
    } else {
        // Copy the raw screen capture — much faster, no IPC overhead
        let capture = capture_temp_path();
        if capture.exists() {
            fs::copy(&capture, &png_path)
                .map_err(|e| format!("Failed to copy capture: {}", e))?;
        } else {
            return Err("No capture file found".to_string());
        }
    }

    // Inject image filename into metadata
    let mut metadata: serde_json::Value =
        serde_json::from_str(&metadata_json).map_err(|e| format!("Invalid JSON: {}", e))?;
    metadata["image_filename"] = serde_json::json!(png_name);

    let metadata_str =
        serde_json::to_string_pretty(&metadata).map_err(|e| format!("JSON serialize: {}", e))?;

    // Write JSON
    fs::write(&json_path, &metadata_str)
        .map_err(|e| format!("Failed to write metadata: {}", e))?;

    log_event(&format!("Saved annotation: {}", png_name));

    Ok(png_path.to_string_lossy().to_string())
}

// ----- Read capture as base64 (avoids tainted canvas) -----

#[tauri::command]
fn read_capture_base64() -> Result<String, String> {
    let path = capture_temp_path();
    let bytes = fs::read(&path).map_err(|e| format!("Failed to read capture: {}", e))?;
    Ok(base64::engine::general_purpose::STANDARD.encode(&bytes))
}

// ----- Overlay lifecycle -----

#[tauri::command]
fn mark_overlay_closed() {
    OVERLAY_ACTIVE.store(false, Ordering::SeqCst);
    log_event("overlay closed");
}

/// Lets the frontend write diagnostics (viewport size, DPR, capture size)
/// into ~/.snap/snap.log alongside the backend's lines.
#[tauri::command]
fn frontend_log(msg: String) {
    log_event(&format!("[ui] {}", msg));
}

// ----- Helpers -----

fn inbox_dir() -> Result<PathBuf, String> {
    let dir = dirs::home_dir()
        .ok_or("Could not determine home directory")?
        .join(".snap")
        .join("inbox");
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create inbox: {}", e))?;
    Ok(dir)
}

pub fn log_event(msg: &str) {
    let log_dir = dirs::home_dir()
        .map(|h| h.join(".snap"))
        .unwrap_or_else(|| std::env::temp_dir());
    let _ = fs::create_dir_all(&log_dir);
    let log_path = log_dir.join("snap.log");

    // Rotate if over 1MB
    if let Ok(meta) = fs::metadata(&log_path) {
        if meta.len() > 1_000_000 {
            let backup = log_dir.join("snap.log.old");
            let _ = fs::rename(&log_path, &backup);
        }
    }

    let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let line = format!("[{}] {}\n", timestamp, msg);
    let _ = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .and_then(|mut f| {
            use std::io::Write;
            f.write_all(line.as_bytes())
        });
}

// ----- Overlay window -----

/// Common settings for the fullscreen annotation window. Callers add the
/// platform-specific visibility/transparency flags before building.
pub fn overlay_window_builder(
    handle: &tauri::AppHandle,
) -> tauri::WebviewWindowBuilder<'_, tauri::Wry, tauri::AppHandle> {
    tauri::WebviewWindowBuilder::new(
        handle,
        "overlay",
        tauri::WebviewUrl::App("index.html".into()),
    )
    .title("Snap")
    .decorations(false)
    .always_on_top(true)
    .skip_taskbar(true)
    // Deliberately resizable: GTK turns resizable(false) into fixed size
    // hints, and some compositors then refuse to fullscreen the window.
    // With no decorations the user cannot resize it anyway.
}

// ----- Public: generate the invoke handler -----

pub fn invoke_handler() -> impl Fn(tauri::ipc::Invoke) -> bool {
    tauri::generate_handler![
        capture_screen,
        get_active_window_context,
        save_annotation,
        read_capture_base64,
        mark_overlay_closed,
        frontend_log,
    ]
}
