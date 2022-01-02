use crate::opt::{ProcessCategory, SystemCategory};
use crate::types::ProcessInfo;
use crate::{CpuInfo, GpuInfo, PhysicalCpuInfo};
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
    let mut wtr = csv::WriterBuilder::new()
        .flexible(true)
        .from_path(path)
        .unwrap();

    // Process
    for (ci, &c) in proc_categories.into_iter().enumerate() {
        // Title
        wtr.write_field(match c {
            ProcessCategory::CPU => "Process CPU Usage",
            ProcessCategory::Mem => "Process Memory Usage",
            ProcessCategory::GPU => "Process GPU Usage",
            ProcessCategory::FPS => "Process FPS",
        })
        .unwrap();
        for p in processes {
            wtr.write_field(format!("{}({})", &p.name, p.pid)).unwrap();
        }
        wtr.write_record(None::<&[u8]>).unwrap();

        // Data
        for (i, t) in timestamps.into_iter().enumerate() {
            // Timestamp
            wtr.write_field(t.to_rfc3339()).unwrap();
            // Process data
            for p in processes {
                wtr.write_field(format!("{:.2}", p.values[ci][i])).unwrap();
            }
            wtr.write_record(None::<&[u8]>).unwrap();
        }

        wtr.write_record([" "]).unwrap();
    }

    // System
    for &c in sys_categories {
        match c {
            SystemCategory::CPUFreq => {
                // Title
                wtr.write_field("CPUs Frequency").unwrap();
                for i in 0..cpu_info.len() {
                    wtr.write_field(format!("CPU{}", i)).unwrap();
                }
                wtr.write_record(None::<&[u8]>).unwrap();

                // Data
                for (i, t) in timestamps.into_iter().enumerate() {
                    // Timestamp
                    wtr.write_field(t.to_string()).unwrap();
                    // Process data
                    for c in cpu_info {
                        wtr.write_field(format!("{:.2}", c.freq[i])).unwrap();
                    }
                    wtr.write_record(None::<&[u8]>).unwrap();
                }
            }
            SystemCategory::CPUTemp => {
                // Title
                wtr.write_field("CPUs Temperature").unwrap();
                for i in 0..physical_cpu_info.len() {
                    wtr.write_field(format!("CPU{}", i)).unwrap();
                }
                wtr.write_record(None::<&[u8]>).unwrap();

                // Data
                for (i, t) in timestamps.into_iter().enumerate() {
                    // Timestamp
                    wtr.write_field(t.to_string()).unwrap();
                    // Process data
                    for c in physical_cpu_info {
                        wtr.write_field(format!("{:.2}", c.temp[i])).unwrap();
                    }
                    wtr.write_record(None::<&[u8]>).unwrap();
                }
            }
            SystemCategory::GPU => {
                // Title
                wtr.write_field("System GPU Usage").unwrap();
                for _ in 0..gpu_info.len() {
                    wtr.write_field(format!("GPU")).unwrap();
                }
                wtr.write_record(None::<&[u8]>).unwrap();

                // Data
                for (i, t) in timestamps.into_iter().enumerate() {
                    // Timestamp
                    wtr.write_field(t.to_string()).unwrap();
                    // Process data
                    for c in gpu_info {
                        wtr.write_field(format!("{:.2}", c.usage[i])).unwrap();
                    }
                    wtr.write_record(None::<&[u8]>).unwrap();
                }
            }
        }
        wtr.write_record([" "]).unwrap();
    }
}
