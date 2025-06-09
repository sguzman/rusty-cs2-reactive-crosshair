use std::{env, ffi::OsStr, os::windows::ffi::OsStrExt, ptr};
use windows::Win32::{
    Foundation::CloseHandle,
    System::{
        Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS},
        LibraryLoader::GetModuleHandleW,
        Memory::{VirtualAllocEx, WriteProcessMemory, VirtualFreeEx, MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_READWRITE},
        Threading::{CreateRemoteThread, OpenProcess, WaitForSingleObject, PROCESS_CREATE_THREAD, PROCESS_VM_OPERATION, PROCESS_VM_WRITE, PROCESS_VM_READ},
    }
};

fn find_pid(exe: &str) -> Option<u32> {
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).ok()? };
    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let exe_w: Vec<u16> = OsStr::new(exe).encode_wide().chain(Some(0)).collect();

    unsafe {
        if Process32FirstW(snapshot, &mut entry).as_bool() {
            loop {
                if &entry.szExeFile[..] == exe_w.as_slice() {
                    CloseHandle(snapshot);
                    return Some(entry.th32ProcessID);
                }
                if !Process32NextW(snapshot, &mut entry).as_bool() {
                    break;
                }
            }
        }
        CloseHandle(snapshot);
    }
    None
}

fn to_wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(Some(0)).collect()
}

fn main() -> windows::core::Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: injector.exe <proc.exe> <full_path_to_dll>");
        return Ok(());
    }
    let pid = find_pid(&args[1]).expect("Process not found");
    let dll_path = to_wide(&args[2]);

    // open process
    let proc = unsafe {
        OpenProcess(PROCESS_CREATE_THREAD | PROCESS_VM_OPERATION | PROCESS_VM_WRITE | PROCESS_VM_READ, false, pid)
            .expect("OpenProcess failed")
    };

    // alloc & write path
    let size = (dll_path.len() * std::mem::size_of::<u16>()) as usize;
    let remote_mem = unsafe {
        VirtualAllocEx(proc, ptr::null(), size, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE)
            .expect("VirtualAllocEx failed")
    };
    unsafe { WriteProcessMemory(proc, remote_mem, dll_path.as_ptr() as _, size, ptr::null_mut())? };

    // get LoadLibraryW address
    let k32 = unsafe { GetModuleHandleW(windows::w!("kernel32.dll"))? };
    let loadlib = unsafe { windows::Win32::System::LibraryLoader::GetProcAddress(k32, b"LoadLibraryW\0".as_ptr() as *const _)
        .expect("GetProcAddress failed") };

    // inject
    let thread = unsafe { CreateRemoteThread(proc, ptr::null(), 0, Some(std::mem::transmute(loadlib)), remote_mem, 0, ptr::null_mut())? };
    unsafe { WaitForSingleObject(thread, u32::MAX); }
    unsafe { 
        VirtualFreeEx(proc, remote_mem, 0, MEM_RELEASE)?;
        CloseHandle(thread);
        CloseHandle(proc);
    }
    println!("DLL injected. Enjoy your crosshair!");
    Ok(())
}
