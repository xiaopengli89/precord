use crate::Pid;
use core_foundation::base::{kCFAllocatorDefault, CFRelease, ToVoid};
use core_foundation::dictionary::{CFDictionaryGetValueIfPresent, CFMutableDictionaryRef};
use core_foundation::number::{kCFNumberCharType, CFNumberGetValue, CFNumberRef};
use core_foundation::string::CFString;
use serde::Deserialize;
use std::io::{BufRead, BufReader};
use std::process::Command;
use std::sync::{mpsc, Once};
use std::sync::mpsc::{Receiver, Sender};
use std::time::Instant;
use std::{process, ptr, thread};
use std::ffi::c_void;
use IOKit_sys::*;

#[derive(Debug, Default, Deserialize)]
struct PowerMetricsResult {
    #[allow(dead_code)]
    tasks: Vec<Task>,
    processor: ProcessorInfo,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct Task {
    pid: Pid,
    #[serde(default)]
    gputime_ms_per_s: f32,
}

pub struct CommandSource {
    last_update: Instant,
    last_result: PowerMetricsResult,
    net_traffic_result: Vec<ProcessNetTraffic>,
    net_traffic_rx: Option<Receiver<ProcessNetTraffic>>,
}

impl CommandSource {
    pub fn new<T: IntoIterator<Item = Pid> + Clone>(pids: T, net_traffic: bool) -> Self {
        let net_traffic_result: Vec<_> = pids
            .clone()
            .into_iter()
            .map(|pid| ProcessNetTraffic {
                pid,
                ..Default::default()
            })
            .collect();

        let pids: Vec<_> = pids.into_iter().collect();
        let rx = if net_traffic && !pids.is_empty() {
            let (net_top_runner, rx) = NetTopRunner::new();
            thread::spawn(move || {
                net_top_runner.run(pids);
            });
            Some(rx)
        } else {
            None
        };

        Self {
            last_update: Instant::now(),
            last_result: Default::default(),
            net_traffic_result,
            net_traffic_rx: rx,
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
        let rx = if let Some(rx) = &self.net_traffic_rx {
            rx
        } else {
            return;
        };

        while let Ok(net_traffic) = rx.try_recv() {
            if let Some(p) = self
                .net_traffic_result
                .iter_mut()
                .find(|p| p.pid == net_traffic.pid)
            {
                p.bytes_in += net_traffic.bytes_in;
                p.bytes_out += net_traffic.bytes_out;
            }
        }
        let now = Instant::now();
        let d = (now - self.last_update).as_secs_f32();
        for p in self.net_traffic_result.iter_mut() {
            p.bytes_in_per_sec = (p.bytes_in as f32 / d) as _;
            p.bytes_out_per_sec = (p.bytes_out as f32 / d) as _;
            p.bytes_in = 0;
            p.bytes_out = 0;
        }
        self.last_update = now;
    }

    #[deprecated]
    #[allow(dead_code)]
    fn gpu_usage(&self, pid: Option<Pid>) -> Option<f32> {
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
            .map(|p| p.bytes_in_per_sec)
    }

    pub fn process_net_traffic_out(&self, pid: Pid) -> Option<u32> {
        self.net_traffic_result
            .iter()
            .find(|p| p.pid == pid)
            .map(|p| p.bytes_out_per_sec)
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

    #[deprecated]
    #[allow(dead_code)]
    fn poll_from_command(&mut self) {
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
    #[allow(dead_code)]
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
    bytes_in_per_sec: u32,
    bytes_out: u32,
    bytes_out_per_sec: u32,
}

struct NetTopRunner {
    tx: Sender<ProcessNetTraffic>,
}

impl NetTopRunner {
    fn new() -> (Self, Receiver<ProcessNetTraffic>) {
        let (tx, rx) = mpsc::channel();
        (Self { tx }, rx)
    }

    fn run<T: IntoIterator<Item = Pid>>(self, pids: T) {
        let mut command = Command::new("script");
        command.args([
            "-q",
            "/dev/null",
            "nettop",
            "-P",
            "-d",
            "-L",
            "0",
            "-J",
            "bytes_in,bytes_out",
        ]);

        for p in pids {
            command.args(["-p", p.to_string().as_str()]);
        }

        let mut child = command
            .stdin(process::Stdio::piped())
            .stdout(process::Stdio::piped())
            .spawn()
            .unwrap();

        let mut buf = BufReader::new(child.stdout.as_mut().unwrap());
        let mut line = String::new();
        let mut session_index = 0;
        while let Ok(read) = buf.read_line(&mut line) {
            if read == 0 {
                break;
            }

            if line.starts_with(",bytes_in,bytes_out,") {
                if session_index < 2 {
                    session_index += 1;
                }
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

            if let Err(_) = self.tx.send(ProcessNetTraffic {
                pid,
                bytes_in,
                bytes_out,
                ..Default::default()
            }) {
                break;
            }

            line.clear();
        }

        let _ = child.kill();
    }
}

static mut GET_PID_RESPONSIBLE: *mut c_void = ptr::null_mut();
static INIT: Once = Once::new();

pub fn get_pid_responsible() -> Option<extern "C" fn(libc::pid_t) -> libc::pid_t> {
    unsafe {
        INIT.call_once(|| {
            GET_PID_RESPONSIBLE = libc::dlsym(libc::RTLD_NEXT, "responsibility_get_pid_responsible_for_pid\0".as_ptr() as _);
        });
        if GET_PID_RESPONSIBLE.is_null() {
            None
        } else {
            Some(std::mem::transmute(GET_PID_RESPONSIBLE))
        }
    }
}