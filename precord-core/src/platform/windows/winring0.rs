use std::ffi::c_uint;
use std::mem;

const IA32_TEMPERATURE_TARGET: c_uint = 0x1a2;
const IA32_PACKAGE_THERM_STATUS: c_uint = 0x1b1;

pub struct WinRing0 {
    rdmsr: libloading::Symbol<'static, Rdmsr>,
    lib: libloading::Library,
}

impl WinRing0 {
    pub fn new() -> Result<Self, libloading::Error> {
        #[cfg(target_arch = "x86_64")]
        let dll_name = "WinRing0x64.dll";
        #[cfg(target_arch = "x86")]
        let dll_name = "WinRing0.dll";

        unsafe {
            let lib = libloading::Library::new(dll_name)?;
            let rdmsr: libloading::Symbol<Rdmsr> = lib.get(b"Rdmsr")?;
            Ok(Self {
                rdmsr: mem::transmute(rdmsr),
                lib,
            })
        }
    }
}

type Rdmsr =
    extern "system" fn(c_uint, *mut c_uint, *mut c_uint) -> windows::Win32::Foundation::BOOL;
