#[cfg(target_os = "windows")]
pub use self::windows::threads_info;
#[cfg(target_os = "linux")]
pub use linux::threads_info;
#[cfg(target_os = "macos")]
pub use macos::threads_info;

#[cfg(target_os = "windows")]
pub use self::windows::threads_count;
#[cfg(target_os = "linux")]
pub use linux::threads_count;
#[cfg(target_os = "macos")]
pub use macos::threads_count;

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "windows")]
pub mod windows;
