pub use system::{Features, System};

mod platform;
mod system;

pub type Pid = sysinfo::Pid;
