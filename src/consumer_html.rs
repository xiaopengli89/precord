use crate::opt::{ProcessCategory, SystemCategory};
use crate::types::ProcessInfo;
use crate::{CpuInfo, GpuInfo, PhysicalCpuInfo};
use serde_json::json;
use std::fs::File;
use std::io::Write;
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

    let mut vconcat = vec![];
    let mut vconcat_index = 0;

    for ci in 0..proc_category.len() {
        let mut values = vec![];
        let mut max_value: f32;
        let category_title;
        let unit;
        let mut total = vec![];

        match proc_category[ci] {
            ProcessCategory::CPU => {
                max_value = 100.0;
                category_title = "Process CPU Usage(%)";
                unit = "%";
            }
            ProcessCategory::Mem => {
                max_value = 10.0;
                category_title = "Process Memory Usage(M)";
                unit = "M";
            }
            ProcessCategory::GPU => {
                max_value = 100.0;
                category_title = "Process GPU Usage(%)";
                unit = "%";
            }
            ProcessCategory::FPS => {
                max_value = 60.0;
                category_title = "Process FPS";
                unit = "";
            }
        }

        for p in processes {
            let avg = p.avg_value(ci);

            total.extend_from_slice(
                vec![0.0; p.values[ci].len().saturating_sub(total.len())].as_slice(),
            );

            for (i, v) in p.values[ci].iter().copied().enumerate() {
                max_value = max_value.max(v);

                values.push(json!({
                    "Data": format!(
                        "{}({}) / AVG({:.2}{})",
                        &p.name,
                        p.pid,
                        avg,
                        unit
                    ),
                    "Time": timestamps[i].timestamp_millis(),
                    "Value": v,
                    "Command": &p.command,
                }));

                total[i] += v;
            }
        }

        if processes.len() > 1 {
            let avg: f32 = total.iter().copied().sum::<f32>() / total.len() as f32;
            for (i, v) in total.into_iter().enumerate() {
                max_value = max_value.max(v);

                values.push(json!({
                    "Data": format!(
                        "Total / AVG({:.2}{})",
                        avg,
                        unit
                    ),
                    "Time": timestamps[i].timestamp_millis(),
                    "Value": v,
                    "Command": "",
                }));
            }
        }

        vconcat.push(json!({
            "data": {"values": values},
            "width": 800,
            "height": 600,
            "encoding": {
                "color": {
                  "field": "Data",
                  "type": "nominal",
                  "legend": {
                    "labelLimit": 0,
                    "title": category_title,
                  }
                },
                "x": {"field": "Time", "type": "temporal", "timeUnit": "hoursminutesseconds", "title": "Time"},
                "y": {"field": "Value", "type": "quantitative", "title": category_title, "scale": {"domain": [0.0, max_value]}},
                "description": {
                  "field": "Command",
                  "type": "nominal",
                }
            },
            "layer": [
                {
                    "mark": {
                        "type": "line",
                        "point": {
                            "filled": false,
                            "fill": "white",
                        }
                    },
                    "encoding": {
                        "opacity": {
                          "condition": {"param": "series", "value": 1},
                          "value": 0.1,
                        },
                    },
                    "params": [
                        {
                            "name": "series",
                            "select": {"type": "point", "fields": ["Data"]},
                            "bind": "legend",
                        },
                        {
                            "name": format!("scales{}", vconcat_index),
                            "select": "interval",
                            "bind": "scales",
                        },
                    ],
                },
                {
                    "mark": {
                        "type": "point",
                        "tooltip": true,
                    },
                    "encoding": {
                        "opacity": {"value": 0},
                    },
                    "params": [
                        {
                            "name": "hover",
                            "select": {
                                "type": "point",
                                "fields": ["Time"],
                                "nearest": true,
                                "on": "mouseover",
                                "clear": "mouseout",
                            }
                        },
                    ],
                },
            ],
        }));
        vconcat_index += 1;
    }

    for &sys_c in sys_category {
        let category_title;
        let mut values = vec![];
        let max_value;

        match sys_c {
            SystemCategory::CPUFreq => {
                category_title = "CPUs Frequency(MHz)";
                max_value = cpu_frequency_max;

                for (si, info) in cpu_info.into_iter().enumerate() {
                    let avg = info.freq_avg();
                    for (i, v) in info.freq.iter().copied().enumerate() {
                        values.push(json!({
                            "Data": format!(
                                "CPU{} / AVG({:.2}MHz)",
                                si,
                                avg,
                            ),
                            "Time": timestamps[i].timestamp_millis(),
                            "Value": v,
                        }));
                    }
                }
            }
            SystemCategory::CPUTemp => {
                category_title = "CPUs Temperature(°C)";
                max_value = cpu_temperature_max;

                for (si, info) in physical_cpu_info.into_iter().enumerate() {
                    let avg = info.temp_avg();
                    for (i, v) in info.temp.iter().copied().enumerate() {
                        values.push(json!({
                            "Data": format!(
                                "CPU{} / AVG({:.2}°C)",
                                si,
                                avg,
                            ),
                            "Time": timestamps[i].timestamp_millis(),
                            "Value": v,
                        }));
                    }
                }
            }
            SystemCategory::GPU => {
                category_title = "GPUs Usage(%)";
                max_value = 100.0;

                for (si, info) in gpu_info.into_iter().enumerate() {
                    let avg = info.usage_avg();
                    for (i, v) in info.usage.iter().copied().enumerate() {
                        values.push(json!({
                            "Data": format!(
                                "GPU{} / AVG({:.2}%)",
                                si,
                                avg,
                            ),
                            "Time": timestamps[i].timestamp_millis(),
                            "Value": v,
                        }));
                    }
                }
            }
        }

        vconcat.push(json!({
            "data": {"values": values},
            "width": 800,
            "height": 600,
            "encoding": {
                "color": {
                  "field": "Data",
                  "type": "nominal",
                  "legend": {
                    "labelLimit": 0,
                    "title": category_title,
                  }
                },
                "x": {"field": "Time", "type": "temporal", "timeUnit": "hoursminutesseconds", "title": "Time"},
                "y": {"field": "Value", "type": "quantitative", "title": category_title, "scale": {"domain": [0.0, max_value]}},
            },
            "layer": [
                {
                    "mark": {
                        "type": "line",
                        "point": {
                            "filled": false,
                            "fill": "white",
                        }
                    },
                    "encoding": {
                        "opacity": {
                          "condition": {"param": "series", "value": 1},
                          "value": 0.1,
                        },
                    },
                    "params": [
                        {
                            "name": "series",
                            "select": {"type": "point", "fields": ["Data"]},
                            "bind": "legend",
                        },
                        {
                            "name": format!("scales{}", vconcat_index),
                            "select": "interval",
                            "bind": "scales",
                        },
                    ],
                },
                {
                    "mark": {
                        "type": "point",
                        "tooltip": true,
                    },
                    "encoding": {
                        "opacity": {"value": 0},
                    },
                    "params": [
                        {
                            "name": "hover",
                            "select": {
                                "type": "point",
                                "fields": ["Time"],
                                "nearest": true,
                                "on": "mouseover",
                                "clear": "mouseout",
                            }
                        },
                    ],
                },
            ],
        }));
        vconcat_index += 1;
    }

    let spec = json!({
        "$schema": "https://vega.github.io/schema/vega-lite/v5.json",
        "vconcat": vconcat,
        "resolve": {
            "scale": {
              "color": "independent",
              "size": "independent",
              "x": "independent",
              "y": "independent",
            },
            "legend": {
              "color": "independent"
            },
          },
    });

    let html_content = r#"
   <!DOCTYPE html>
<html>
  <head>
    <meta charset="utf-8" />

    <script src="https://cdn.jsdelivr.net/npm/vega@5.21.0"></script>
    <script src="https://cdn.jsdelivr.net/npm/vega-lite@5.2.0"></script>
    <script src="https://cdn.jsdelivr.net/npm/vega-embed@6.20.2"></script>

    <style media="screen">
      /* Add space between Vega-Embed links  */
      .vega-actions a {
        margin-right: 5px;
      }
    </style>
  </head>
  <body>
    <!-- Container for the visualization -->
    <div id="vis"></div>

    <script>
      // Assign the specification to a local variable vlSpec.
      var vlSpec = "#
        .to_string()
        + &spec.to_string()
        + r#";

      // Embed the visualization in the container with id `vis`
      vegaEmbed('#vis', vlSpec, {renderer: "svg"});
    </script>
  </body>
</html>
    "#;

    let mut file = File::create(output).unwrap();
    file.write_all(html_content.as_bytes()).unwrap();
    file.sync_all().unwrap();
}
