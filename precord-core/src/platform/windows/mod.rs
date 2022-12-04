#[allow(dead_code)]
mod winring0;

use crate::{Error, GpuCalculation, Pid};
use ferrisetw::native::etw_types::EventRecord;
use ferrisetw::parser::{Parser, TryParse};
use ferrisetw::provider::Provider;
use ferrisetw::trace::{TraceBaseTrait, TraceTrait, UserTrace};
use ntapi::ntpsapi;
use rand::Rng;
use regex::Regex;
use serde::Deserialize;
use std::cell::RefCell;
use std::collections::HashMap;
use std::mem;
use std::mem::MaybeUninit;
use std::os::windows::io::BorrowedHandle;
use std::os::windows::prelude::{AsHandle, AsRawHandle, FromRawHandle, OwnedHandle, RawHandle};
use std::ptr;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Instant;
use windows::core::HSTRING;
use windows::Win32::Foundation;
use windows::Win32::System::{Performance, Threading};

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

#[allow(non_camel_case_types)]
#[allow(non_snake_case)]
#[allow(dead_code)]
#[repr(C)]
struct VM_COUNTERS_EX2 {
    CountersEx: ntpsapi::VM_COUNTERS_EX,
    PrivateWorkingSetSize: usize,
    SharedCommitUsage: u64,
}

pub struct Pdh {
    update_success: bool,
    query: PdhHandle,
    total_gpu_counter: isize,
    vram_counter: isize,
    pid_re: Regex,
    read_buffer: HashMap<Pid, f32>,
}

struct PdhHandle(isize);

impl Drop for PdhHandle {
    fn drop(&mut self) {
        unsafe {
            let _r = Performance::PdhCloseQuery(self.0);
            debug_assert_eq!(Foundation::WIN32_ERROR(_r as _), Foundation::ERROR_SUCCESS);
        }
    }
}

impl Pdh {
    pub fn new<T: IntoIterator<Item = Pid>>(pids: T) -> Result<Self, Error> {
        unsafe {
            let mut query = 0;
            let mut r =
                Foundation::WIN32_ERROR(Performance::PdhOpenQueryW(None, 0, &mut query) as _);
            if r != Foundation::ERROR_SUCCESS {
                return Err(Error::Pdh(r));
            }

            let mut pdh = Self {
                update_success: true,
                query: PdhHandle(query),
                total_gpu_counter: 0,
                vram_counter: 0,
                pid_re: Regex::new(r"^pid_([0-9]+)_").unwrap(),
                read_buffer: Default::default(),
            };

            r = Foundation::WIN32_ERROR(Performance::PdhAddCounterW(
                pdh.query.0,
                &HSTRING::from("\\GPU Engine(*)\\Utilization Percentage"),
                0,
                &mut pdh.total_gpu_counter,
            ) as _);
            if r != Foundation::ERROR_SUCCESS {
                return Err(Error::Pdh(r));
            }

            // vram counter
            r = Foundation::WIN32_ERROR(Performance::PdhAddCounterW(
                pdh.query.0,
                &HSTRING::from("\\GPU Process Memory(*)\\Local Usage"),
                0,
                &mut pdh.vram_counter,
            ) as _);
            if r != Foundation::ERROR_SUCCESS {
                return Err(Error::Pdh(r));
            }

            r = Foundation::WIN32_ERROR(Performance::PdhCollectQueryData(pdh.query.0) as _);
            if r != Foundation::ERROR_SUCCESS {
                return Err(Error::Pdh(r));
            }

            Ok(pdh)
        }
    }

    fn extract_pid(&self, name: &str) -> Option<Pid> {
        let caps = self.pid_re.captures(name)?;
        caps.get(1)?.as_str().parse().ok()
    }

    pub fn update(&mut self) {
        unsafe {
            let r = Foundation::WIN32_ERROR(Performance::PdhCollectQueryData(self.query.0) as _);
            self.update_success = r == Foundation::ERROR_SUCCESS;
        }
    }

    pub fn poll_gpu_usage(
        &mut self,
        ty: GpuCounterType,
        pid: Option<Pid>,
        calc: GpuCalculation,
    ) -> Option<f32> {
        if !self.update_success {
            return None;
        }

        let counter = match ty {
            GpuCounterType::Utilization => self.total_gpu_counter,
            GpuCounterType::VRam => self.vram_counter,
        };

        let mut buffer_size = 0;
        let mut item_count = 0;

        unsafe {
            let mut r = Performance::PdhGetFormattedCounterArrayW(
                counter,
                Performance::PDH_FMT_DOUBLE,
                &mut buffer_size,
                &mut item_count,
                None,
            );

            if r == Performance::PDH_NO_DATA {
                return Some(0.0);
            }

            if r != Performance::PDH_MORE_DATA {
                return None;
            }

            let mut buffer: Vec<Performance::PDH_FMT_COUNTERVALUE_ITEM_W> = Vec::with_capacity(
                buffer_size as usize / mem::size_of::<Performance::PDH_FMT_COUNTERVALUE_ITEM_W>()
                    + 1,
            );
            buffer.set_len(item_count as _);

            r = Performance::PdhGetFormattedCounterArrayW(
                counter,
                Performance::PDH_FMT_DOUBLE,
                &mut buffer_size,
                &mut item_count,
                Some(buffer.as_mut_ptr()),
            );

            if r == Performance::PDH_NO_DATA {
                return Some(0.0);
            }

            if Foundation::WIN32_ERROR(r as _) != Foundation::ERROR_SUCCESS {
                return None;
            }

            self.read_buffer.clear();

            for i in 0..item_count {
                let name = match buffer[i as usize].szName.to_string() {
                    Ok(name) => name,
                    Err(_) => continue,
                };

                if let Some(pid) = self.extract_pid(&name) {
                    let pid_sum = self.read_buffer.entry(pid).or_default();
                    let value = buffer[i as usize].FmtValue.Anonymous.doubleValue as f32;

                    match calc {
                        GpuCalculation::Max => {
                            *pid_sum = pid_sum.max(value);
                        }
                        GpuCalculation::Sum => {
                            *pid_sum += value;
                        }
                    }
                }
            }
        }

        if let Some(pid) = pid {
            self.read_buffer.remove(&pid)
        } else {
            Some(self.read_buffer.drain().map(|(_, v)| v).sum())
        }
    }
}

pub enum GpuCounterType {
    Utilization,
    VRam,
}

struct EtwProvider {
    guid: &'static str,
    name: &'static str,
    present_event_id: Vec<u16>,
}

pub struct EtwTrace {
    last_update: Instant,
    handler: Arc<RwLock<EtwTraceHandler>>,
    _trace_guard: Receiver<()>,
}

impl EtwTrace {
    pub fn new(present: bool, tcp_ip: bool) -> Result<Self, Error> {
        let mut trace = UserTrace::new().named(format!("precord-{}", rand_string(10)));
        let handler = Arc::new(RwLock::new(EtwTraceHandler::default()));

        if present {
            for (index, provider_guid) in [
                // Microsoft-Windows-DXGI
                EtwProvider {
                    guid: "CA11C036-0102-4A2D-A6AD-F03CFED5D3C9",
                    name: "Microsoft-Windows-DXGI",
                    present_event_id: vec![0x002a],
                },
                // Microsoft-Windows-D3D9
                EtwProvider {
                    guid: "783ACA0A-790E-4d7f-8451-AA850511C6B9",
                    name: "Microsoft-Windows-D3D9",
                    present_event_id: vec![0x0001],
                },
                // Microsoft-Windows-Dwm-Core
                EtwProvider {
                    guid: "9E9BBA3C-2E38-40CB-99F4-9E8281425164",
                    name: "Microsoft-Windows-Dwm-Core",
                    present_event_id: vec![0x000f],
                },
                // Microsoft-Windows-DxgKrnl
                EtwProvider {
                    guid: "802EC45A-1E99-4B83-9920-87C98277BA9D",
                    name: "Microsoft-Windows-DxgKrnl",
                    present_event_id: vec![0x00b8],
                },
                // Microsoft-Windows-Win32k
                EtwProvider {
                    guid: "8C416C79-D49B-4F01-A467-E56D3AA8234C",
                    name: "Microsoft-Windows-Win32k",
                    present_event_id: vec![0x0029],
                },
            ]
            .into_iter()
            .enumerate()
            {
                let handler = handler.clone();
                let provider = Provider::new()
                    .by_guid(provider_guid.guid)
                    .add_callback(move |record: EventRecord, schema_locator| {
                        // Issue: https://github.com/n4r1b/ferrisetw/issues/26
                        match schema_locator.event_schema(record) {
                            Ok(schema) => {
                                if schema.provider_name() == provider_guid.name
                                    && provider_guid.present_event_id.contains(&schema.event_id())
                                {
                                    let mut guard = handler.write().unwrap();
                                    guard.add_present(index, schema.process_id());
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
                                                let mut guard = handler.write().unwrap();
                                                guard.add_network(pid, bytes, true);
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
                                                let mut guard = handler.write().unwrap();
                                                guard.add_network(pid, bytes, false);
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
            value.present_per_sec = value.present.into_iter().max().unwrap() as f32 / d;
            value.net_send_per_sec = (value.net_send as f32 / d) as _;
            value.net_recv_per_sec = (value.net_recv as f32 / d) as _;

            value.present = Default::default();
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
    fn add_present(&mut self, index: usize, pid: u32) {
        let p = self
            .trace_events
            .entry(pid)
            .or_insert(TraceEventInfo::default());
        p.present[index] = p.present[index].saturating_add(1);
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
    present: [u32; 5],
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
        unsafe {
            for pid in pids {
                let options = Threading::PROCESS_QUERY_INFORMATION | Threading::PROCESS_VM_READ;
                let r = Threading::OpenProcess(options, false, pid);
                match r {
                    Ok(handle) => {
                        vm_counter.process_counters.push(ProcessVmCounter {
                            pid,
                            handle: OwnedHandle::from_raw_handle(handle.0 as _),
                            valid: true,
                            mem: 0,
                            virtual_mem: 0,
                        });
                    }
                    Err(_) => {
                        let err = if Foundation::GetLastError() == Foundation::ERROR_ACCESS_DENIED {
                            Error::AccessDenied
                        } else {
                            Error::ProcessHandle
                        };

                        return Err(err);
                    }
                }
            }
        }

        Ok(vm_counter)
    }

    pub fn update(&mut self) {
        for p in self.process_counters.iter_mut() {
            if !p.valid {
                continue;
            }

            if is_proc_running(p.handle.as_handle()) {
                unsafe {
                    let mut info: VM_COUNTERS_EX2 = MaybeUninit::uninit().assume_init();
                    let r = Threading::NtQueryInformationProcess(
                        windows_raw_handle(p.handle.as_raw_handle()),
                        Threading::PROCESSINFOCLASS(ntpsapi::ProcessVmCounters as _),
                        mem::transmute(&mut info),
                        mem::size_of::<VM_COUNTERS_EX2>() as _,
                        ptr::null_mut(),
                    );
                    if r.is_ok() {
                        p.mem = info.PrivateWorkingSetSize;
                        p.virtual_mem = info.CountersEx.PrivateUsage;
                    }
                }
            } else {
                p.valid = false;
            }
        }
    }

    pub fn process_mem(&mut self, pid: Pid) -> Option<usize> {
        self.process_counters
            .iter_mut()
            .find(|p| p.pid == pid && p.valid)
            .map(|p| p.mem)
    }

    pub fn process_virtual_mem(&mut self, pid: Pid) -> Option<usize> {
        self.process_counters
            .iter_mut()
            .find(|p| p.pid == pid && p.valid)
            .map(|p| p.virtual_mem)
    }

    pub fn process_handles(&mut self, pid: Pid) -> Option<u32> {
        let p = self
            .process_counters
            .iter_mut()
            .find(|p| p.pid == pid && p.valid)?;
        if !is_proc_running(p.handle.as_handle()) {
            p.valid = false;
            return None;
        }
        unsafe {
            let mut count = 0;
            let r = Threading::GetProcessHandleCount(
                windows_raw_handle(p.handle.as_raw_handle()),
                &mut count,
            );
            if r.as_bool() {
                Some(count)
            } else {
                None
            }
        }
    }
}

struct ProcessVmCounter {
    pid: Pid,
    handle: OwnedHandle,
    valid: bool,
    mem: usize,
    virtual_mem: usize,
}

// Source from sysinfo
fn is_proc_running(handle: BorrowedHandle) -> bool {
    let mut exit_code = 0;
    let ret = unsafe {
        Threading::GetExitCodeProcess(windows_raw_handle(handle.as_raw_handle()), &mut exit_code)
    };
    !(!ret.as_bool() || Foundation::NTSTATUS(exit_code as _) != Foundation::STATUS_PENDING)
}

thread_local! {
    static COM_LIB: RefCell<Option<wmi::COMLibrary>> = RefCell::new(None);
}

pub fn get_com_lib() -> Option<wmi::COMLibrary> {
    COM_LIB.with(|com| {
        let mut com_ref = com.borrow_mut();
        if com_ref.is_none() {
            *com_ref = wmi::COMLibrary::new().ok();
        }
        *com_ref
    })
}

fn rand_string(length: usize) -> String {
    rand::thread_rng()
        .sample_iter(&rand::distributions::Alphanumeric)
        .take(length)
        .map(char::from)
        .collect()
}

unsafe fn windows_raw_handle(handle: RawHandle) -> Foundation::HANDLE {
    mem::transmute(handle)
}
