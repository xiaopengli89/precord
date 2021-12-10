use crate::Pid;
use precord_core::System;

pub struct ProcessInfo {
    pub pid: Pid,
    pub name: String,
    pub command: String,
    pub value_percents: Vec<Vec<f32>>,
    pub valid: bool,
}

impl ProcessInfo {
    pub fn new(system: &System, categories: &[String], pid: Pid) -> Self {
        let name = system
            .process_name(pid)
            .expect(&format!("No such process({})", pid))
            .to_string();
        let command = system
            .process_command(pid)
            .expect(&format!("No such process({})", pid))
            .join(" ");

        Self {
            pid,
            name,
            command,
            value_percents: vec![vec![]; categories.len()],
            valid: true,
        }
    }

    pub fn avg_percent(&self, idx: usize) -> f32 {
        if self.value_percents[idx].is_empty() {
            0.0
        } else {
            self.value_percents[idx].iter().sum::<f32>() / (self.value_percents[idx].len() as f32)
        }
    }
}

pub struct CpuInfo {
    pub freq: Vec<f32>,
}

impl CpuInfo {
    pub fn avg(&self) -> f32 {
        if self.freq.is_empty() {
            0.0
        } else {
            self.freq.iter().sum::<f32>() / (self.freq.len() as f32)
        }
    }
}

pub struct GpuInfo {
    pub utilization: Vec<f32>,
}

impl GpuInfo {
    pub fn utilization_avg(&self) -> f32 {
        if self.utilization.is_empty() {
            0.0
        } else {
            self.utilization.iter().sum::<f32>() / (self.utilization.len() as f32)
        }
    }
}
