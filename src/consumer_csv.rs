use crate::{CpuInfo, GpuInfo, ProcessInfo};
use std::path::Path;

pub fn consume<P: AsRef<Path>>(
    path: P,
    categories: &[String],
    sys_categories: &[String],
    timestamps: &[chrono::DateTime<chrono::Local>],
    processes: &[ProcessInfo],
    cpu_info: &[CpuInfo],
    gpu_info: &[GpuInfo],
) {
    let mut wtr = csv::WriterBuilder::new()
        .flexible(true)
        .from_path(path)
        .unwrap();

    // Process
    for (ci, c) in categories.into_iter().enumerate() {
        // Title
        wtr.write_field(match c.as_str() {
            "cpu" => "Process CPU Usage",
            "mem" => "Process MEM Usage",
            "gpu" => "Process GPU Usage",
            "fps" => "Process FPS",
            _ => unimplemented!(),
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
                wtr.write_field(format!("{:.2}", p.value_percents[ci][i]))
                    .unwrap();
            }
            wtr.write_record(None::<&[u8]>).unwrap();
        }

        wtr.write_record([" "]).unwrap();
    }

    // System
    for c in sys_categories {
        match c.as_str() {
            "sys_cpu_freq" => {
                // Title
                wtr.write_field("CPU Frequency").unwrap();
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
            "sys_gpu" => {
                // Title
                wtr.write_field("System GPU Utilization").unwrap();
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
                        wtr.write_field(format!("{:.2}", c.utilization[i])).unwrap();
                    }
                    wtr.write_record(None::<&[u8]>).unwrap();
                }
            }
            _ => unimplemented!(),
        }
        wtr.write_record([" "]).unwrap();
    }
}
