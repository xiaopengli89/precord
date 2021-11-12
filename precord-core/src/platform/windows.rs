use crate::Pid;
use serde::Deserialize;
use std::io::BufReader;
use std::io::{BufRead, Write};
use std::mem::MaybeUninit;
use std::ptr;
use std::{mem, process};
use winapi::shared::minwindef::DWORD;
use winapi::shared::winerror::ERROR_SUCCESS;
use winapi::um::pdh::*;

// https://docs.microsoft.com/en-us/windows/win32/perfctrs/pdh-error-codes
// 0x800007D2 (PDH_MORE_DATA)
const PDH_MORE_DATA: DWORD = 0x800007D2;
// 0x800007D5 (PDH_NO_DATA)
const PDH_NO_DATA: DWORD = 0x800007D5;

#[allow(dead_code)]
pub struct Powershell {
    process: process::Child,
    stdout: BufReader<process::ChildStdout>,
}

#[allow(dead_code)]
impl Powershell {
    pub fn new() -> Self {
        let mut p = process::Command::new("powershell")
            .args(&["-Command", "-"])
            .stdin(process::Stdio::piped())
            .stdout(process::Stdio::piped())
            .spawn()
            .unwrap();
        let o = BufReader::new(p.stdout.take().unwrap());
        Self {
            process: p,
            stdout: o,
        }
    }

    pub fn poll_gpu_percent(&mut self, pid: Option<Pid>) -> Option<f32> {
        for _ in 0..2 {
            let usage = self.poll_gpu_percent_inner(pid);
            if usage.is_some() {
                return usage;
            } else {
                // Kill previous powershell
                let _ = self.process.kill();

                // Rebuild powershell
                let mut p = process::Command::new("powershell")
                    .args(&["-Command", "-"])
                    .stdin(process::Stdio::piped())
                    .stdout(process::Stdio::piped())
                    .spawn()
                    .unwrap();
                let o = BufReader::new(p.stdout.take().unwrap());
                self.process = p;
                self.stdout = o;
            }
        }
        None
    }

    fn poll_gpu_percent_inner(&mut self, pid: Option<Pid>) -> Option<f32> {
        let pid = if let Some(pid) = pid {
            format!("pid_{}", pid)
        } else {
            "".to_string()
        };
        let mut gpu_percent = 0.0;
        let mut r = String::new();

        let stdin = self.process.stdin.as_mut().unwrap();
        let stdout = &mut self.stdout;

        for engine in ["3D", "VideoEncode", "VideoDecode", "VideoProcessing"] {
            stdin
                .write_all(
                    format!(include_str!("../../../asset/powershell.txt"), pid, engine).as_bytes(),
                )
                .unwrap();

            loop {
                r.clear();
                stdout.read_line(&mut r).ok()?;
                match r.trim() {
                    "" => continue,
                    "EOF" => break,
                    _ => {}
                }
                gpu_percent += r.trim().parse::<f32>().ok()?;
            }
        }

        Some(gpu_percent)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ProcessorInfo {
    pub percent_processor_performance: f32,
    pub processor_frequency: f32,
}

struct ProcessCounter {
    pid: Pid,
    counter: PDH_HCOUNTER,
}

pub struct Pdh {
    query: PDH_HQUERY,
    process_gpu_counters: Vec<ProcessCounter>,
    total_gpu_counter: PDH_HCOUNTER,
}

impl Pdh {
    pub fn new<T: IntoIterator<Item = Pid>>(pids: T) -> Self {
        unsafe {
            let mut query = MaybeUninit::uninit().assume_init();
            let mut r = PdhOpenQueryW(ptr::null(), 0, &mut query);
            assert_eq!(r, ERROR_SUCCESS as _);

            let process_gpu_counters = pids
                .into_iter()
                .map(|pid| {
                    let mut process_gpu_counter: PDH_HCOUNTER = MaybeUninit::uninit().assume_init();
                    r = PdhAddCounterW(
                        query,
                        widestring::U16CString::from_str(format!(
                            "\\GPU Engine(pid_{}*)\\Utilization Percentage",
                            pid
                        ))
                        .unwrap()
                        .as_ptr(),
                        0,
                        &mut process_gpu_counter,
                    );
                    assert_eq!(r, ERROR_SUCCESS as _);

                    ProcessCounter {
                        pid,
                        counter: process_gpu_counter,
                    }
                })
                .collect();

            let mut total_gpu_counter: PDH_HCOUNTER = MaybeUninit::uninit().assume_init();
            r = PdhAddCounterW(
                query,
                widestring::U16CString::from_str("\\GPU Engine(*)\\Utilization Percentage")
                    .unwrap()
                    .as_ptr(),
                0,
                &mut total_gpu_counter,
            );
            assert_eq!(r, ERROR_SUCCESS as _);

            r = PdhCollectQueryData(query);
            assert_eq!(r, ERROR_SUCCESS as _);

            Self {
                query,
                process_gpu_counters,
                total_gpu_counter,
            }
        }
    }

    pub fn update(&mut self) {
        unsafe {
            let r = PdhCollectQueryData(self.query);
            assert_eq!(r, ERROR_SUCCESS as _);
        }
    }

    pub fn poll_gpu_percent(&mut self, pid: Option<Pid>) -> Option<f32> {
        let counter = if let Some(pid) = pid {
            if let Some(counter) = self.process_gpu_counters.iter().find(|p| p.pid == pid) {
                counter.counter
            } else {
                return None;
            }
        } else {
            self.total_gpu_counter
        };

        let mut buffer_size = 0;
        let mut item_count = 0;
        let mut sum = 0.0;

        unsafe {
            let mut r = PdhGetFormattedCounterArrayW(
                counter,
                PDH_FMT_DOUBLE,
                &mut buffer_size,
                &mut item_count,
                ptr::null_mut(),
            );

            if r as DWORD == PDH_NO_DATA {
                return Some(0.0);
            }

            assert_eq!(r as DWORD, PDH_MORE_DATA);

            let mut buffer: Vec<PDH_FMT_COUNTERVALUE_ITEM_W> = Vec::with_capacity(
                buffer_size as usize / mem::size_of::<PDH_FMT_COUNTERVALUE_ITEM_W>() + 1,
            );
            buffer.set_len(item_count as _);

            r = PdhGetFormattedCounterArrayW(
                counter,
                PDH_FMT_DOUBLE,
                &mut buffer_size,
                &mut item_count,
                buffer.as_mut_ptr(),
            );

            if r as DWORD == PDH_NO_DATA {
                return Some(0.0);
            }

            assert_eq!(r, ERROR_SUCCESS as _);

            for i in 0..item_count {
                sum += *buffer[i as usize].FmtValue.u.doubleValue();
            }
        }

        Some(sum as _)
    }
}

impl Drop for Pdh {
    fn drop(&mut self) {
        unsafe {
            let r = PdhCloseQuery(self.query);
            assert_eq!(r, ERROR_SUCCESS as _);
        }
    }
}
