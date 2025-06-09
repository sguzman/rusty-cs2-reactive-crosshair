use std::{
    env,
    ffi::{OsStr, c_void},
    os::windows::ffi::OsStrExt,
}; // Removed unused 'ptr'
use windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE},
        System::{
            Diagnostics::Debug::WriteProcessMemory,
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
                TH32CS_SNAPPROCESS,
            },
            LibraryLoader::{GetModuleHandleW, GetProcAddress},
            Memory::{
                MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_READWRITE, VirtualAllocEx, VirtualFreeEx,
            },
            Threading::{
                CreateRemoteThread, OpenProcess, PROCESS_CREATE_THREAD, PROCESS_VM_OPERATION,
                PROCESS_VM_READ, PROCESS_VM_WRITE, WaitForSingleObject,
            },
        },
    },
    core::{PCSTR, PCWSTR, Result},
};

fn find_pid(exe: &str) -> Option<u32> {
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).ok()? };
    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let exe_w: Vec<u16> = OsStr::new(exe).encode_wide().collect();

    unsafe {
        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                let mut len = 0;
                while len < entry.szExeFile.len() && entry.szExeFile[len] != 0 {
                    len += 1;
                }
                let remote_exe_name = &entry.szExeFile[..len];

                if remote_exe_name == exe_w.as_slice() {
                    CloseHandle(snapshot).ok();
                    return Some(entry.th32ProcessID);
                }

                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }
        CloseHandle(snapshot).ok();
    }
    None
}

fn to_wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(Some(0)).collect()
}

fn main() -> Result<()> {
    println!("injector: starting up");

    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: injector.exe <proc.exe> <fullPathToDll>");
        eprintln!("Example: injector.exe cs2.exe C:\\path\\to\\hookdll.dll");
        return Ok(());
    }
    let proc_name = &args[1];
    let dll_path = &args[2];
    let dll_path_wide = to_wide(dll_path);

    let pid = find_pid(proc_name)
        .unwrap_or_else(|| panic!("injector: process '{}' not found", proc_name));
    println!("injector: found PID {}", pid);

    let proc: HANDLE = unsafe {
        OpenProcess(
            PROCESS_CREATE_THREAD | PROCESS_VM_OPERATION | PROCESS_VM_WRITE | PROCESS_VM_READ,
            false,
            pid,
        )?
    };
    println!("injector: OpenProcess OK");

    let size = dll_path_wide.len() * std::mem::size_of::<u16>();
    let remote_mem: *mut c_void =
        unsafe { VirtualAllocEx(proc, None, size, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE) };
    if remote_mem.is_null() {
        return Err(windows::core::Error::from_win32());
    }
    println!("injector: VirtualAllocEx OK");

    let mut bytes_written = 0;
    unsafe {
        WriteProcessMemory(
            proc,
            remote_mem,
            dll_path_wide.as_ptr() as *const c_void,
            size,
            Some(&mut bytes_written),
        )?;
    }
    if bytes_written != size {
        panic!(
            "injector: WriteProcessMemory failed: wrote {} of {} bytes",
            bytes_written, size
        );
    }
    println!("injector: WriteProcessMemory OK");

    let k32 = unsafe { GetModuleHandleW(PCWSTR(to_wide("kernel32.dll").as_ptr()))? };
    let load_addr = unsafe {
        GetProcAddress(k32, PCSTR(b"LoadLibraryW\0".as_ptr()))
        // The .into() is removed here to fix the compilation error
    }
    .ok_or_else(|| {
        windows::core::Error::new(
            windows::core::HRESULT(0),
            "GetProcAddress failed for LoadLibraryW",
        )
    })?;

    let thread = unsafe {
        CreateRemoteThread(
            proc,
            None,
            0,
            Some(std::mem::transmute(load_addr)),
            Some(remote_mem),
            0,
            None,
        )?
    };
    println!("injector: CreateRemoteThread OK");

    unsafe {
        WaitForSingleObject(thread, u32::MAX);
        VirtualFreeEx(proc, remote_mem, 0, MEM_RELEASE)?;
        CloseHandle(thread)?;
        CloseHandle(proc)?;
    }
    println!("injector: injection complete");
    Ok(())
}
