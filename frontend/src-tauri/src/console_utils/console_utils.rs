#[cfg(target_os = "windows")]
use std::ptr;
#[cfg(target_os = "macos")]
use std::process::Command;

/// Open the folder containing the app's log files (meetily.log) in the
/// system file manager. Works in packaged builds - this is the primary
/// way for end users to retrieve logs for troubleshooting.
#[tauri::command]
pub fn open_logs_folder<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> Result<String, String> {
    use tauri::Manager;
    let log_dir = app
        .path()
        .app_log_dir()
        .map_err(|e| format!("Failed to resolve log dir: {}", e))?;
    std::fs::create_dir_all(&log_dir).map_err(|e| format!("Failed to create log dir: {}", e))?;

    #[cfg(target_os = "macos")]
    let opener = "open";
    #[cfg(target_os = "windows")]
    let opener = "explorer";
    #[cfg(target_os = "linux")]
    let opener = "xdg-open";

    std::process::Command::new(opener)
        .arg(&log_dir)
        .spawn()
        .map_err(|e| format!("Failed to open log dir: {}", e))?;

    Ok(log_dir.to_string_lossy().to_string())
}

#[cfg(target_os = "windows")]
#[link(name = "kernel32")]
extern "system" {
    fn AllocConsole() -> i32;
    #[allow(dead_code)]
    fn FreeConsole() -> i32;
    fn GetConsoleWindow() -> *mut std::ffi::c_void;
    fn ShowWindow(hwnd: *mut std::ffi::c_void, n_cmd_show: i32) -> i32;
}

#[cfg(target_os = "windows")]
const SW_HIDE: i32 = 0;
#[cfg(target_os = "windows")]
const SW_SHOW: i32 = 5;

#[tauri::command]
pub fn show_console() -> Result<String, String> {
    #[cfg(target_os = "windows")]
    unsafe {
        let console_window = GetConsoleWindow();
        if console_window == ptr::null_mut() {
            // If no console exists, allocate one
            if AllocConsole() == 0 {
                return Err("Failed to allocate console".to_string());
            }
            // Logger is already initialized by tauri-plugin-log in lib.rs;
            // its Stdout target writes into the newly allocated console.
        } else {
            // Show existing console window
            ShowWindow(console_window, SW_SHOW);
        }
        Ok("Console shown".to_string())
    }
    
    #[cfg(target_os = "macos")]
    {
        // On macOS, we'll open Terminal.app with our app's logs
        // First, get the app name from the bundle
        match Command::new("osascript")
            .arg("-e")
            .arg(r#"
                tell application "Terminal"
                    activate
                    do script "log stream --process meetily --level info --style compact"
                end tell
            "#)
            .spawn()
        {
            Ok(_) => Ok("Console opened in Terminal".to_string()),
            Err(e) => Err(format!("Failed to open console: {}", e)),
        }
    }
    
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        Ok("Console control is only available on Windows and macOS".to_string())
    }
}

#[tauri::command]
pub fn hide_console() -> Result<String, String> {
    #[cfg(target_os = "windows")]
    unsafe {
        let console_window = GetConsoleWindow();
        if console_window != ptr::null_mut() {
            ShowWindow(console_window, SW_HIDE);
            Ok("Console hidden".to_string())
        } else {
            Err("No console window found".to_string())
        }
    }
    
    #[cfg(target_os = "macos")]
    {
        // On macOS, we'll close the Terminal window that's showing our logs
        match Command::new("osascript")
            .arg("-e")
            .arg(r#"
                tell application "Terminal"
                    set windowList to windows
                    repeat with aWindow in windowList
                        if contents of selected tab of aWindow contains "log stream --process meetily" then
                            close aWindow
                        end if
                    end repeat
                end tell
            "#)
            .spawn()
        {
            Ok(_) => Ok("Console closed".to_string()),
            Err(e) => Err(format!("Failed to close console: {}", e)),
        }
    }
    
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        Ok("Console control is only available on Windows and macOS".to_string())
    }
}

#[tauri::command]
pub fn toggle_console() -> Result<String, String> {
    #[cfg(target_os = "windows")]
    unsafe {
        let console_window = GetConsoleWindow();
        if console_window == ptr::null_mut() {
            show_console()
        } else {
            // Check if window is visible (this is a simplified approach)
            // In a real implementation, you might want to use GetWindowLong to check visibility
            hide_console()
        }
    }
    
    #[cfg(target_os = "macos")]
    {
        // On macOS, check if Terminal is running with our log stream
        let check_result = Command::new("osascript")
            .arg("-e")
            .arg(r#"
                tell application "Terminal"
                    set windowList to windows
                    repeat with aWindow in windowList
                        if contents of selected tab of aWindow contains "log stream --process meetily" then
                            return "found"
                        end if
                    end repeat
                    return "not found"
                end tell
            "#)
            .output();
            
        match check_result {
            Ok(output) => {
                let output_str = String::from_utf8_lossy(&output.stdout);
                if output_str.trim() == "found" {
                    hide_console()
                } else {
                    show_console()
                }
            }
            Err(_) => show_console()
        }
    }
    
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        Ok("Console control is only available on Windows and macOS".to_string())
    }
}