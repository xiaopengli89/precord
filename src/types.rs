use crate::Pid;
use precord_core::System;

pub struct ProcessInfo {
    pub pid: Pid,
    pub name: String,
    pub command: String,
    pub values: Vec<Vec<f32>>,
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
            values: vec![vec![]; categories.len()],
            valid: true,
        }
    }

    pub fn avg_value(&self, idx: usize) -> f32 {
        if self.values[idx].is_empty() {
            0.0
        } else {
            self.values[idx].iter().sum::<f32>() / (self.values[idx].len() as f32)
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
    pub usage: Vec<f32>,
}

impl GpuInfo {
    pub fn usage_avg(&self) -> f32 {
        if self.usage.is_empty() {
            0.0
        } else {
            self.usage.iter().sum::<f32>() / (self.usage.len() as f32)
        }
    }
}
