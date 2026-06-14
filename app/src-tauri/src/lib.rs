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
        // Linux: try capture tools in order of preference
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

        for (tool, args) in &methods {
            match Command::new(tool).args(args).output() {
                Ok(output) if output.status.success() => {
                    match fs::metadata(&path) {
                        Ok(meta) if meta.len() > 0 => {
                            log_event(&format!("screen captured via {}", tool));
                            return Ok(path_str.clone());
                        }
                        _ => {
                            last_error = format!("{} produced empty file", tool);
                            continue;
                        }
                    }
                }
                Ok(output) => {
                    last_error = format!(
                        "{} failed: {}",
                        tool,
                        String::from_utf8_lossy(&output.stderr)
                    );
                    continue;
                }
                Err(_) => continue, // tool not installed, try next
            }
        }

        Err(format!(
            "Screen capture failed: {}. Install one of: sudo apt install gnome-screenshot grim scrot",
            last_error
        ))
    }
}

// ----- Window Context -----

#[derive(Serialize, Clone)]
struct WindowContext {
    window_title: Option<String>,
    url: Option<String>,
    window_class: Option<String>,
    pid: Option<u32>,
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
        let ctx = get_active_window_context_macos().unwrap_or(WindowContext {
            window_title: None,
            url: None,
            window_class: None,
            pid: None,
        });
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
            return Ok(WindowContext {
                window_title: None,
                url: None,
                window_class: None,
                pid: None,
            });
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
            return WindowContext {
                window_title: None,
                url: None,
                window_class: None,
                pid: None,
            };
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

// ----- Public: generate the invoke handler -----

pub fn invoke_handler() -> impl Fn(tauri::ipc::Invoke) -> bool {
    tauri::generate_handler![
        capture_screen,
        get_active_window_context,
        save_annotation,
        read_capture_base64,
        mark_overlay_closed,
    ]
}
