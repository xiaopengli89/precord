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
}
