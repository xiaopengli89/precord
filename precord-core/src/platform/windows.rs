use crate::Pid;
use serde::Deserialize;
use std::io::BufReader;
use std::io::{BufRead, Write};
use std::process;

pub struct Powershell {
    process: process::Child,
    stdout: BufReader<process::ChildStdout>,
}

impl Powershell {
    pub fn new() -> Self {
        let mut p = process::Command::new("powershell")
            .args(&["-Command", "-"])
            .stdin(process::Stdio::piped())
            .stdout(process::Stdio::piped())
            .spawn()
            .unwrap();
        let o = BufReader::new(p.stdout.take().unwrap());
        Self {
            process: p,
            stdout: o,
        }
    }

    pub fn poll_gpu_percent(&mut self, pid: Option<Pid>) -> Option<f32> {
        let pid = if let Some(pid) = pid {
            format!("pid_{}", pid)
        } else {
            "".to_string()
        };
        let mut gpu_percent = 0.0;
        let mut r = String::new();

        let stdin = self.process.stdin.as_mut().unwrap();
        let stdout = &mut self.stdout;

        for engine in ["3D", "VideoEncode", "VideoDecode", "VideoProcessing"] {
            stdin
                .write_all(
                    format!(include_str!("../../../asset/powershell.txt"), pid, engine).as_bytes(),
                )
                .unwrap();

            loop {
                r.clear();
                stdout.read_line(&mut r).ok()?;
                match r.trim() {
                    "" => continue,
                    "EOF" => break,
                    _ => {}
                }
                gpu_percent += r.trim().parse::<f32>().ok()?;
            }
        }

        Some(gpu_percent)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ProcessorInfo {
    pub percent_processor_performance: f32,
    pub processor_frequency: f32,
}
