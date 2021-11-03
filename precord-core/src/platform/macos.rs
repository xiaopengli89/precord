use crate::Pid;
use serde::Deserialize;
use std::process::Command;

#[derive(Debug, Default, Deserialize)]
struct PowerMetricsResult {
    tasks: Vec<Task>,
    processor: ProcessorInfo,
}

#[derive(Debug, Deserialize)]
pub struct Task {
    pid: Pid,
    #[serde(default)]
    gputime_ms_per_s: f32,
}

pub struct PowerMetrics {
    last_result: PowerMetricsResult,
}

impl PowerMetrics {
    pub fn new() -> Self {
        Self {
            last_result: Default::default(),
        }
    }

    pub fn poll(&mut self) {
        let o = Command::new("powermetrics")
            .args([
                "--samplers",
                "tasks,cpu_power",
                "--show-process-gpu",
                "-n1",
                "-i1000",
                "-f",
                "plist",
            ])
            .output()
            .unwrap();

        match o.status.code() {
            Some(0) => {}
            _ => {
                panic!("Error: {}", String::from_utf8_lossy(&o.stderr));
            }
        }

        self.last_result = plist::from_bytes(o.stdout.as_slice()).unwrap();
    }

    pub fn gpu_percent(&self, pid: Pid) -> Option<f32> {
        self.last_result
            .tasks
            .iter()
            .find(|task| task.pid == pid)
            .map(|task| task.gputime_ms_per_s / 10.0)
    }

    pub fn cpu_frequency(&self) -> Vec<f32> {
        if !self.last_result.processor.clusters.is_empty() {
            // Apple Silicon
            self.last_result
                .processor
                .clusters
                .iter()
                .map(|c| c.cpus.iter())
                .flatten()
                .map(|c| c.freq_hz / 1_000_000.0)
                .collect()
        } else {
            // Intel
            self.last_result
                .processor
                .packages
                .iter()
                .map(|p| p.cores.iter())
                .flatten()
                .map(|c| c.cpus.iter())
                .flatten()
                .map(|c| c.freq_hz / 1_000_000.0)
                .collect()
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct ProcessorInfo {
    #[serde(default)]
    clusters: Vec<Cluster>,
    #[serde(default)]
    packages: Vec<Package>,
}

#[derive(Debug, Deserialize)]
struct Cpu {
    freq_hz: f32,
}

// Apple Silicon
#[derive(Debug, Deserialize)]
struct Cluster {
    cpus: Vec<Cpu>,
}

// Intel
#[derive(Debug, Deserialize)]
struct Package {
    cores: Vec<Core>,
}

#[derive(Debug, Deserialize)]
struct Core {
    cpus: Vec<Cpu>,
}
