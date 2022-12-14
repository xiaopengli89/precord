use mach2::{kern_return, mach_port, port, traps};

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
    pub id_info: libc::thread_identifier_info,
    pub basic_info: libc::thread_basic_info,
    pub port: MachPort,
}

impl ThreadInfo {
    pub fn id(&self) -> u64 {
        self.id_info.thread_id
    }

    pub fn cpu_usage(&self) -> f32 {
        self.basic_info.cpu_usage as f32 / 10. // TODO: TH_USAGE_SCALE
    }
}
