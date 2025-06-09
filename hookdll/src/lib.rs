#![allow(non_snake_case)]

use std::{ffi::c_void, ptr, sync::Once};
use windows::{
    core::HRESULT,
    Win32::{
        Foundation::{BOOL, HINSTANCE, HWND},
        Graphics::{
            Direct3D11::*,
            Dxgi::{Common::*, *},
        },
        System::{
            LibraryLoader::DisableThreadLibraryCalls,
            Memory::{
                VirtualAlloc, VirtualProtect, MEM_COMMIT, MEM_RESERVE, PAGE_EXECUTE_READWRITE,
                PAGE_PROTECTION_FLAGS,
            },
            SystemServices::{DLL_PROCESS_ATTACH, DLL_PROCESS_DETACH},
            Threading::CreateThread,
        },
    },
};
use imgui::{Context, FontConfig, FontSource};
use imgui_dx11_renderer::Renderer;

// Type alias for the function pointer we want to hook
type FnPresent = unsafe extern "system" fn(*mut IDXGISwapChain, u32, u32) -> HRESULT;

// A global static to hold the pointer to the original Present function
static mut O_PRESENT: FnPresent = present_stub;

// A stub function to initialize O_PRESENT to something safe
unsafe extern "system" fn present_stub(
    _p_swap_chain: *mut IDXGISwapChain,
    _sync_interval: u32,
    _flags: u32,
) -> HRESULT {
    HRESULT(0)
}

static INIT: Once = Once::new();
static mut IMGUI_CONTEXT: *mut Context = ptr::null_mut();
static mut RENDERER: Option<Renderer> = None;
static mut HOOK_DATA: Option<Hook> = None;

struct Hook {
    target: *mut c_void,
    original_bytes: [u8; 12],
}

impl Hook {
    // Overwrites the target function with a JMP to our hook
    unsafe fn install(target: *mut c_void, detour: *mut c_void) -> Result<Self, ()> {
        let mut original_bytes: [u8; 12] = [0; 12];
        ptr::copy_nonoverlapping(target, original_bytes.as_mut_ptr() as _, 12);

        // This is the x86-64 assembly for:
        // mov rax, <address>
        // jmp rax
        let mut patch: [u8; 12] = [
            0x48, 0xB8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xE0,
        ];
        // Write the address of our hook function into the patch
        *(patch.as_mut_ptr().add(2) as *mut u64) = detour as u64;

        // Change memory protection to write the patch
        let mut old_protect = PAGE_PROTECTION_FLAGS(0);
        VirtualProtect(target, 12, PAGE_EXECUTE_READWRITE, &mut old_protect);
        ptr::copy_nonoverlapping(patch.as_ptr(), target as _, 12);
        VirtualProtect(target, 12, old_protect, &mut old_protect);

        Ok(Hook { target, original_bytes })
    }

    // Restores the original function bytes
    unsafe fn uninstall(&self) {
        let mut old_protect = PAGE_PROTECTION_FLAGS(0);
        VirtualProtect(self.target, 12, PAGE_EXECUTE_READWRITE, &mut old_protect);
        ptr::copy_nonoverlapping(self.original_bytes.as_ptr(), self.target as _, 12);
        VirtualProtect(self.target, 12, old_protect, &mut old_protect);
    }
}


#[no_mangle]
pub extern "system" fn DllMain(hinst: HINSTANCE, reason: u32, _: *const c_void) -> BOOL {
    unsafe {
        match reason {
            DLL_PROCESS_ATTACH => {
                DisableThreadLibraryCalls(hinst);
                CreateThread(None, 0, Some(setup_hook), None, 0, None).unwrap();
            }
            DLL_PROCESS_DETACH => {
                if let Some(hook) = HOOK_DATA.take() {
                    hook.uninstall();
                }
            }
            _ => (),
        }
    }
    BOOL(1)
}

fn get_main_window() -> HWND {
    unsafe { windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow() }
}

unsafe extern "system" fn setup_hook(_: *mut c_void) -> u32 {
    let mut p_device: *mut ID3D11Device = ptr::null_mut();
    let mut p_context: *mut ID3D11DeviceContext = ptr::null_mut();
    let mut p_swap_chain: *mut IDXGISwapChain = ptr::null_mut();

    let mut swap_chain_desc = DXGI_SWAP_CHAIN_DESC::default();
    swap_chain_desc.BufferCount = 1;
    swap_chain_desc.BufferDesc.Format = DXGI_FORMAT_R8G8B8A8_UNORM;
    swap_chain_desc.BufferUsage = DXGI_USAGE_RENDER_TARGET_OUTPUT;
    swap_chain_desc.OutputWindow = get_main_window();
    swap_chain_desc.SampleDesc.Count = 1;
    swap_chain_desc.Windowed = true.into();

    let feature_level = D3D_FEATURE_LEVEL_11_0;

    D3D11CreateDeviceAndSwapChain(
        None, D3D_DRIVER_TYPE_HARDWARE, None, 0, Some(&feature_level), 1, D3D11_SDK_VERSION,
        Some(&swap_chain_desc), Some(&mut p_swap_chain as *mut _ as _), Some(&mut p_device as *mut _ as _),
        None, Some(&mut p_context as *mut _ as _),
    ).unwrap();

    // Get the vtable from the swap chain object
    let vtable = (*(p_swap_chain as *mut *mut *mut c_void)).read();

    // Get the pointer to the original Present function (it's the 8th function in the vtable)
    let present_fn_ptr = (*vtable.add(8)).read() as *mut c_void;

    // Create the trampoline that calls the original function
    let trampoline_mem = VirtualAlloc(None, 24, MEM_COMMIT | MEM_RESERVE, PAGE_EXECUTE_READWRITE);
    if trampoline_mem.is_null() { return 1; }

    let original_fn_bytes = present_fn_ptr as *const u8;
    ptr::copy_nonoverlapping(original_fn_bytes, trampoline_mem as _, 12);

    let jmp_back_addr = original_fn_bytes.add(12) as u64;
    let mut jmp_patch: [u8; 12] = [
        0x48, 0xB8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xE0,
    ];
    *(jmp_patch.as_mut_ptr().add(2) as *mut u64) = jmp_back_addr;
    ptr::copy_nonoverlapping(jmp_patch.as_ptr(), (trampoline_mem as *mut u8).add(12) as _, 12);

    O_PRESENT = std::mem::transmute(trampoline_mem);

    // Install the hook
    let hook = Hook::install(present_fn_ptr, hooked_present as *mut c_void).unwrap();
    HOOK_DATA = Some(hook);

    (*p_context).Release();
    (*p_device).Release();
    (*p_swap_chain).Release();
    0
}


unsafe extern "system" fn hooked_present(p_swap_chain: *mut IDXGISwapChain, sync_interval: u32, flags: u32) -> HRESULT {
    INIT.call_once(|| {
        let mut p_device: *mut ID3D11Device = ptr::null_mut();
        let mut p_context: *mut ID3D11DeviceContext = ptr::null_mut();
        (*p_swap_chain).GetDevice(&ID3D11Device::IID, &mut p_device as *mut _ as _).unwrap();
        (*p_device).GetImmediateContext(&mut p_context);

        let mut imgui = Context::create();
        imgui.set_ini_filename(None);
        imgui.fonts().add_font(&[FontSource::DefaultFontData { config: Some(FontConfig { size_pixels: 16.0, ..Default::default() }) }]);

        IMGUI_CONTEXT = Box::into_raw(Box::new(imgui));
        RENDERER = Some(Renderer::new(&mut *IMGUI_CONTEXT, p_device, p_context));
    });

    let imgui = &mut *IMGUI_CONTEXT;
    let renderer = RENDERER.as_mut().unwrap();

    let mut sd = DXGI_SWAP_CHAIN_DESC::default();
    (*p_swap_chain).GetDesc(&mut sd).unwrap();
    imgui.io_mut().display_size = [sd.BufferDesc.Width as f32, sd.BufferDesc.Height as f32];
    imgui.new_frame();

    let draw_list = imgui.get_background_draw_list();
    let [w, h] = imgui.io().display_size;
    let (cx, cy) = (w * 0.5, h * 0.5);
    draw_list.add_line([cx - 10.0, cy], [cx + 10.0, cy], 0xFFFFFFFF, 2.0);
    draw_list.add_line([cx, cy - 10.0], [cx, cy + 10.0], 0xFFFFFFFF, 2.0);

    renderer.render(imgui.render());

    // Call the original Present function via our trampoline
    O_PRESENT(p_swap_chain, sync_interval, flags)
}