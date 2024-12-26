pub use system::{Features, System};

pub mod platform;
mod system;

pub type Pid = u32;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Feature {0:?} missing")]
    FeatureMissing(Features),
    #[cfg(target_os = "macos")]
    #[error(transparent)]
    Smc(#[from] smc::SMCError),
    #[error("Can't get physical core count")]
    PhysicalCoreCount,
    #[cfg(target_os = "windows")]
    #[error(transparent)]
    Wmi(#[from] wmi::WMIError),
    #[cfg(target_os = "windows")]
    #[error("Can't get com library")]
    ComLib,
    #[cfg(target_os = "windows")]
    #[error("PDH_STATUS: {0:#X}")]
    Pdh(u32),
    #[cfg(target_os = "windows")]
    #[error("Can't open process handle")]
    ProcessHandle,
    #[cfg(target_os = "windows")]
    #[error("WinError: {0}")]
    WinError(#[from] windows::core::Error),
    #[error("Access denied")]
    AccessDenied,
    #[cfg(target_os = "windows")]
    #[error("Etw error: {0:?}")]
    Etw(ferrisetw::trace::TraceError),
    #[cfg(all(target_os = "macos", feature = "dtrace"))]
    #[error(transparent)]
    Dtrace(#[from] dtrace::Error),
    #[error("Unsupported features: {0:?}")]
    UnsupportedFeatures(Features),
}

#[derive(Copy, Clone)]
pub enum GpuCalculation {
    Max,
    Sum,
}

impl Default for GpuCalculation {
    fn default() -> Self {
        Self::Max
    }
}
