use windows::{
    Win32::Graphics::{Direct3D::*, Direct3D12::*, Dxgi::*},
    core::h,
};

pub struct GPULib {
    pub queue: ID3D12CommandQueue,
    pub device: ID3D12Device9,
    pub factory: IDXGIFactory7,
}

impl GPULib {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        #[cfg(debug_assertions)]
        {
            let mut debug_option: Option<ID3D12Debug6> = None;
            unsafe { D3D12GetDebugInterface(&mut debug_option) }?;
            if let Some(debug) = debug_option.take() {
                unsafe {
                    debug.EnableDebugLayer();
                    debug.SetEnableGPUBasedValidation(true);
                }
            } else {
                println!("Debug is active, but the debug layer could not be loaded");
            }
        }

        let dxgi_factory_flags = if cfg!(debug_assertions) {
            DXGI_CREATE_FACTORY_DEBUG
        } else {
            DXGI_CREATE_FACTORY_FLAGS(0)
        };

        let factory = unsafe { CreateDXGIFactory2::<IDXGIFactory7>(dxgi_factory_flags) }?;

        let device = Self::create_device(&factory, DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE)?;

        // Register debug callback, so messages will be printed to stderr
        // Callback will never be unregistered, so all the related variables can be dropped
        #[cfg(debug_assertions)]
        unsafe {
            use windows::core::Interface;
            // Get InfoQueue1 interface from device
            let mut info_queue = std::mem::MaybeUninit::<ID3D12InfoQueue1>::uninit();
            if device
                .query(&ID3D12InfoQueue1::IID, info_queue.as_mut_ptr() as _)
                .is_err()
            {
                return Err("Failed to query info queue".into());
            }

            let mut callback_cookie = std::mem::MaybeUninit::uninit();
            info_queue.assume_init().RegisterMessageCallback(
                Some(debug_message_callback),
                D3D12_MESSAGE_CALLBACK_FLAG_NONE,
                std::ptr::null_mut(),
                callback_cookie.as_mut_ptr(),
            )?;

            // Secondary error check according to Microsoft docs
            if (callback_cookie.assume_init()) == 0 {
                return Err("Failed to register D3D12 debug layer message callback".into());
            }
        }

        let queue: ID3D12CommandQueue = unsafe {
            let desc = D3D12_COMMAND_QUEUE_DESC {
                Type: D3D12_COMMAND_LIST_TYPE_DIRECT,
                Priority: D3D12_COMMAND_QUEUE_PRIORITY_HIGH.0,
                Flags: D3D12_COMMAND_QUEUE_FLAG_NONE,
                NodeMask: 0,
            };
            device.CreateCommandQueue(&desc)
        }?;

        unsafe {
            device.SetName(h!("Gimslib main device")).unwrap();
            queue.SetName(h!("Gimslib main queue")).unwrap();
        }

        Ok(GPULib {
            factory,
            device,
            queue,
        })
    }

    fn create_device(
        factory: &IDXGIFactory7,
        preference: DXGI_GPU_PREFERENCE,
    ) -> Result<ID3D12Device9, Box<dyn std::error::Error>> {
        for i in 0.. {
            let adapter: IDXGIAdapter1 =
                unsafe { factory.EnumAdapterByGpuPreference(i, preference) }?;
            let desc = unsafe { adapter.GetDesc1()? };

            if (DXGI_ADAPTER_FLAG(desc.Flags as _) & DXGI_ADAPTER_FLAG_SOFTWARE)
                != DXGI_ADAPTER_FLAG_NONE
            {
                // Don't select software renderers
                continue;
            }

            let mut device_option: Option<ID3D12Device9> = None;
            unsafe { D3D12CreateDevice(&adapter, D3D_FEATURE_LEVEL_11_0, &mut device_option) }?;

            let device = device_option.ok_or("Failed to create device")?;

            return Ok(device);
        }
        unreachable!();
    }
}

#[cfg(debug_assertions)]
unsafe extern "system" fn debug_message_callback(
    _category_code: D3D12_MESSAGE_CATEGORY,
    severity_code: D3D12_MESSAGE_SEVERITY,
    _id: D3D12_MESSAGE_ID,
    description: windows::core::PCSTR,
    _context: *mut std::ffi::c_void,
) {
    if severity_code.0 > D3D12_MESSAGE_SEVERITY_WARNING.0 {
        // Any message less severe than a warning should be ignored
        return;
    }

    let severity = match severity_code {
        D3D12_MESSAGE_SEVERITY_CORRUPTION => "Corruption",
        D3D12_MESSAGE_SEVERITY_ERROR => "Error",
        D3D12_MESSAGE_SEVERITY_INFO => "Info",
        D3D12_MESSAGE_SEVERITY_MESSAGE => "Message",
        D3D12_MESSAGE_SEVERITY_WARNING => "Warning",
        _ => "Unknown",
    };

    if let Ok(description_string) = unsafe { description.to_string() } {
        eprintln!("D3D12 Debug {}: {}", severity, description_string);
    } else {
        eprintln!(
            "Failed to decode D3D12 debug layer message with severity {}",
            severity
        );
    }
}
