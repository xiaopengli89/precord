use crate::{Error, GpuCalculation, Pid};
use ferrisetw::native::etw_types::EventRecord;
use ferrisetw::parser::{Parser, TryParse};
use ferrisetw::provider::Provider;
use ferrisetw::trace::{TraceBaseTrait, TraceTrait, UserTrace};
use ntapi::ntpsapi::{NtQueryInformationProcess, ProcessVmCounters, VM_COUNTERS_EX};
use serde::Deserialize;
use std::collections::HashMap;
use std::io::BufReader;
use std::io::{BufRead, Write};
use std::mem::MaybeUninit;
use std::ptr;
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Once, RwLock};
use std::thread;
use std::time::Instant;
use std::{mem, process};
use winapi::shared::basetsd::SIZE_T;
use winapi::shared::minwindef::{DWORD, FALSE};
use winapi::shared::ntdef::{NT_SUCCESS, ULONGLONG};
use winapi::shared::winerror::ERROR_SUCCESS;
use winapi::um::handleapi::CloseHandle;
use winapi::um::pdh::*;
use winapi::um::processthreadsapi::{GetExitCodeProcess, OpenProcess};
use winapi::um::winnt::{HANDLE, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ, STATUS_PENDING};
use windows::Win32::Foundation::{GetLastError, ERROR_ACCESS_DENIED};

// https://docs.microsoft.com/en-us/windows/win32/perfctrs/pdh-error-codes
// 0x800007D2 (PDH_MORE_DATA)
const PDH_MORE_DATA: DWORD = 0x800007D2;
// 0x800007D5 (PDH_NO_DATA)
const PDH_NO_DATA: DWORD = 0x800007D5;
// 0xC0000BBA (PDH_CSTATUS_INVALID_DATA)
const PDH_CSTATUS_INVALID_DATA: DWORD = 0xC0000BBA;

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

    pub fn poll_gpu_usage(&mut self, pid: Option<Pid>) -> Option<f32> {
        for _ in 0..2 {
            let usage = self.poll_gpu_usage_inner(pid);
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

    fn poll_gpu_usage_inner(&mut self, pid: Option<Pid>) -> Option<f32> {
        let pid = if let Some(pid) = pid {
            format!("pid_{}", pid)
        } else {
            "".to_string()
        };
        let mut gpu_usage = 0.0;
        let mut r = String::new();

        let stdin = self.process.stdin.as_mut().unwrap();
        let stdout = &mut self.stdout;

        for engine in ["3D", "VideoEncode", "VideoDecode", "VideoProcessing"] {
            stdin
                .write_all(
                    format!(include_str!("../../asset/powershell.txt"), pid, engine).as_bytes(),
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
                gpu_usage += r.trim().parse::<f32>().ok()?;
            }
        }

        Some(gpu_usage)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ProcessorInfo {
    pub percent_processor_performance: f32,
    pub processor_frequency: f32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ThermalZoneInformation {
    pub temperature: f32,
}

struct ProcessCounter {
    pid: Pid,
    counter: PDH_HCOUNTER,
}

#[allow(non_camel_case_types)]
#[allow(non_snake_case)]
#[allow(dead_code)]
struct VM_COUNTERS_EX2 {
    CountersEx: VM_COUNTERS_EX,
    PrivateWorkingSetSize: SIZE_T,
    SharedCommitUsage: ULONGLONG,
}

pub struct Pdh {
    update_success: bool,
    query: PDH_HQUERY,
    process_gpu_counters: Vec<ProcessCounter>,
    total_gpu_counter: PDH_HCOUNTER,
}

impl Pdh {
    pub fn new<T: IntoIterator<Item = Pid>>(pids: T) -> Result<Self, Error> {
        unsafe {
            let mut query = MaybeUninit::uninit().assume_init();
            let mut r = PdhOpenQueryW(ptr::null(), 0, &mut query);
            if r != ERROR_SUCCESS as PDH_STATUS {
                return Err(Error::Pdh(r));
            }

            let mut pdh = Self {
                update_success: true,
                query,
                process_gpu_counters: vec![],
                total_gpu_counter: MaybeUninit::uninit().assume_init(),
            };

            for pid in pids {
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
                if r != ERROR_SUCCESS as PDH_STATUS {
                    return Err(Error::Pdh(r));
                }

                pdh.process_gpu_counters.push(ProcessCounter {
                    pid,
                    counter: process_gpu_counter,
                });
            }

            r = PdhAddCounterW(
                query,
                widestring::U16CString::from_str("\\GPU Engine(*)\\Utilization Percentage")
                    .unwrap()
                    .as_ptr(),
                0,
                &mut pdh.total_gpu_counter,
            );
            if r != ERROR_SUCCESS as PDH_STATUS {
                return Err(Error::Pdh(r));
            }

            r = PdhCollectQueryData(query);
            if r != ERROR_SUCCESS as PDH_STATUS {
                return Err(Error::Pdh(r));
            }

            Ok(pdh)
        }
    }

    pub fn update(&mut self) {
        unsafe {
            let r = PdhCollectQueryData(self.query);
            self.update_success = r as DWORD == ERROR_SUCCESS;
        }
    }

    pub fn poll_gpu_usage(&mut self, pid: Option<Pid>, calc: GpuCalculation) -> Option<f32> {
        if !self.update_success {
            return None;
        }

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
        let mut sum: f32 = 0.0;

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

            if r as DWORD != PDH_MORE_DATA {
                return None;
            }

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

            if r as DWORD != ERROR_SUCCESS {
                return None;
            }

            for i in 0..item_count {
                let value = (*buffer[i as usize].FmtValue.u.doubleValue()) as f32;

                match calc {
                    GpuCalculation::Max => {
                        sum = sum.max(value);
                    }
                    GpuCalculation::Sum => {
                        sum += value;
                    }
                }
            }
        }

        Some(sum)
    }
}

impl Drop for Pdh {
    fn drop(&mut self) {
        unsafe {
            let _r = PdhCloseQuery(self.query);
            debug_assert_eq!(_r, ERROR_SUCCESS as _);
        }
    }
}

struct EtwProvider {
    guid: &'static str,
    name: &'static str,
    present_event_id: u16,
}

pub struct EtwTrace {
    last_update: Instant,
    handler: Arc<RwLock<EtwTraceHandler>>,
    _trace_guard: Receiver<()>,
}

impl EtwTrace {
    pub fn new(present: bool, tcp_ip: bool) -> Result<Self, Error> {
        let mut trace = UserTrace::new().named("precord".to_string());
        let handler = Arc::new(RwLock::new(EtwTraceHandler::default()));

        if present {
            for provider_guid in [
                // Microsoft-Windows-DXGI
                EtwProvider {
                    guid: "CA11C036-0102-4A2D-A6AD-F03CFED5D3C9",
                    name: "Microsoft-Windows-DXGI",
                    present_event_id: 0x002a,
                },
                // Microsoft-Windows-D3D9
                EtwProvider {
                    guid: "783ACA0A-790E-4d7f-8451-AA850511C6B9",
                    name: "Microsoft-Windows-D3D9",
                    present_event_id: 0x0001,
                },
                // Microsoft-Windows-Dwm-Core
                EtwProvider {
                    guid: "9E9BBA3C-2E38-40CB-99F4-9E8281425164",
                    name: "Microsoft-Windows-Dwm-Core",
                    present_event_id: 0x000f,
                },
                // Microsoft-Windows-DxgKrnl
                EtwProvider {
                    guid: "802EC45A-1E99-4B83-9920-87C98277BA9D",
                    name: "Microsoft-Windows-DxgKrnl",
                    present_event_id: 0x00aa, // RenderKm
                },
            ] {
                let handler = handler.clone();
                let provider = Provider::new()
                    .by_guid(provider_guid.guid)
                    .add_callback(move |record: EventRecord, schema_locator| {
                        match schema_locator.event_schema(record) {
                            Ok(schema) => {
                                if schema.provider_name() == provider_guid.name
                                    && schema.event_id() == provider_guid.present_event_id
                                {
                                    handler.write().unwrap().add_present(schema.process_id());
                                }
                            }
                            Err(_) => {}
                        };
                    })
                    .build()
                    .unwrap();
                trace = trace.enable(provider);
            }
        }

        if tcp_ip {
            let handler = handler.clone();
            let provider = Provider::new()
                .by_guid("7DD42A49-5329-4832-8DFD-43D979153A88") // Microsoft-Windows-Kernel-Network
                .add_callback(move |record: EventRecord, schema_locator| {
                    match schema_locator.event_schema(record) {
                        Ok(schema) => {
                            if schema.provider_name() == "Microsoft-Windows-Kernel-Network" {
                                match schema.event_id() {
                                    // https://github.com/repnz/etw-providers-docs/blob/master/Manifests-Win10-17134/Microsoft-Windows-Kernel-Network.xml
                                    10 | 26 | 42 | 58 => {
                                        let mut parser = Parser::create(&schema);
                                        match (
                                            TryParse::<u32>::try_parse(&mut parser, "PID"),
                                            TryParse::<u32>::try_parse(&mut parser, "size"),
                                        ) {
                                            (Ok(pid), Ok(bytes)) => {
                                                handler
                                                    .write()
                                                    .unwrap()
                                                    .add_network(pid, bytes, true);
                                            }
                                            _ => {}
                                        }
                                    }
                                    11 | 27 | 43 | 59 => {
                                        let mut parser = Parser::create(&schema);
                                        match (
                                            TryParse::<u32>::try_parse(&mut parser, "PID"),
                                            TryParse::<u32>::try_parse(&mut parser, "size"),
                                        ) {
                                            (Ok(pid), Ok(bytes)) => {
                                                handler
                                                    .write()
                                                    .unwrap()
                                                    .add_network(pid, bytes, false);
                                            }
                                            _ => {}
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                        Err(_) => {}
                    };
                })
                .build()
                .unwrap();
            trace = trace.enable(provider);
        }

        let (tx, rx) = mpsc::sync_channel(0);
        thread::spawn(move || {
            match trace.start() {
                Ok(_trace) => {
                    // Success signal
                    let _ = tx.send(());
                    // Block here
                    let _ = tx.send(());
                }
                Err(_err) => {}
            }
        });

        if rx.recv().is_err() {
            return Err(Error::Etw);
        }

        Ok(Self {
            last_update: Instant::now(),
            handler,
            _trace_guard: rx,
        })
    }

    pub fn fps(&self, pid: Pid) -> f32 {
        self.handler.read().unwrap().fps(pid as _)
    }

    pub fn net_send_per_sec(&self, pid: Pid) -> u32 {
        self.handler.read().unwrap().net_send_per_sec(pid as _)
    }

    pub fn net_recv_per_sec(&self, pid: Pid) -> u32 {
        self.handler.read().unwrap().net_recv_per_sec(pid as _)
    }

    pub fn update(&mut self) {
        let now = Instant::now();
        let d = (now - self.last_update).as_secs_f32();
        for value in self.handler.write().unwrap().trace_events.values_mut() {
            value.present_per_sec = value.present as f32 / d;
            value.net_send_per_sec = (value.net_send as f32 / d) as _;
            value.net_recv_per_sec = (value.net_recv as f32 / d) as _;

            value.present = 0;
            value.net_send = 0;
            value.net_recv = 0;
        }
        self.last_update = now;
    }
}

#[derive(Default)]
struct EtwTraceHandler {
    trace_events: HashMap<u32, TraceEventInfo>,
}

impl EtwTraceHandler {
    fn add_present(&mut self, pid: u32) {
        let p = self
            .trace_events
            .entry(pid)
            .or_insert(TraceEventInfo::default());
        p.present = p.present.saturating_add(1);
    }

    fn add_network(&mut self, pid: u32, bytes: u32, is_send: bool) {
        let p = self
            .trace_events
            .entry(pid)
            .or_insert(TraceEventInfo::default());
        if is_send {
            p.net_send = p.net_send.saturating_add(bytes);
        } else {
            p.net_recv = p.net_recv.saturating_add(bytes);
        }
    }

    fn fps(&self, pid: u32) -> f32 {
        if let Some(p) = self.trace_events.get(&pid) {
            p.present_per_sec
        } else {
            0.0
        }
    }

    fn net_send_per_sec(&self, pid: u32) -> u32 {
        if let Some(p) = self.trace_events.get(&pid) {
            p.net_send_per_sec
        } else {
            0
        }
    }

    fn net_recv_per_sec(&self, pid: u32) -> u32 {
        if let Some(p) = self.trace_events.get(&pid) {
            p.net_recv_per_sec
        } else {
            0
        }
    }
}

#[derive(Default)]
struct TraceEventInfo {
    present: u32,
    present_per_sec: f32,
    net_send: u32,
    net_send_per_sec: u32,
    net_recv: u32,
    net_recv_per_sec: u32,
}

pub struct VmCounter {
    process_counters: Vec<ProcessVmCounter>,
}

impl VmCounter {
    pub fn new<T: IntoIterator<Item = Pid>>(pids: T) -> Result<Self, Error> {
        let mut vm_counter = Self {
            process_counters: vec![],
        };

        for pid in pids {
            let options = PROCESS_QUERY_INFORMATION | PROCESS_VM_READ;
            let handle = unsafe { OpenProcess(options, FALSE, pid as DWORD) };
            if handle.is_null() {
                let err = unsafe {
                    if GetLastError() == ERROR_ACCESS_DENIED {
                        Error::AccessDenied
                    } else {
                        Error::ProcessHandle
                    }
                };

                return Err(err);
            } else {
                vm_counter.process_counters.push(ProcessVmCounter {
                    pid,
                    handle,
                    valid: true,
                    mem: 0.0,
                });
            }
        }

        Ok(vm_counter)
    }

    pub fn process_mem(&mut self, pid: Pid) -> Option<f32> {
        self.process_counters
            .iter_mut()
            .find(|p| p.pid == pid && p.valid)
            .map(|p| unsafe {
                if is_proc_running(p.handle) {
                    let mut info: VM_COUNTERS_EX2 = MaybeUninit::uninit().assume_init();
                    let r = NtQueryInformationProcess(
                        p.handle,
                        ProcessVmCounters,
                        std::mem::transmute(&mut info),
                        std::mem::size_of::<VM_COUNTERS_EX2>() as _,
                        std::ptr::null_mut(),
                    );
                    if NT_SUCCESS(r) {
                        Some((info.PrivateWorkingSetSize >> 10) as f32)
                    } else {
                        None
                    }
                } else {
                    p.valid = false;
                    None
                }
            })
            .flatten()
    }
}

struct ProcessVmCounter {
    pid: Pid,
    handle: HANDLE,
    valid: bool,
    mem: f32,
}

impl Drop for ProcessVmCounter {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.handle);
        }
    }
}

// Source from sysinfo
fn is_proc_running(handle: HANDLE) -> bool {
    let mut exit_code = 0;
    let ret = unsafe { GetExitCodeProcess(handle, &mut exit_code) };
    !(ret == FALSE || exit_code != STATUS_PENDING)
}

static mut COM_LIB: Option<wmi::COMLibrary> = None;
static INIT: Once = Once::new();

pub fn get_com_lib() -> Option<Rc<wmi::COMLibrary>> {
    unsafe {
        INIT.call_once(|| {
            COM_LIB = wmi::COMLibrary::new().ok();
        });

        let com_lib = COM_LIB.as_ref().map(|c| {
            let c = Rc::new(mem::transmute_copy::<_, wmi::COMLibrary>(c));
            mem::forget(c.clone());
            c
        });
        com_lib
    }
}
