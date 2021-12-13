use crate::types::ProcessInfo;
use crate::{Category, CpuInfo, GpuInfo, Pid};
use serde::Serialize;
use std::fs::File;
use std::path::Path;

pub fn consume<P: AsRef<Path>>(
    path: P,
    categories: &[Category],
    sys_categories: &[Category],
    timestamps: &[chrono::DateTime<chrono::Local>],
    processes: &[ProcessInfo],
    cpu_info: &[CpuInfo],
    gpu_info: &[GpuInfo],
) {
    let file = File::create(path).unwrap();

    let mut json_output = JsonOutput::default();

    // Process
    for (ci, &c) in categories.into_iter().enumerate() {
        let target = match c {
            Category::CPU => &mut json_output.cpu,
            Category::Mem => &mut json_output.mem,
            Category::GPU => &mut json_output.gpu,
            Category::FPS => &mut json_output.fps,
            _ => unimplemented!(),
        };
        for p in processes {
            target.push(ProcessRecord {
                pid: p.pid,
                name: p.name.clone(),
                command: p.command.clone(),
                records: timestamps
                    .iter()
                    .enumerate()
                    .map(|(i, t)| Record {
                        timestamp: t.to_rfc3339(),
                        value: p.values[ci][i],
                    })
                    .collect(),
            });
        }
    }

    // System
    for &c in sys_categories {
        match c {
            Category::SysCPUFreq => {
                for info in cpu_info {
                    json_output.sys_cpu_freq.push(SystemRecord {
                        records: timestamps
                            .iter()
                            .enumerate()
                            .map(|(i, t)| Record {
                                timestamp: t.to_rfc3339(),
                                value: info.freq[i],
                            })
                            .collect(),
                    });
                }
            }
            Category::SysGPU => {
                for info in gpu_info {
                    json_output.sys_gpu.push(SystemRecord {
                        records: timestamps
                            .iter()
                            .enumerate()
                            .map(|(i, t)| Record {
                                timestamp: t.to_rfc3339(),
                                value: info.usage[i],
                            })
                            .collect(),
                    });
                }
            }
            _ => unimplemented!(),
        }
    }

    serde_json::to_writer_pretty(file, &json_output).unwrap();
}

#[derive(Serialize)]
struct Record {
    timestamp: String,
    value: f32,
}

#[derive(Serialize)]
struct ProcessRecord {
    pid: Pid,
    name: String,
    command: String,
    records: Vec<Record>,
}

#[derive(Serialize)]
struct SystemRecord {
    records: Vec<Record>,
}

#[derive(Default, Serialize)]
struct JsonOutput {
    cpu: Vec<ProcessRecord>,
    mem: Vec<ProcessRecord>,
    gpu: Vec<ProcessRecord>,
    fps: Vec<ProcessRecord>,
    sys_cpu_freq: Vec<SystemRecord>,
    sys_gpu: Vec<SystemRecord>,
}

/*

{
    "cpu": [
        {
            "pid": 1,
            "name": "",
            "command": "",
            "records": [
                {
                    "timestamp": "<timestamp>",
                    "value": 1.0,
                }
            ]
        }
    ],
    "sys_cpu_freq": [
        {
            "records": [
                {
                    "timestamp": "<timestamp>",
                    "value": 1.0,
                }
            ]
        }
    ]
}

*/
