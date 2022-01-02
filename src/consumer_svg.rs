use crate::opt::{ProcessCategory, SystemCategory};
use crate::types::ProcessInfo;
use crate::{CpuInfo, GpuInfo, PhysicalCpuInfo};
use plotters::prelude::*;
use std::path::Path;

pub fn consume<P: AsRef<Path>>(
    output: P,
    proc_category: &[ProcessCategory],
    sys_category: &[SystemCategory],
    timestamps: &[chrono::DateTime<chrono::Local>],
    processes: &[ProcessInfo],
    cpu_info: &[CpuInfo],
    cpu_frequency_max: f32,
    physical_cpu_info: &[PhysicalCpuInfo],
    cpu_temperature_max: f32,
    gpu_info: &[GpuInfo],
) {
    if timestamps.is_empty() {
        return;
    }

    let timestamp_range = || timestamps[0].clone()..timestamps.last().cloned().unwrap();

    let top_height = if !processes.is_empty() {
        (processes.len() + 2) * 15
    } else {
        0
    };

    let root = SVGBackend::new(
        &output,
        (
            1280,
            (top_height + 720 * (proc_category.len() + sys_category.len())) as u32,
        ),
    )
    .into_drawing_area();
    root.fill(&WHITE).unwrap();

    let (top, bottom) = root.split_vertically(top_height as u32);
    let default_font = ("sans-serif", 12).into_font();
    let default_style: TextStyle = default_font.into();

    for (i, p) in processes.into_iter().enumerate() {
        let color = Palette99::pick(i).stroke_width(2).filled();
        let i = i as i32;
        let legend = PathElement::new(vec![(60, 23 + i * 15), (80, 23 + i * 15)], color);
        top.draw(&legend).unwrap();
        let label = format!("{}({}) - {}", p.name, p.pid, p.command);
        top.draw_text(&label, &default_style, (90, 19 + i * 15))
            .unwrap();
    }

    let areas = bottom.split_evenly((proc_category.len() + sys_category.len(), 1));

    // Draw process
    for idx_c in 0..proc_category.len() {
        let area = &areas[idx_c];
        let mut total = vec![];

        for process in processes.iter() {
            if total.len() < process.values[idx_c].len() {
                total.extend_from_slice(
                    vec![0.0; process.values[idx_c].len() - total.len()].as_slice(),
                );
            }
            for (a, b) in total.iter_mut().zip(process.values[idx_c].iter()) {
                *a += *b;
            }
        }
        let mut max = 0.0f32;
        for &t in total.iter() {
            max = max.max(t);
        }

        let caption;
        let unit;

        match proc_category[idx_c] {
            ProcessCategory::CPU => {
                caption = "Process CPU Usage";
                unit = "%";
                max = max.max(100.0);
            }
            ProcessCategory::Mem => {
                caption = "Process Memory Usage";
                unit = "M";
                max += 100.0;
            }
            ProcessCategory::GPU => {
                caption = "Process GPU Usage";
                unit = "%";
                max = max.max(100.0);
            }
            ProcessCategory::FPS => {
                caption = "Process FPS";
                unit = "";
                max = max.max(60.0);
            }
        };

        let mut chart = ChartBuilder::on(&area)
            .caption(caption, ("sans-serif", 30).into_font())
            .margin(10)
            .x_label_area_size(40)
            .y_label_area_size(50)
            .build_cartesian_2d(timestamp_range(), 0f32..max)
            .unwrap();

        chart
            .configure_mesh()
            .y_label_formatter(&|y| format!("{}{}", y, unit))
            .draw()
            .unwrap();

        for (idx, process) in processes.iter().enumerate() {
            let color = Palette99::pick(idx).stroke_width(2).filled();
            chart
                .draw_series(LineSeries::new(
                    timestamps
                        .into_iter()
                        .cloned()
                        .zip(process.values[idx_c].iter().cloned()),
                    color.clone(),
                ))
                .unwrap()
                .label(format!(
                    "{}({}) / AVG({:.2}{})",
                    &process.name,
                    process.pid,
                    process.avg_value(idx_c),
                    unit
                ))
                .legend(move |(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], color.clone()));
        }

        if processes.len() > 1 {
            // Total
            let avg: f32 = total.iter().map(|a| *a).sum::<f32>() / total.len() as f32;
            let color = Palette99::pick(processes.len()).stroke_width(2).filled();
            chart
                .draw_series(LineSeries::new(
                    timestamps.into_iter().cloned().zip(total.into_iter()),
                    color.clone(),
                ))
                .unwrap()
                .label(format!("Total / AVG({:.2}{})", avg, unit))
                .legend(move |(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], color.clone()));
        }

        chart
            .configure_series_labels()
            .background_style(&WHITE.mix(0.8))
            .border_style(&BLACK)
            .draw()
            .unwrap();
    }

    // Draw system
    let mut area_i = proc_category.len();

    for &c in sys_category {
        let area = &areas[area_i];
        let mut chart;

        match c {
            SystemCategory::CPUFreq => {
                chart = ChartBuilder::on(&area)
                    .caption("CPUs Frequency", ("sans-serif", 30).into_font())
                    .margin(10)
                    .x_label_area_size(40)
                    .y_label_area_size(50)
                    .build_cartesian_2d(timestamp_range(), 0f32..cpu_frequency_max)
                    .unwrap();

                chart
                    .configure_mesh()
                    .y_label_formatter(&|y| format!("{}MHz", y))
                    .draw()
                    .unwrap();

                for (idx, info) in cpu_info.iter().enumerate() {
                    let color = Palette99::pick(idx).stroke_width(2).filled();
                    chart
                        .draw_series(LineSeries::new(
                            timestamps
                                .into_iter()
                                .cloned()
                                .zip(info.freq.iter().cloned()),
                            color.clone(),
                        ))
                        .unwrap()
                        .label(format!("CPU{} / AVG({:.2}MHz)", idx, info.freq_avg(),))
                        .legend(move |(x, y)| {
                            PathElement::new(vec![(x, y), (x + 20, y)], color.clone())
                        });
                }
            }
            SystemCategory::CPUTemp => {
                chart = ChartBuilder::on(&area)
                    .caption("CPUs Temperature", ("sans-serif", 30).into_font())
                    .margin(10)
                    .x_label_area_size(40)
                    .y_label_area_size(50)
                    .build_cartesian_2d(timestamp_range(), 0f32..cpu_temperature_max)
                    .unwrap();

                chart
                    .configure_mesh()
                    .y_label_formatter(&|y| format!("{}°C", y))
                    .draw()
                    .unwrap();

                for (idx, info) in physical_cpu_info.iter().enumerate() {
                    let color = Palette99::pick(idx).stroke_width(2).filled();
                    chart
                        .draw_series(LineSeries::new(
                            timestamps
                                .into_iter()
                                .cloned()
                                .zip(info.temp.iter().cloned()),
                            color.clone(),
                        ))
                        .unwrap()
                        .label(format!("CPU{} / AVG({:.2}°C)", idx, info.temp_avg(),))
                        .legend(move |(x, y)| {
                            PathElement::new(vec![(x, y), (x + 20, y)], color.clone())
                        });
                }
            }
            SystemCategory::GPU => {
                chart = ChartBuilder::on(&area)
                    .caption("System GPU Usage", ("sans-serif", 30).into_font())
                    .margin(10)
                    .x_label_area_size(40)
                    .y_label_area_size(50)
                    .build_cartesian_2d(timestamp_range(), 0.0f32..100.0f32)
                    .unwrap();

                chart
                    .configure_mesh()
                    .y_label_formatter(&|y| format!("{}%", y))
                    .draw()
                    .unwrap();

                for (idx, info) in gpu_info.iter().enumerate() {
                    let color = Palette99::pick(idx).stroke_width(2).filled();
                    chart
                        .draw_series(LineSeries::new(
                            timestamps
                                .into_iter()
                                .cloned()
                                .zip(info.usage.iter().cloned()),
                            color.clone(),
                        ))
                        .unwrap()
                        .label(format!("GPU / AVG({:.2}%)", info.usage_avg()))
                        .legend(move |(x, y)| {
                            PathElement::new(vec![(x, y), (x + 20, y)], color.clone())
                        });
                }
            }
        }

        chart
            .configure_series_labels()
            .background_style(&WHITE.mix(0.8))
            .border_style(&BLACK)
            .draw()
            .unwrap();

        area_i += 1;
    }
}
