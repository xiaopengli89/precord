use crate::opt::{ProcessCategory, SystemCategory};
use crate::types::{ProcessInfo, SystemMetrics};
use std::path::Path;

pub fn consume<P: AsRef<Path>>(
    path: P,
    proc_categories: &[ProcessCategory],
    sys_categories: &[SystemCategory],
    timestamps: &[chrono::DateTime<chrono::Local>],
    processes: &[ProcessInfo],
    system_metrics: &[SystemMetrics],
) {
    let mut wtr = csv::WriterBuilder::new()
        .flexible(true)
        .from_path(path)
        .unwrap();

    // Process
    for (ci, &c) in proc_categories.into_iter().enumerate() {
        // Title
        wtr.write_field(format!("Process {:?}", c)).unwrap();
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
    for (i, &c) in sys_categories.into_iter().enumerate() {
        let metrics = &system_metrics[i];

        // Title
        wtr.write_field(format!("System {:?}", c)).unwrap();
        for i in 0..metrics.rows.len() {
            wtr.write_field(format!("{:?}{}", c, i)).unwrap();
        }
        wtr.write_record(None::<&[u8]>).unwrap();

        // Data
        for (i, t) in timestamps.into_iter().enumerate() {
            // Timestamp
            wtr.write_field(t.to_string()).unwrap();
            // Process data
            for row in metrics.rows.iter() {
                wtr.write_field(format!("{:.2}", row[i])).unwrap();
            }
            wtr.write_record(None::<&[u8]>).unwrap();
        }

        wtr.write_record([" "]).unwrap();
    }
}
