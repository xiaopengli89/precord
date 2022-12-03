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
    pub fn new(system: &System, proc_category_len: usize, pid: Pid) -> Option<Self> {
        let name = system.process_name(pid)?.to_string();
        let command = system.process_command(pid)?.join(" ");

        Some(Self {
            pid,
            name,
            command,
            values: vec![vec![]; proc_category_len],
            valid: true,
        })
    }

    pub fn avg_value(&self, idx: usize) -> f32 {
        if self.values[idx].is_empty() {
            0.0
        } else {
            self.values[idx].iter().sum::<f32>() / (self.values[idx].len() as f32)
        }
    }
}

#[derive(Default, Clone)]
pub struct SystemMetrics {
    pub rows: Vec<Vec<f32>>,
}

impl SystemMetrics {
    pub fn row_avg(&self, index: usize) -> Option<f32> {
        let row = self.rows.get(index)?;
        if row.is_empty() {
            None
        } else {
            Some(row.iter().sum::<f32>() / (row.len() as f32))
        }
    }

    pub fn max(&self) -> Option<f32> {
        self.rows
            .iter()
            .flat_map(|v| v.iter().copied())
            .max_by(f32::total_cmp)
    }
}
