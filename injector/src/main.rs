// src/main.rs
use windows::Win32::{
    Foundation::HWND,
    System::{Diagnostics::Debug::OutputDebugStringA, Threading::GetCurrentThreadId},
};

fn main() {
    unsafe {
        OutputDebugStringA("injector: Hello from injector!\0".as_ptr() as _);
        // no actual injection yet
        let tid = GetCurrentThreadId();
        OutputDebugStringA(format!("injector: TID={} - exiting\n\0", tid).as_ptr() as _);
    }
}
