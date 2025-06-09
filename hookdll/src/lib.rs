// /rusty-cs2-reactive-crosshair/hookdll/src/lib.rs
use std::ffi::c_void;
use std::fs::OpenOptions;
use std::io::Write;
use std::panic;
use std::time::SystemTime;
use windows::Win32::Foundation::{BOOL, HINSTANCE};
use windows::Win32::System::SystemServices::{DLL_PROCESS_ATTACH, DLL_PROCESS_DETACH};

// This function is the entry point of the DLL.
// It is called by the OS when the DLL is loaded or unloaded.
#[no_mangle]
pub extern "system" fn DllMain(
    _hinst: HINSTANCE, // Handle to the DLL module
    reason: u32,       // The reason the function is being called
    _reserved: *mut c_void,
) -> BOOL {
    // Set a panic hook to log any panics that might occur in the DLL.
    panic::set_hook(Box::new(|panic_info| {
        log_to_file(&format!("A panic occurred: {:?}", panic_info));
    }));

    match reason {
        DLL_PROCESS_ATTACH => {
            log_to_file("DLL Injected Successfully!");
            // Here you would start a new thread to do the actual hooking.
            // For now, we just log.
        }
        DLL_PROCESS_DETACH => {
            log_to_file("DLL Unloaded.");
        }
        _ => {}
    }

    // Return TRUE to indicate success.
    BOOL(1)
}

/// A helper function to write log messages to a file.
/// This gives us "comprehensive logging" to see what's happening.
fn log_to_file(message: &str) {
    // Open the log file in append mode. Create it if it doesn't exist.
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open("C:\\temp\\hookdll_log.txt")
    // Using a predictable location
    {
        // Format the log entry with a timestamp.
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let log_entry = format!("[{}] {}\n", timestamp, message);

        // Write the log entry to the file.
        let _ = file.write_all(log_entry.as_bytes());
    }
}
