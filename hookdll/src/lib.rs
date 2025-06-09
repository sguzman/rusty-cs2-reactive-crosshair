#![allow(non_snake_case)]
use std::sync::Once;
use std::{ffi::c_void, ptr};
use std::thread;

use detour::static_detour;
use imgui::*;
use imgui_dx11_renderer::Renderer as Dx11Renderer;
use windows::Win32::{
    Foundation::{BOOL, HINSTANCE, S_OK},
    Graphics::Direct3D11::*,
    Graphics::Dxgi::Common::DXGI_FORMAT_R8G8B8A8_UNORM,
    Graphics::Dxgi::{IDXGISwapChain, DXGI_SWAP_CHAIN_DESC},
    System::LibraryLoader::GetModuleHandleA,
    System::Threading::{CreateThread, DisableThreadLibraryCalls},
};

// 1) Declare the hook type: matches IDXGISwapChain::Present signature
static_detour! {
    static PresentHook: unsafe extern "system" fn(
        swap: *mut IDXGISwapChain,
        sync: u32,
        flags: u32
    ) -> HRESULT;
}

static INIT: Once = Once::new();
static mut IMGUI_CONTEXT: *mut Ui = ptr::null_mut();
static mut DX11_RENDERER: Option<Dx11Renderer> = None;

// This is your DllMain replacement
#[no_mangle]
pub extern "system" fn DllMain(hinst: HINSTANCE, reason: u32, _: *mut c_void) -> BOOL {
    match reason {
        1 /*DLL_PROCESS_ATTACH*/ => {
            unsafe {
                DisableThreadLibraryCalls(hinst);
                // spawn setup on a new thread
                CreateThread(
                    ptr::null_mut(),
                    0,
                    Some(setup_hook),
                    ptr::null_mut(),
                    0,
                    ptr::null_mut(),
                );
            }
        }
        0 /*DLL_PROCESS_DETACH*/ => {
            unsafe { PresentHook.disable().unwrap(); }
        }
        _ => {}
    }
    BOOL(1)
}

// Thread entry: initialize MinHook + attach to Present
unsafe extern "system" fn setup_hook(_: *mut c_void) -> u32 {
    // 1) Create temporary device+swapchain to sniff out the Present vtable slot
    let mut device: *mut ID3D11Device = ptr::null_mut();
    let mut context: *mut ID3D11DeviceContext = ptr::null_mut();
    let mut swapchain: *mut IDXGISwapChain = ptr::null_mut();

    let desc = DXGI_SWAP_CHAIN_DESC {
        BufferCount: 1,
        BufferDesc: Default::default(),
        BufferUsage: 0x20, // DXGI_USAGE_RENDER_TARGET_OUTPUT
        OutputWindow: windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow(),
        SampleDesc: Default::default(),
        SwapEffect: Default::default(),
        Windowed: true.into(),
        BufferDescFormat: DXGI_FORMAT_R8G8B8A8_UNORM,
        ..Default::default()
    };

    D3D11CreateDeviceAndSwapChain(
        ptr::null_mut(),
        D3D_DRIVER_TYPE_HARDWARE,
        ptr::null_mut(),
        0,
        ptr::null(),
        0,
        D3D11_SDK_VERSION,
        &desc,
        &mut swapchain,
        &mut device,
        ptr::null_mut(),
        &mut context,
    ).unwrap();

    // 2) Grab the vtable pointer & locate Present at index 8
    let vtbl = *(swapchain as *mut *mut *mut c_void);
    let present_addr = vtbl.add(8).read() as *const ();

    // 3) Initialize our detour
    PresentHook.initialize(
        std::mem::transmute(present_addr),
        hook_present
    ).unwrap();
    PresentHook.enable().unwrap();

    // release dummy
    (*swapchain).Release();
    (*device).Release();
    (*context).Release();

    0
}

// Our detour callback
unsafe extern "system" fn hook_present(
    swap: *mut IDXGISwapChain,
    sync: u32,
    flags: u32
) -> HRESULT {
    // one-time ImGui + renderer init
    INIT.call_once(|| {
        // grab device & context
        let mut dev = ptr::null_mut();
        let mut ctx = ptr::null_mut();
        (*swap).GetDevice(&ID3D11Device::IID, &mut dev as *mut _ as _).unwrap();
        (*(dev as *mut ID3D11Device)).GetImmediateContext(&mut ctx);

        let mut desc = DXGI_SWAP_CHAIN_DESC::default();
        (*swap).GetDesc(&mut desc).unwrap();

        let mut imgui = Context::create();
        let h_wnd = desc.OutputWindow;
        imgui.set_ini_filename(None);
        let mut platform = imgui_winit_support::WinitPlatform::init(&mut imgui);
        // we don’t have a winit::Window, but we only need Win32 init:
        platform.attach_window(imgui.io_mut(), h_wnd, imgui_winit_support::HiDpiMode::Rounded);

        let renderer = Dx11Renderer::new(&mut imgui, dev, ctx).unwrap();

        IMGUI_CONTEXT = Box::into_raw(Box::new(imgui));
        DX11_RENDERER = Some(renderer);
    });

    // every frame: start new frame, draw crosshair, render
    let imgui = &mut *IMGUI_CONTEXT;
    let renderer = DX11_RENDERER.as_mut().unwrap();

    let mut io = imgui.io_mut();
    let sz = io.display_size;
    io.delta_time = 1.0 / 60.0;

    imgui.new_frame();

    // Draw your crosshair (always white here for demo)
    let draw = imgui.get_background_draw_list();
    let center = [sz[0] * 0.5, sz[1] * 0.5];
    draw.add_line([center[0] - 10.0, center[1]], [center[0] + 10.0, center[1]], 0xFFFFFFFF, 2.0);
    draw.add_line([center[0], center[1] - 10.0], [center[0], center[1] + 10.0], 0xFFFFFFFF, 2.0);

    imgui.render();
    renderer.render(imgui.render());

    // call original
    PresentHook.call(swap, sync, flags)
}
