use crate::opt::{ProcessCategory, SystemCategory};
use crate::types::ProcessInfo;
use crate::{CpuInfo, GpuInfo, PhysicalCpuInfo, Pid};
use serde::Serialize;
use std::fs::File;
use std::path::Path;

pub fn consume<P: AsRef<Path>>(
    path: P,
    proc_categories: &[ProcessCategory],
    sys_categories: &[SystemCategory],
    timestamps: &[chrono::DateTime<chrono::Local>],
    processes: &[ProcessInfo],
    cpu_info: &[CpuInfo],
    physical_cpu_info: &[PhysicalCpuInfo],
    gpu_info: &[GpuInfo],
) {
    let file = File::create(path).unwrap();

    let mut json_output = JsonOutput::default();

    // Process
    for (ci, &c) in proc_categories.into_iter().enumerate() {
        let target = match c {
            ProcessCategory::CPU => &mut json_output.cpu,
            ProcessCategory::Mem => &mut json_output.mem,
            ProcessCategory::GPU => &mut json_output.gpu,
            ProcessCategory::FPS => &mut json_output.fps,
            ProcessCategory::NetIn => &mut json_output.net_in,
            ProcessCategory::NetOut => &mut json_output.net_out,
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
            SystemCategory::CPUFreq => {
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
            SystemCategory::CPUTemp => {
                for info in physical_cpu_info {
                    json_output.sys_cpu_temp.push(SystemRecord {
                        records: timestamps
                            .iter()
                            .enumerate()
                            .map(|(i, t)| Record {
                                timestamp: t.to_rfc3339(),
                                value: info.temp[i],
                            })
                            .collect(),
                    });
                }
            }
            SystemCategory::GPU => {
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
        }
    }

    serde_json::to_writer(&file, &json_output).unwrap();
    file.sync_all().unwrap();
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
    net_in: Vec<ProcessRecord>,
    net_out: Vec<ProcessRecord>,
    sys_cpu_freq: Vec<SystemRecord>,
    sys_cpu_temp: Vec<SystemRecord>,
    sys_gpu: Vec<SystemRecord>,
}
