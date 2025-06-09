#![allow(unused_imports)]
use windows::core::PCSTR;
use windows::Win32::System::Diagnostics::Debug::OutputDebugStringA;

fn main() {
    unsafe {
        // These two lines should appear in DebugView when you run injector.exe
        OutputDebugStringA(PCSTR(b"injector: Hello from injector!\0".as_ptr()));
        OutputDebugStringA(PCSTR(b"injector: exiting\0".as_ptr()));
    }
}
