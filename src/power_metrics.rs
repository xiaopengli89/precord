use heim::process::Pid;
use serde::Deserialize;
use std::process::Command;

#[derive(Debug, Deserialize)]
pub struct PowerMetricsResult {
    tasks: Vec<Task>,
}

impl PowerMetricsResult {
    pub fn gpu_percent(&self, pid: Pid) -> Option<f32> {
        self.tasks
            .iter()
            .find(|task| task.pid == pid)
            .map(|task| task.gputime_ms_per_s / 10.0)
    }
}

#[derive(Debug, Deserialize)]
pub struct Task {
    pid: Pid,
    #[serde(default)]
    gputime_ms_per_s: f32,
}

pub struct PowerMetrics {}

impl PowerMetrics {
    pub fn new() -> Self {
        Self {}
    }

    pub fn poll(&self) -> PowerMetricsResult {
        let o = Command::new("powermetrics")
            .args([
                "--samplers",
                "tasks",
                "--show-process-gpu",
                "-n1",
                "-i1000",
                "-f",
                "plist",
            ])
            .output()
            .unwrap();

        assert_eq!(o.status.code(), Some(0));

        let r: PowerMetricsResult = plist::from_bytes(o.stdout.as_slice()).unwrap();
        r
    }
}
