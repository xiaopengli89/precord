use crate::Pid;
use core_foundation::base::{kCFAllocatorDefault, CFRelease, ToVoid};
use core_foundation::dictionary::{CFDictionaryGetValueIfPresent, CFMutableDictionaryRef};
use core_foundation::number::{kCFNumberCharType, CFNumberGetValue, CFNumberRef};
use core_foundation::string::CFString;
use serde::Deserialize;
use std::io::{BufRead, Cursor};
use std::process::Command;
use std::time::Instant;
use IOKit_sys::*;

#[derive(Debug, Default, Deserialize)]
struct PowerMetricsResult {
    tasks: Vec<Task>,
    processor: ProcessorInfo,
}

#[derive(Debug, Deserialize)]
pub struct Task {
    pid: Pid,
    #[serde(default)]
    gputime_ms_per_s: f32,
}

pub struct CommandSource {
    last_result: PowerMetricsResult,
    net_traffic_result: Vec<ProcessNetTraffic>,
}

impl CommandSource {
    pub fn new<T: IntoIterator<Item = Pid>>(pids: T) -> Self {
        let net_traffic_result: Vec<_> = pids
            .into_iter()
            .map(|pid| ProcessNetTraffic {
                pid,
                ..Default::default()
            })
            .collect();

        Self {
            last_result: Default::default(),
            net_traffic_result,
        }
    }

    pub fn update_cpu_frequency(&mut self) {
        let o = Command::new("powermetrics")
            .args([
                "--samplers",
                "tasks,cpu_power",
                "--show-process-gpu",
                "-n1",
                "-i1000",
                "-f",
                "plist",
            ])
            .output()
            .unwrap();

        match o.status.code() {
            Some(0) => {}
            _ => {
                panic!("Error: {}", String::from_utf8_lossy(&o.stderr));
            }
        }

        self.last_result = plist::from_bytes(o.stdout.as_slice()).unwrap();
    }

    pub fn update_net_traffic(&mut self) {
        let mut command = Command::new("nettop");
        command.args(["-P", "-d", "-L", "2", "-J", "bytes_in,bytes_out"]);

        for p in self.net_traffic_result.iter() {
            command.args(["-p", p.pid.to_string().as_str()]);
        }

        let o = command.output().unwrap();

        match o.status.code() {
            Some(0) => {}
            _ => {
                panic!("Error: {}", String::from_utf8_lossy(&o.stderr));
            }
        }

        for p in self.net_traffic_result.iter_mut() {
            p.bytes_in = 0;
            p.bytes_out = 0;
        }
        let mut cursor = Cursor::new(o.stdout);
        let mut line = String::new();
        let mut session_index = 0;
        while let Ok(read) = cursor.read_line(&mut line) {
            if read == 0 {
                break;
            }

            if line.starts_with(",bytes_in,bytes_out,") {
                session_index += 1;
                line.clear();
                continue;
            }

            if session_index < 2 {
                line.clear();
                continue;
            }

            let data: Vec<_> = line.split(',').collect();
            if data.len() < 3 {
                line.clear();
                continue;
            }

            let process: Vec<_> = data[0].split('.').collect();
            if process.len() < 2 {
                line.clear();
                continue;
            }

            let pid: Pid = process.last().unwrap().parse().unwrap();
            let bytes_in: u32 = data[1].parse().unwrap();
            let bytes_out: u32 = data[2].parse().unwrap();

            self.net_traffic_result
                .iter_mut()
                .find(|p| p.pid == pid)
                .map(|p| {
                    p.bytes_in = bytes_in;
                    p.bytes_out = bytes_out;
                });

            line.clear()
        }
    }

    pub fn gpu_usage(&self, pid: Option<Pid>) -> Option<f32> {
        let pid = if let Some(pid) = pid {
            pid
        } else {
            unimplemented!()
        };
        self.last_result
            .tasks
            .iter()
            .find(|task| task.pid == pid)
            .map(|task| task.gputime_ms_per_s / 10.0)
    }

    pub fn cpu_frequency(&self) -> Vec<f32> {
        if !self.last_result.processor.clusters.is_empty() {
            // Apple Silicon
            self.last_result
                .processor
                .clusters
                .iter()
                .map(|c| c.cpus.iter())
                .flatten()
                .map(|c| c.freq_hz / 1_000_000.0)
                .collect()
        } else {
            // Intel
            self.last_result
                .processor
                .packages
                .iter()
                .map(|p| p.cores.iter())
                .flatten()
                .map(|c| c.cpus.iter())
                .flatten()
                .map(|c| c.freq_hz / 1_000_000.0)
                .collect()
        }
    }

    pub fn process_net_traffic_in(&self, pid: Pid) -> Option<u32> {
        self.net_traffic_result
            .iter()
            .find(|p| p.pid == pid)
            .map(|p| p.bytes_in)
    }

    pub fn process_net_traffic_out(&self, pid: Pid) -> Option<u32> {
        self.net_traffic_result
            .iter()
            .find(|p| p.pid == pid)
            .map(|p| p.bytes_out)
    }
}

#[derive(Debug, Default, Deserialize)]
struct ProcessorInfo {
    #[serde(default)]
    clusters: Vec<Cluster>,
    #[serde(default)]
    packages: Vec<Package>,
}

#[derive(Debug, Deserialize)]
struct Cpu {
    freq_hz: f32,
}

// Apple Silicon
#[derive(Debug, Deserialize)]
struct Cluster {
    cpus: Vec<Cpu>,
}

// Intel
#[derive(Debug, Deserialize)]
struct Package {
    cores: Vec<Core>,
}

#[derive(Debug, Deserialize)]
struct Core {
    cpus: Vec<Cpu>,
}

pub struct IOKitRegistry {
    last_result: Vec<IOKitResult>,
}

impl IOKitRegistry {
    pub fn new() -> Self {
        Self {
            last_result: vec![],
        }
    }

    pub fn poll(&mut self) {
        self.last_result.clear();

        unsafe {
            let io_acc = IOServiceMatching("IOAccelerator\0".as_ptr() as _);
            let mut it: io_iterator_t = 0;
            if IOServiceGetMatchingServices(kIOMasterPortDefault, io_acc, &mut it)
                == kIOReturnSuccess
            {
                #[allow(unused_assignments)]
                let mut entry: io_registry_entry_t = 0;
                loop {
                    entry = IOIteratorNext(it);
                    if entry == 0 {
                        break;
                    }

                    let mut props: CFMutableDictionaryRef = std::ptr::null_mut();
                    if IORegistryEntryCreateCFProperties(
                        entry,
                        std::mem::transmute(&mut props),
                        std::mem::transmute(kCFAllocatorDefault),
                        0,
                    ) == kIOReturnSuccess
                    {
                        let mut perf_props: CFMutableDictionaryRef = std::ptr::null_mut();
                        if CFDictionaryGetValueIfPresent(
                            props,
                            CFString::new("PerformanceStatistics").to_void(),
                            std::mem::transmute(&mut perf_props),
                        ) != 0
                        {
                            let mut device_utilization_ref: CFNumberRef = std::ptr::null_mut();
                            if CFDictionaryGetValueIfPresent(
                                perf_props,
                                CFString::new("Device Utilization %").to_void(),
                                std::mem::transmute(&mut device_utilization_ref),
                            ) != 0
                            {
                                let mut device_utilization: u8 = 0;
                                if CFNumberGetValue(
                                    device_utilization_ref,
                                    kCFNumberCharType,
                                    std::mem::transmute(&mut device_utilization),
                                ) {
                                    self.last_result.push(IOKitResult {
                                        // TODO: Get accelerator name
                                        io_class: "".to_string(),
                                        performance_statistics: PerformanceStatistics {
                                            device_utilization: device_utilization as _,
                                        },
                                    });
                                }
                            }
                        }
                        CFRelease(props.to_void());
                    }

                    IOObjectRelease(entry);
                }
                IOObjectRelease(it);
            } else {
                CFRelease(io_acc as _);
            }
        }
    }

    pub fn poll_from_command(&mut self) {
        let o = Command::new("ioreg")
            .args(["-r", "-d", "1", "-w", "0", "-c", "IOAccelerator", "-a"])
            .output()
            .unwrap();

        match o.status.code() {
            Some(0) => {}
            _ => {
                panic!("Error: {}", String::from_utf8_lossy(&o.stderr));
            }
        }

        self.last_result = plist::from_bytes(o.stdout.as_slice()).unwrap();
    }

    pub fn sys_gpu_usage(&self) -> f32 {
        let mut max: f32 = 0.0;
        for r in &self.last_result {
            max = max.max(r.performance_statistics.device_utilization);
        }
        max
    }
}

#[derive(Debug, Deserialize)]
struct IOKitResult {
    #[serde(rename = "IOClass")]
    io_class: String,
    #[serde(rename = "PerformanceStatistics")]
    performance_statistics: PerformanceStatistics,
}

#[derive(Debug, Deserialize)]
struct PerformanceStatistics {
    #[serde(rename = "Device Utilization %", default)]
    device_utilization: f32,
}

#[derive(Default)]
struct ProcessNetTraffic {
    pid: Pid,
    bytes_in: u32,
    bytes_out: u32,
}
