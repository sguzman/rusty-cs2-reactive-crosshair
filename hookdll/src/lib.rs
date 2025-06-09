// src/lib.rs
#![allow(non_snake_case)]

use core::ffi::c_void;
use windows::Win32::{
    Foundation::{HINSTANCE, BOOL},
    System::{
        LibraryLoader::GetModuleHandleA,
        Threading::DisableThreadLibraryCalls,
        Diagnostics::Debug::OutputDebugStringA,
    },
};

#[no_mangle]
pub extern "system" fn DllMain(hinst: HINSTANCE, reason: u32, _: *mut c_void) -> BOOL {
    unsafe {
        OutputDebugStringA("hookdll: DllMain()\0".as_ptr() as _);
        if reason == 1 /*DLL_PROCESS_ATTACH*/ {
            DisableThreadLibraryCalls(hinst);
            OutputDebugStringA("hookdll: Attached successfully\n\0".as_ptr() as _);
        }
    }
    // TRUE
    BOOL(1)
}
