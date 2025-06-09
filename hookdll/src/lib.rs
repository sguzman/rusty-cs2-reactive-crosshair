#![allow(non_snake_case)]
use std::{ffi::c_void, ptr, sync::Once};
use windows::Win32::{
    Foundation::HWND,
    Graphics::{
        Direct3D11::*, Dxgi::Common::*, Dxgi::*, 
    },
    System::{
        Diagnostics::Debug::OutputDebugStringA,
        LibraryLoader::GetModuleHandleA,
        Threading::{CreateThread, DisableThreadLibraryCalls},
    },
};
use detour::static_detour;
use imgui::*;                          // 0.7.0
use imgui_dx11_renderer::Renderer;    // 0.7.0
use imgui_winit_support::{WinitPlatform, HiDpiMode}; // 0.7.1

// ─────── Hook Definition ───────
static_detour! {
    static PresentHook: unsafe extern "system" fn(
        swap: *mut IDXGISwapChain,
        sync: u32,
        flags: u32
    ) -> HRESULT;
}

static INIT: Once = Once::new();
static mut IMGUI: *mut Context = ptr::null_mut();
static mut RDR: Option<Renderer> = None;

// ─────── DllMain ───────
#[no_mangle]
pub extern "system" fn DllMain(hinst: HINSTANCE, reason: u32, _: *mut c_void) -> BOOL {
    unsafe {
        OutputDebugStringA("hookdll: DllMain()\0".as_ptr() as _);
        match reason {
            1 /* DLL_PROCESS_ATTACH */ => {
                DisableThreadLibraryCalls(hinst);
                CreateThread(
                    ptr::null_mut(),
                    0,
                    Some(setup_hook),
                    ptr::null_mut(),
                    0,
                    ptr::null_mut(),
                );
            }
            0 /* DLL_PROCESS_DETACH */ => {
                PresentHook.disable().ok();
                OutputDebugStringA("hookdll: Hook disabled\n\0".as_ptr() as _);
            }
            _ => {}
        }
    }
    BOOL(1)
}

// ─────── hook setup ───────
unsafe extern "system" fn setup_hook(_: *mut c_void) -> u32 {
    OutputDebugStringA("hookdll: setup_hook()\0".as_ptr() as _);

    // Create dummy D3D11 device+swapchain to get the vtable
    let mut dev: *mut ID3D11Device = ptr::null_mut();
    let mut ctx: *mut ID3D11DeviceContext = ptr::null_mut();
    let mut swap: *mut IDXGISwapChain = ptr::null_mut();

    let mut desc = DXGI_SWAP_CHAIN_DESC::default();
    desc.BufferCount = 1;
    desc.BufferDesc.Format = DXGI_FORMAT_R8G8B8A8_UNORM;
    desc.BufferUsage = DXGI_USAGE_RENDER_TARGET_OUTPUT;
    desc.OutputWindow = GetModuleHandleA(ptr::null()).0 as HWND; // fallback
    desc.SampleDesc.Count = 1;
    desc.Windowed = true.into();

    D3D11CreateDeviceAndSwapChain(
        ptr::null_mut(),
        D3D_DRIVER_TYPE_HARDWARE,
        ptr::null_mut(),
        0,
        ptr::null(),
        0,
        D3D11_SDK_VERSION,
        &desc,
        &mut swap,
        &mut dev,
        ptr::null_mut(),
        &mut ctx,
    ).ok().unwrap();

    // Grab Present from vtable index 8
    let vtbl = *(swap as *mut *mut *mut c_void);
    let present_addr = vtbl.add(8).read() as *const ();

    OutputDebugStringA("hookdll: Installing PresentHook\n\0".as_ptr() as _);
    PresentHook
        .initialize(std::mem::transmute(present_addr), hooked_present)
        .unwrap();
    PresentHook.enable().unwrap();

    // clean up dummy
    (*swap).Release();
    (*dev).Release();
    (*ctx).Release();

    OutputDebugStringA("hookdll: Hook installed!\n\0".as_ptr() as _);
    0
}

// ─────── Our hooked Present ───────
unsafe extern "system" fn hooked_present(
    swap: *mut IDXGISwapChain,
    sync: u32,
    flags: u32,
) -> HRESULT {
    INIT.call_once(|| {
        // one-time ImGui + Dx11 init
        OutputDebugStringA("hookdll: Initializing ImGui\n\0".as_ptr() as _);

        // get device & ctx
        let mut dev: *mut ID3D11Device = ptr::null_mut();
        let mut ctx: *mut ID3D11DeviceContext = ptr::null_mut();
        (*swap).GetDevice(&ID3D11Device::IID, &mut dev as _ as _).ok().unwrap();
        (*dev).GetImmediateContext(&mut ctx).ok().unwrap();

        // get window
        let mut sd = DXGI_SWAP_CHAIN_DESC::default();
        (*swap).GetDesc(&mut sd).ok().unwrap();
        let hwnd = sd.OutputWindow;

        // create ImGui context
        let mut imgui = Context::create();
        imgui.set_ini_filename(None);

        let mut platform = WinitPlatform::init(&mut imgui);
        platform.attach_window(imgui.io_mut(), hwnd, HiDpiMode::Default);

        let renderer = Renderer::new(&mut imgui, dev, ctx).unwrap();

        IMGUI = Box::into_raw(Box::new(imgui));
        RDR = Some(renderer);
        OutputDebugStringA("hookdll: ImGui + Renderer ready\n\0".as_ptr() as _);
    });

    // each frame
    let ui = &mut *IMGUI;
    let rdr = RDR.as_mut().unwrap();

    let io = ui.io_mut();
    io.display_size = {
        let mut ds = [0.0, 0.0];
        ui.io().display_size = ds;
        ds
    };
    ui.new_frame();

    // draw crosshair (always white for now)
    let draw = ui.get_background_draw_list();
    let [w, h] = ui.io().display_size;
    let cx = w * 0.5;
    let cy = h * 0.5;
    draw.add_line([cx - 10.0, cy], [cx + 10.0, cy], 0xFFFFFFFF, 2.0);
    draw.add_line([cx, cy - 10.0], [cx, cy + 10.0], 0xFFFFFFFF, 2.0);

    ui.render();
    rdr.render(ui.render());

    // call original Present
    PresentHook.call(swap, sync, flags)
}
