#![allow(non_snake_case)]
use core::ffi::c_void;
use windows::core::PCSTR;
use windows::Win32::{
    Foundation::{HINSTANCE, BOOL},
    System::Diagnostics::Debug::OutputDebugStringA,
};

#[no_mangle]
pub extern "system" fn DllMain(
    _hinst: HINSTANCE,
    _reason: u32,
    _reserved: *mut c_void
) -> BOOL {
    unsafe {
        // This should show up in DebugView as soon as the DLL loads
        OutputDebugStringA(PCSTR(b"hookdll: DllMain called\0".as_ptr()));
    }
    // Return TRUE so Windows keeps the DLL loaded
    BOOL(1)
}
