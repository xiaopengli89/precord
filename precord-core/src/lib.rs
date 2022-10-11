pub use system::{Features, System};
use windows::Win32::Foundation;

mod platform;
mod system;

pub type Pid = u32;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Feature {0} missing")]
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
    #[error("PDH_STATUS({})", 0.0)]
    Pdh(Foundation::WIN32_ERROR),
    #[cfg(target_os = "windows")]
    #[error("Can't open process handle")]
    ProcessHandle,
    #[error("Access denied")]
    AccessDenied,
    #[error("Etw error")]
    Etw,
    #[error("Unsupported features: {0}")]
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
