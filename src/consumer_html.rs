use crate::opt::{ProcessCategory, SystemCategory};
use crate::types::ProcessInfo;
use crate::{CpuInfo, GpuInfo, PhysicalCpuInfo};
use serde_json::json;
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::Path;

const CHART_WIDTH: usize = 900;
const CHART_HEIGHT: usize = 800;
const CHART_PADDING_LEFT: usize = 100;
const CHART_PADDING_RIGHT: usize = 50;
const CHART_PADDING_TOP_BOTTOM: usize = 100;

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

    let mut titles = vec![];
    let mut grids = vec![];
    let mut x_axis = vec![];
    let mut y_axis = vec![];
    let mut series = vec![];
    let mut legends = vec![];
    let mut data_zooms = vec![];
    let mut tooltips = vec![];

    for ci in 0..proc_category.len() {
        let mut max_value: f32;
        let category_title;
        let unit;
        let mut total = vec![];
        let mut legend_c = vec![];
        let mut tooltip = HashMap::new();

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
            ProcessCategory::NetIn => {
                max_value = 1000.0;
                category_title = "Process Net In(KBps)";
                unit = "KBps";
            }
            ProcessCategory::NetOut => {
                max_value = 1000.0;
                category_title = "Process Net Out(KBps)";
                unit = "KBps";
            }
        }

        for p in processes {
            let avg = p.avg_value(ci);

            total.extend_from_slice(
                vec![0.0; p.values[ci].len().saturating_sub(total.len())].as_slice(),
            );

            let data: Vec<_> = p.values[ci]
                .iter()
                .copied()
                .enumerate()
                .zip(timestamps.into_iter())
                .map(|((i, v), t)| {
                    max_value = max_value.max(v);
                    total[i] += v;

                    json!([t, v])
                })
                .collect();
            let name = format!("{} / AVG({:.2}{}) / {}", p.pid, avg, unit, &p.name);
            series.push(json!({
                "name": &name,
                "type": "line",
                "showSymbol": false,
                "xAxisIndex": grids.len(),
                "yAxisIndex": grids.len(),
                "data": data,
                "markLine": {
                    "data": [{"type": "average"}],
                },
            }));
            legend_c.push(json!({
                "name": &name,
            }));
            tooltip.insert(name, p.command.clone());
        }

        if processes.len() > 1 {
            let avg: f32 = total.iter().copied().sum::<f32>() / total.len() as f32;

            let data: Vec<_> = total
                .into_iter()
                .zip(timestamps.into_iter())
                .map(|(v, t)| {
                    max_value = max_value.max(v);

                    json!([t, v])
                })
                .collect();
            series.push(json!({
                "name": format!("Total / AVG({:.2}{})", avg, unit),
                "type": "line",
                "showSymbol": false,
                "xAxisIndex": grids.len(),
                "yAxisIndex": grids.len(),
                "data": data,
                "markLine": {
                    "data": [{"type": "average"}],
                },
            }));
            legend_c.push(json!({
                "name": format!("Total / AVG({:.2}{})", avg, unit),
            }));
        }

        x_axis.push(json!({
            "gridIndex": grids.len(),
            "type": "time",
        }));
        y_axis.push(json!({
            "gridIndex": grids.len(),
            "min": 0.0,
            "max": max_value.ceil(),
            "axisLabel": {
                "formatter": format!("{{value}}{}", unit),
            },
        }));
        titles.push(json!({
            "text": category_title,
            "textAlign": "center",
            "top": format!("{}px", 800 * grids.len() + 20),
            "left": "450px",
        }));
        legends.push(json!({
            "type": "scroll",
            "orient": "vertical",
            "left": "900px",
            "bottom": 0,
            "top": format!("{}px", 800 * grids.len() + 100),
            "data": legend_c,
            "tooltip": {
                "show": true,
                "extraCssText": "max-width:500px; white-space:normal;",
            },
        }));
        data_zooms.push(json!({
            "xAxisIndex": [grids.len()],
            "top": format!("{}px", 800 * grids.len() + 730),
        }));
        grids.push(json!({
            "width": format!("{}px", CHART_WIDTH - CHART_PADDING_LEFT - CHART_PADDING_RIGHT),
            "height": format!("{}px", CHART_HEIGHT - CHART_PADDING_TOP_BOTTOM * 2),
            "left": format!("{}px", CHART_PADDING_LEFT),
            "top": format!("{}px", CHART_HEIGHT * grids.len() + CHART_PADDING_TOP_BOTTOM),
        }));
        tooltips.push(tooltip);
    }

    for &sys_c in sys_category {
        let max_value;
        let category_title;
        let unit;
        let mut legend_c = vec![];
        let tooltip = HashMap::new();

        match sys_c {
            SystemCategory::CPUFreq => {
                max_value = cpu_frequency_max;
                category_title = "CPUs Frequency(MHz)";
                unit = "MHz";

                for (si, info) in cpu_info.into_iter().enumerate() {
                    let avg = info.freq_avg();
                    let data: Vec<_> = info
                        .freq
                        .iter()
                        .copied()
                        .zip(timestamps.into_iter())
                        .map(|(v, t)| json!([t, v]))
                        .collect();
                    let name = format!("CPU{} / AVG({:.2}{})", si, avg, unit);
                    series.push(json!({
                        "name": &name,
                        "type": "line",
                        "showSymbol": false,
                        "xAxisIndex": grids.len(),
                        "yAxisIndex": grids.len(),
                        "data": data,
                        "markLine": {
                            "data": [{"type": "average"}],
                        },
                    }));
                    legend_c.push(json!({
                        "name": &name,
                    }));
                }
            }
            SystemCategory::CPUTemp => {
                max_value = cpu_temperature_max;
                category_title = "CPUs Temperature(°C)";
                unit = "°C";

                for (si, info) in physical_cpu_info.into_iter().enumerate() {
                    let avg = info.temp_avg();
                    let data: Vec<_> = info
                        .temp
                        .iter()
                        .copied()
                        .zip(timestamps.into_iter())
                        .map(|(v, t)| json!([t, v]))
                        .collect();
                    let name = format!("CPU{} / AVG({:.2}{})", si, avg, unit);
                    series.push(json!({
                        "name": &name,
                        "type": "line",
                        "showSymbol": false,
                        "xAxisIndex": grids.len(),
                        "yAxisIndex": grids.len(),
                        "data": data,
                        "markLine": {
                            "data": [{"type": "average"}],
                        },
                    }));
                    legend_c.push(json!({
                        "name": &name,
                    }));
                }
            }
            SystemCategory::GPU => {
                max_value = 100.0;
                category_title = "GPUs Usage(%)";
                unit = "%";

                for (si, info) in gpu_info.into_iter().enumerate() {
                    let avg = info.usage_avg();
                    let data: Vec<_> = info
                        .usage
                        .iter()
                        .copied()
                        .zip(timestamps.into_iter())
                        .map(|(v, t)| json!([t, v]))
                        .collect();
                    let name = format!("GPU{} / AVG({:.2}{})", si, avg, unit);
                    series.push(json!({
                        "name": &name,
                        "type": "line",
                        "showSymbol": false,
                        "xAxisIndex": grids.len(),
                        "yAxisIndex": grids.len(),
                        "data": data,
                        "markLine": {
                            "data": [{"type": "average"}],
                        },
                    }));
                    legend_c.push(json!({
                        "name": &name,
                    }));
                }
            }
        }

        x_axis.push(json!({
            "gridIndex": grids.len(),
            "type": "time",
        }));
        y_axis.push(json!({
            "gridIndex": grids.len(),
            "min": 0.0,
            "max": max_value.ceil(),
            "axisLabel": {
                "formatter": format!("{{value}}{}", unit),
            },
        }));
        titles.push(json!({
            "text": category_title,
            "textAlign": "center",
            "top": format!("{}px", 800 * grids.len()),
            "left": "450px",
        }));
        legends.push(json!({
            "type": "scroll",
            "orient": "vertical",
            "left": "900px",
            "bottom": 0,
            "top": format!("{}px", 800 * grids.len() + 100),
            "data": legend_c,
            "tooltip": {
                "show": true,
                "extraCssText": "max-width:500px; white-space:normal;",
            },
        }));
        data_zooms.push(json!({
            "xAxisIndex": [grids.len()],
            "top": format!("{}px", 800 * grids.len() + 730),
        }));
        grids.push(json!({
            "width": format!("{}px", CHART_WIDTH - CHART_PADDING_LEFT - CHART_PADDING_RIGHT),
            "height": format!("{}px", CHART_HEIGHT - CHART_PADDING_TOP_BOTTOM * 2),
            "left": format!("{}px", CHART_PADDING_LEFT),
            "top": format!("{}px", CHART_HEIGHT * grids.len() + CHART_PADDING_TOP_BOTTOM),
        }));
        tooltips.push(tooltip);
    }

    let grid_len = grids.len();
    let option = json!({
        "tooltip": {
            "show": true,
            "trigger": "axis",
        },
        "grid": grids,
        "title": titles,
        "xAxis": x_axis,
        "yAxis": y_axis,
        "legend": legends,
        "series": series,
        "dataZoom": data_zooms,
    });

    let html_content = r#"
   <!DOCTYPE html>
<html>
  <head>
    <meta charset="utf-8" />
    <script src="https://cdn.jsdelivr.net/npm/echarts@5.2.2/dist/echarts.min.js"></script>
    <style>
        #main {
            margin: 20px auto;
        }
    </style>
  </head>
  <body>
    <div id="main" style="width: 1200px; height: "#
        .to_string()
        + &(800 * grid_len).to_string()
        + &r#"px;"></div>
    <script>
      var myChart = echarts.init(document.getElementById('main'), null, { renderer: 'svg' });
      var option = "#
            .to_string()
        + &option.to_string()
        + r#";

      var tooltips = "#
        + &serde_json::to_string(&tooltips).unwrap()
        + r#";
      option.legend.forEach(function(l, i) {
        l.tooltip.formatter = function(name) {
            console.log(name);
            var t = tooltips[i][name.name];
            t = t ? "<br/>" + t : "";
            return "<b>" + name.name + "</b>" + t;
        };
      });

      myChart.setOption(option);
    </script>
  </body>
</html>
    "#;

    let mut file = File::create(output).unwrap();
    file.write_all(html_content.as_bytes()).unwrap();
    file.sync_all().unwrap();
}
