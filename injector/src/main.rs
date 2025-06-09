use std::{env, ffi::OsStr, os::windows::ffi::OsStrExt, ptr};
use windows::Win32::{
    Foundation::{CloseHandle, BOOL, HANDLE},
    System::{
        Diagnostics::Debug::{OutputDebugStringA, WriteProcessMemory},
        Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS},
        LibraryLoader::{GetModuleHandleW, GetProcAddress},
        Memory::{VirtualAllocEx, VirtualFreeEx, MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_READWRITE},
        Threading::{CreateRemoteThread, OpenProcess, WaitForSingleObject, PROCESS_CREATE_THREAD, PROCESS_VM_OPERATION, PROCESS_VM_WRITE, PROCESS_VM_READ},
    },
};
use windows::core::PCSTR;

// Find PID by exe name
fn find_pid(exe: &str) -> Option<u32> {
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).ok()? };
    let mut entry = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as _, ..Default::default() };
    let exe_w: Vec<u16> = OsStr::new(exe).encode_wide().chain(Some(0)).collect();

    unsafe {
        if Process32FirstW(snapshot, &mut entry).as_bool() {
            loop {
                if entry.szExeFile.iter().zip(&exe_w).all(|(a, b)| *a as u16 == *b) {
                    CloseHandle(snapshot);
                    return Some(entry.th32ProcessID);
                }
                if !Process32NextW(snapshot, &mut entry).as_bool() { break; }
            }
        }
        CloseHandle(snapshot);
    }
    None
}

// Convert Rust &str → wide
fn to_wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(Some(0)).collect()
}

fn main() -> windows::core::Result<()> {
    OutputDebugStringA("injector: starting up\n\0".as_ptr() as _);

    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: injector.exe <proc.exe> <fullPathToDll>");
        return Ok(());
    }
    let proc_name = &args[1];
    let dll_path  = to_wide(&args[2]);

    let pid = find_pid(proc_name).expect("injector: process not found");
    OutputDebugStringA(&format!("injector: found PID {}\n\0", pid).as_ptr() as _);

    // open process
    let proc: HANDLE = unsafe {
        OpenProcess(
            PROCESS_CREATE_THREAD | PROCESS_VM_OPERATION | PROCESS_VM_WRITE | PROCESS_VM_READ,
            BOOL(0),
            pid,
        )?
    };
    OutputDebugStringA("injector: OpenProcess OK\n\0".as_ptr() as _);

    // alloc remote memory
    let size = (dll_path.len() * std::mem::size_of::<u16>()) as usize;
    let remote_mem = unsafe {
        VirtualAllocEx(proc, Some(ptr::null()), size, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE)?
    };
    OutputDebugStringA("injector: VirtualAllocEx OK\n\0".as_ptr() as _);

    // write DLL path
    let success = unsafe {
        WriteProcessMemory(proc, remote_mem, dll_path.as_ptr() as _, size, ptr::null_mut())
    };
    if !success.as_bool() {
        panic!("injector: WriteProcessMemory failed");
    }
    OutputDebugStringA("injector: WriteProcessMemory OK\n\0".as_ptr() as _);

    // get LoadLibraryW address
    let k32 = unsafe { GetModuleHandleW(windows::w!("kernel32.dll"))? };
    let load_addr = unsafe {
        GetProcAddress(k32, PCSTR(b"LoadLibraryW\0".as_ptr()))?
    };
    OutputDebugStringA("injector: GetProcAddress OK\n\0".as_ptr() as _);

    // spawn remote thread
    let thread = unsafe {
        CreateRemoteThread(proc, None, 0, Some(std::mem::transmute(load_addr)), remote_mem, 0, None)?
    };
    OutputDebugStringA("injector: CreateRemoteThread OK\n\0".as_ptr() as _);

    // wait & clean up
    unsafe {
        WaitForSingleObject(thread, u32::MAX);
        VirtualFreeEx(proc, remote_mem, 0, MEM_RELEASE)?;
        CloseHandle(thread);
        CloseHandle(proc);
    }
    OutputDebugStringA("injector: injection complete\n\0".as_ptr() as _);
    Ok(())
}
