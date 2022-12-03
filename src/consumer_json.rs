use crate::opt::{ProcessCategory, SystemCategory};
use crate::types::{ProcessInfo, SystemMetrics};
use crate::Pid;
use serde::Serialize;
use serde_with::with_prefix;
use std::collections::HashMap;
use std::fs::File;
use std::path::Path;

pub fn consume<P: AsRef<Path>>(
    path: P,
    proc_categories: &[ProcessCategory],
    sys_categories: &[SystemCategory],
    timestamps: &[chrono::DateTime<chrono::Local>],
    processes: &[ProcessInfo],
    system_metrics: &[SystemMetrics],
) {
    let file = File::create(path).unwrap();

    let mut json_output = JsonOutput::default();

    // Process
    for (ci, &c) in proc_categories.into_iter().enumerate() {
        let mut target = vec![];

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

        json_output.process_records.insert(c, target);
    }

    // System
    for (i, &c) in sys_categories.into_iter().enumerate() {
        let metrics = &system_metrics[i];
        let target: Vec<_> = metrics
            .rows
            .iter()
            .map(|row| SystemRecord {
                records: timestamps
                    .iter()
                    .enumerate()
                    .map(|(i, t)| Record {
                        timestamp: t.to_rfc3339(),
                        value: row[i],
                    })
                    .collect(),
            })
            .collect();

        json_output.sys_records.insert(c, target);
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

with_prefix!(prefix_sys "sys_");

#[derive(Default, Serialize)]
struct JsonOutput {
    #[serde(flatten)]
    process_records: HashMap<ProcessCategory, Vec<ProcessRecord>>,
    #[serde(flatten, with = "prefix_sys")]
    sys_records: HashMap<SystemCategory, Vec<SystemRecord>>,
}
