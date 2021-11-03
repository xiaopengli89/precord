pub use system::{System, Features};

mod platform;
mod system;

#[cfg(unix)]
pub type Pid = libc::pid_t;

#[cfg(target_os = "windows")]
pub type Pid = winapi::shared::minwindef::DWORD;
