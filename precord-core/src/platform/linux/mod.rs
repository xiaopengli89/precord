use crate::{Error, Pid};

pub struct ThreadInfo;

impl ThreadInfo {
    pub fn id(&self) -> u64 {
        unimplemented!()
    }

    pub fn cpu_usage(&self) -> f32 {
        unimplemented!()
    }
}

pub fn threads_info(_pid: Pid, _nb_cpus: u32) -> Result<Vec<ThreadInfo>, Error> {
    Ok(vec![])
}
