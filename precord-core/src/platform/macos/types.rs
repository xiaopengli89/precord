use mach2::{kern_return, mach_port, port, traps};
use std::ffi::CStr;

pub struct MachPort {
    raw: port::mach_port_t,
}

impl MachPort {
    pub unsafe fn from_raw(raw: port::mach_port_t) -> Self {
        Self { raw }
    }

    pub fn as_raw(&self) -> port::mach_port_t {
        self.raw
    }
}

impl Drop for MachPort {
    fn drop(&mut self) {
        unsafe {
            let r = mach_port::mach_port_deallocate(traps::mach_task_self(), self.raw);
            assert_eq!(r, kern_return::KERN_SUCCESS);
        }
    }
}

pub struct ThreadInfo {
    pub id: String,
    pub cpu_usage: f32,
}

impl ThreadInfo {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn cpu_usage(&self) -> f32 {
        self.cpu_usage
    }
}

impl From<proc_threadinfo> for ThreadInfo {
    fn from(info: proc_threadinfo) -> Self {
        let name = unsafe { CStr::from_ptr(info.pth_name.as_ptr() as _) }
            .to_str()
            .unwrap_or("<Unnamed>");
        Self {
            id: if name.is_empty() { "<Unnamed>" } else { name }.to_string(),
            cpu_usage: info.pth_cpu_usage as f32 / 10.,
        }
    }
}

impl From<ThreadInfoPrivilege> for ThreadInfo {
    fn from(info: ThreadInfoPrivilege) -> Self {
        Self {
            id: info.id_info.thread_id.to_string(),
            cpu_usage: info.basic_info.cpu_usage as f32 / 10.,
        }
    }
}

#[allow(non_camel_case_types)]
#[repr(C)]
pub struct proc_threadinfo {
    pub pth_user_time: u64,   /* user run time */
    pub pth_system_time: u64, /* system run time */
    pub pth_cpu_usage: i32,   /* scaled cpu usage percentage */
    pub pth_policy: i32,      /* scheduling policy in effect */
    pub pth_run_state: i32,   /* run state (see below) */
    pub pth_flags: i32,       /* various flags (see below) */
    pub pth_sleep_time: i32,  /* number of seconds that thread */
    pub pth_curpri: i32,      /* cur priority*/
    pub pth_priority: i32,    /*  priority*/
    pub pth_maxpriority: i32, /* max priority*/
    pub pth_name: [u8; 64],   /* thread name, if any */
}

pub struct ThreadInfoPrivilege {
    pub id_info: libc::thread_identifier_info,
    pub basic_info: libc::thread_basic_info,
    pub port: MachPort,
}

#[allow(non_camel_case_types)]
#[repr(C)]
pub struct gpu_energy_data {
    pub task_gpu_utilisation: u64,
    pub _data: [u64; 3],
}

#[allow(non_camel_case_types)]
#[repr(C)]
pub struct task_power_info_v2 {
    pub cpu_energy: [u64; 6],
    pub gpu_energy: gpu_energy_data,
    #[cfg(target_arch = "aarch64")]
    pub task_energy: u64,
    pub task_ptime: u64,
    pub task_pset_switches: u64,
}

// https://github.com/llvm/llvm-project/blob/main/lldb/tools/debugserver/source/MacOSX/MachTask.mm
// https://source.chromium.org/chromium/chromium/src/+/main:base/process/process_metrics_mac.cc
#[allow(non_camel_case_types, dead_code)]
#[repr(C)]
pub struct pm_task_energy_data_t {
    pub data: [u8; 408],
}
