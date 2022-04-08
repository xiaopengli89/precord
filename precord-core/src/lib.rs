pub use system::{Features, System};

mod platform;
mod system;

pub type Pid = sysinfo::Pid;

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
    #[error("PDH_STATUS({0})")]
    Pdh(winapi::um::pdh::PDH_STATUS),
    #[cfg(target_os = "windows")]
    #[error("Can't open process handle")]
    ProcessHandle,
    #[error("Access denied")]
    AccessDenied,
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
