use heim::process::Pid;
use serde::Deserialize;
use std::process::Command;

#[derive(Debug, Default, Deserialize)]
struct PowerMetricsResult {
    tasks: Vec<Task>,
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

        self.last_result = plist::from_bytes(o.stdout.as_slice()).unwrap();
    }

    pub fn gpu_percent(&self, pid: Pid) -> Option<f32> {
        self.last_result
            .tasks
            .iter()
            .find(|task| task.pid == pid)
            .map(|task| task.gputime_ms_per_s / 10.0)
    }
}
