#![allow(non_snake_case)]

use std::{ffi::c_void, ptr, sync::Once};
use windows::{
    core::{HRESULT, PCSTR},
    Win32::{
        Foundation::{BOOL, HINSTANCE, HWND},
        Graphics::{
            Direct3D11::*,
            Dxgi::{Common::*, *},
        },
        System::{
            LibraryLoader::DisableThreadLibraryCalls,
            SystemServices::DLL_PROCESS_ATTACH,
            Threading::CreateThread,
        },
    },
};
use detour::static_detour;
use imgui::{Context, FontConfig, FontSource};
use imgui_dx11_renderer::Renderer;

// Hook Definition
static_detour! {
    static PresentHook: unsafe extern "system" fn(*mut IDXGISwapChain, u32, u32) -> HRESULT;
}

static INIT: Once = Once::new();
static mut IMGUI_CONTEXT: *mut Context = ptr::null_mut();
static mut RENDERER: Option<Renderer> = None;
static mut HWND: HWND = HWND(0);

// DllMain
#[no_mangle]
pub extern "system" fn DllMain(hinst: HINSTANCE, reason: u32, _: *const c_void) -> BOOL {
    if reason == DLL_PROCESS_ATTACH {
        unsafe {
            DisableThreadLibraryCalls(hinst);
            CreateThread(None, 0, Some(setup_hook), None, 0, None).unwrap();
        }
    }
    BOOL(1)
}

// Hook setup
unsafe extern "system" fn setup_hook(_: *mut c_void) -> u32 {
    // To get the Present function pointer, we need to create a dummy device and swap chain.
    let mut p_device: Option<ID3D11Device> = None;
    let mut p_context: Option<ID3D11DeviceContext> = None;
    let mut p_swap_chain: Option<IDXGISwapChain> = None;

    let mut swap_chain_desc = DXGI_SWAP_CHAIN_DESC::default();
    swap_chain_desc.BufferCount = 1;
    swap_chain_desc.BufferDesc.Format = DXGI_FORMAT_R8G8B8A8_UNORM;
    swap_chain_desc.BufferUsage = DXGI_USAGE_RENDER_TARGET_OUTPUT;
    swap_chain_desc.OutputWindow = get_main_window(); // Find a real window
    swap_chain_desc.SampleDesc.Count = 1;
    swap_chain_desc.Windowed = true.into();

    let feature_level = D3D_FEATURE_LEVEL_11_0;

    if D3D11CreateDeviceAndSwapChain(
        None,
        D3D_DRIVER_TYPE_HARDWARE,
        None,
        0,
        Some(&feature_level),
        1,
        D3D11_SDK_VERSION,
        Some(&swap_chain_desc),
        Some(&mut p_swap_chain),
        Some(&mut p_device),
        None,
        Some(&mut p_context),
    )
    .is_err()
    {
        return 1;
    }

    let swap_chain = p_swap_chain.unwrap();
    let vtable = swap_chain.vtable();
    let present_fn = vtable.Present;

    PresentHook
        .initialize(std::mem::transmute(present_fn), hooked_present)
        .unwrap();
    PresentHook.enable().unwrap();

    0
}

// This is a helper to find the game window.
// You might need a more robust way for a real game.
fn get_main_window() -> HWND {
    unsafe {
        // Find window by class or title. For CS2, you may need to find the correct window class name.
        // As a fallback, this code is not provided since it can be complex.
        // A simple approach is to get the foreground window, but it's not reliable.
        windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow()
    }
}


// Our hooked Present function
unsafe extern "system" fn hooked_present(
    p_swap_chain: *mut IDXGISwapChain,
    sync_interval: u32,
    flags: u32,
) -> HRESULT {
    let swap_chain = &*p_swap_chain;

    INIT.call_once(|| {
        let mut device: Option<ID3D11Device> = None;
        swap_chain.GetDevice(&mut device).unwrap();
        let mut context: Option<ID3D11DeviceContext> = None;
        device.as_ref().unwrap().GetImmediateContext(&mut context);

        let mut sd = DXGI_SWAP_CHAIN_DESC::default();
        swap_chain.GetDesc(&mut sd).unwrap();
        HWND = sd.OutputWindow;

        let mut imgui = Context::create();
        imgui.set_ini_filename(None);
        
        // This is important to get input working later if you want.
        imgui.io_mut().backend_flags |= imgui::BackendFlags::RENDERER_HAS_VTX_OFFSET;

        // Setup font
        imgui.fonts().add_font(&[FontSource::DefaultFontData { config: Some(FontConfig { size_pixels: 16.0, ..Default::default() }) }]);

        IMGUI_CONTEXT = Box::into_raw(Box::new(imgui));
        RENDERER = Some(Renderer::new(
            &mut *IMGUI_CONTEXT,
            device.unwrap(),
            context.unwrap(),
        ).unwrap());
    });
    
    // Each frame
    let imgui = &mut *IMGUI_CONTEXT;
    let renderer = RENDERER.as_mut().unwrap();

    // Update display size from swap chain
    let mut sd = DXGI_SWAP_CHAIN_DESC::default();
    swap_chain.GetDesc(&mut sd).unwrap();
    imgui.io_mut().display_size = [sd.BufferDesc.Width as f32, sd.BufferDesc.Height as f32];

    imgui.new_frame();

    // Draw a white crosshair
    let draw_list = imgui.get_background_draw_list();
    let [w, h] = imgui.io().display_size;
    let (cx, cy) = (w * 0.5, h * 0.5);
    draw_list.add_line([cx - 10.0, cy], [cx + 10.0, cy], 0xFFFFFFFF).thickness(2.0).build();
    draw_list.add_line([cx, cy - 10.0], [cx, cy + 10.0], 0xFFFFFFFF).thickness(2.0).build();

    imgui.render();
    renderer.render(imgui.render()).unwrap();

    PresentHook.call(p_swap_chain, sync_interval, flags)
}