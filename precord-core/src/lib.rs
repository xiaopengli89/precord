pub use system::{System, Features};

mod platform;
mod system;

pub type Pid = libc::pid_t;
