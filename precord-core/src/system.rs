use crate::platform;
#[cfg(target_os = "macos")]
use crate::platform::macos::{get_pid_responsible, CommandSource, IOKitRegistry};
#[cfg(target_os = "windows")]
use crate::platform::windows::{EtwTrace, Pdh, ProcessorInfo, VmCounter};
use crate::{Error, GpuCalculation, Pid};
use bitflags::bitflags;
use std::time::{Duration, Instant};
use sysinfo::{CpuExt, CpuRefreshKind, PidExt, ProcessExt, ProcessRefreshKind, SystemExt};

pub struct System {
    last_update: Instant,
    last_duration: Duration,
    features: Features,
    sysinfo_system: Option<sysinfo::System>,
    refresh_kind: sysinfo::RefreshKind,
    #[cfg(target_os = "macos")]
    command_source: Option<CommandSource>,
    #[cfg(target_os = "macos")]
    ioreg: Option<IOKitRegistry>,
    #[cfg(target_os = "macos")]
    smc: Option<smc::SMC>,
    #[cfg(target_os = "windows")]
    pdh: Option<Pdh>,
    #[cfg(target_os = "windows")]
    wmi_conn: Option<wmi::WMIConnection>,
    #[cfg(target_os = "windows")]
    etw_trace: Option<EtwTrace>,
    #[cfg(target_os = "windows")]
    vm_counter: Option<VmCounter>,
    #[cfg(target_os = "windows")]
    battery: Option<platform::windows::Battery>,
}

impl System {
    pub fn new<T: IntoIterator<Item = Pid> + Clone>(
        features: Features,
        pids: T,
    ) -> Result<Self, Error> {
        let mut system = System {
            last_update: Instant::now(),
            last_duration: Duration::ZERO,
            features,
            sysinfo_system: None,
            refresh_kind: sysinfo::RefreshKind::default().with_cpu(CpuRefreshKind::everything()),
            #[cfg(target_os = "macos")]
            command_source: None,
            #[cfg(target_os = "macos")]
            ioreg: None,
            #[cfg(target_os = "macos")]
            smc: None,
            #[cfg(target_os = "windows")]
            pdh: None,
            #[cfg(target_os = "windows")]
            wmi_conn: None,
            #[cfg(target_os = "windows")]
            etw_trace: None,
            #[cfg(target_os = "windows")]
            vm_counter: None,
            #[cfg(target_os = "windows")]
            battery: None,
        };

        let mut use_sysinfo_system = false;
        if features.contains(Features::PROCESS) {
            system.refresh_kind = system
                .refresh_kind
                .with_processes(ProcessRefreshKind::everything());
            use_sysinfo_system = true;

            #[cfg(target_os = "windows")]
            {
                system.vm_counter = Some(VmCounter::new(pids.clone())?);
            }
        }
        if features.contains(Features::SMC) | features.contains(Features::CPU_FREQUENCY) {
            use_sysinfo_system = true;
        }
        if use_sysinfo_system {
            let mut sysinfo_system = sysinfo::System::new_with_specifics(system.refresh_kind);
            sysinfo_system.refresh_specifics(system.refresh_kind);
            system.sysinfo_system = Some(sysinfo_system);
        }

        if features.contains(Features::GPU) {
            #[cfg(target_os = "macos")]
            {
                system.ioreg = Some(IOKitRegistry::new(features.contains(Features::GPU)));
            }
            #[cfg(target_os = "windows")]
            {
                system.pdh = Some(Pdh::new()?);
            }
        }

        if features.contains(Features::CPU_FREQUENCY) {
            #[cfg(target_os = "macos")]
            {
                system.command_source = Some(CommandSource::new(
                    pids.clone(),
                    features.contains(Features::NET_TRAFFIC),
                    features.contains(Features::FPS),
                    features.contains(Features::K_OBJECT),
                ));
            }
            #[cfg(target_os = "windows")]
            {
                system.wmi_conn = Some(wmi::WMIConnection::new(
                    platform::windows::get_com_lib().ok_or(Error::ComLib)?,
                )?);
            }
        }

        if features.contains(Features::FPS) {
            #[cfg(target_os = "macos")]
            {
                system.command_source = Some(system.command_source.unwrap_or_else(|| {
                    CommandSource::new(
                        pids.clone(),
                        features.contains(Features::NET_TRAFFIC),
                        features.contains(Features::FPS),
                        features.contains(Features::K_OBJECT),
                    )
                }));
            }
            #[cfg(target_os = "windows")]
            {
                system.etw_trace = Some(EtwTrace::new(
                    true,
                    features.contains(Features::NET_TRAFFIC),
                )?);
            }
        }

        if features.contains(Features::SMC) {
            #[cfg(target_os = "macos")]
            {
                system.smc = Some(smc::SMC::new()?);
            }
            #[cfg(target_os = "windows")]
            {
                if system.wmi_conn.is_none() {
                    system.wmi_conn = Some(wmi::WMIConnection::new(
                        platform::windows::get_com_lib().ok_or(Error::ComLib)?,
                    )?);
                }

                system.battery = platform::windows::Battery::new()?;
            }
        }

        if features.contains(Features::NET_TRAFFIC) {
            #[cfg(target_os = "macos")]
            {
                system.command_source = Some(system.command_source.unwrap_or_else(|| {
                    CommandSource::new(
                        pids.clone(),
                        features.contains(Features::NET_TRAFFIC),
                        features.contains(Features::FPS),
                        features.contains(Features::K_OBJECT),
                    )
                }));
            }
            #[cfg(target_os = "windows")]
            {
                if system.etw_trace.is_none() {
                    system.etw_trace = Some(EtwTrace::new(false, true)?);
                }
            }
        }

        if features.contains(Features::K_OBJECT) {
            #[cfg(target_os = "macos")]
            {
                system.command_source = Some(system.command_source.unwrap_or_else(|| {
                    CommandSource::new(
                        pids,
                        features.contains(Features::NET_TRAFFIC),
                        features.contains(Features::FPS),
                        features.contains(Features::K_OBJECT),
                    )
                }));
            }
        }

        Ok(system)
    }

    pub fn update(&mut self, now: Instant) {
        self.last_duration = now - self.last_update;
        self.last_update = now;

        if let Some(sysinfo_system) = &mut self.sysinfo_system {
            sysinfo_system.refresh_specifics(self.refresh_kind);
        }

        #[cfg(target_os = "macos")]
        if let Some(command_source) = &mut self.command_source {
            if self.features.contains(Features::CPU_FREQUENCY) {
                command_source.update_power_metrics_data();
            }
            if self.features.contains(Features::NET_TRAFFIC)
                | self.features.contains(Features::FPS)
                | self.features.contains(Features::K_OBJECT)
            {
                command_source.update();
            }
        }

        #[cfg(target_os = "macos")]
        if let Some(ioreg) = &mut self.ioreg {
            ioreg.update();
        }

        #[cfg(target_os = "windows")]
        {
            if let Some(pdh) = &mut self.pdh {
                pdh.update();
            }
            if let Some(etw) = &mut self.etw_trace {
                etw.update();
            }
            if let Some(vm_counter) = &mut self.vm_counter {
                vm_counter.update();
            }
        }
    }

    pub fn sysinfo_system(&self) -> Option<&sysinfo::System> {
        self.sysinfo_system.as_ref()
    }

    pub fn process_cpu_usage(&self, pid: Pid) -> Option<f32> {
        Some(
            self.sysinfo_system
                .as_ref()?
                .process(sysinfo::Pid::from_u32(pid))?
                .cpu_usage(),
        )
    }

    pub fn process_mem(&mut self, pid: Pid) -> Option<usize> {
        #[cfg(target_os = "macos")]
        unsafe {
            let mut rusage_info_data: libc::rusage_info_v2 = std::mem::zeroed();
            let r = libc::proc_pid_rusage(
                pid as _,
                libc::RUSAGE_INFO_V2,
                std::mem::transmute(&mut rusage_info_data),
            );
            if r == libc::KERN_SUCCESS {
                Some(rusage_info_data.ri_phys_footprint as usize)
            } else {
                None
            }
        }

        #[cfg(target_os = "windows")]
        {
            let r = self.vm_counter.as_mut()?.process_mem(pid);
            match r {
                Some(0) => {}
                _ => return r,
            }

            Some(
                self.sysinfo_system
                    .as_ref()?
                    .process(sysinfo::Pid::from_u32(pid))?
                    .memory() as _,
            )
        }

        #[cfg(target_os = "linux")]
        {
            let mem = self
                .sysinfo_system
                .as_ref()?
                .process(sysinfo::Pid::from_u32(pid))?
                .memory();
            Some(mem as usize)
        }
    }

    pub fn process_alloc(&mut self, pid: Pid) -> Option<usize> {
        #[cfg(target_os = "macos")]
        {
            let _ = pid;
            None
        }

        #[cfg(target_os = "windows")]
        {
            self.vm_counter.as_mut()?.process_alloc(pid)
        }

        #[cfg(target_os = "linux")]
        {
            let _ = pid;
            None
        }
    }

    pub fn process_kobject(&mut self, pid: Pid) -> Option<u32> {
        #[cfg(target_os = "macos")]
        {
            let mach_ports = self
                .command_source
                .as_ref()
                .map(|c| c.process_mach_ports(pid))
                .flatten()
                .unwrap_or_default();
            let fds = platform::macos::proc_fds(pid).unwrap_or_default();
            Some(mach_ports + fds)
        }

        #[cfg(target_os = "windows")]
        {
            self.vm_counter.as_mut()?.process_handles(pid)
        }

        #[cfg(target_os = "linux")]
        {
            let _ = pid;
            None
        }
    }

    pub fn process_threads(&self, pid: Pid) -> Option<u32> {
        platform::threads_count(pid)
    }

    pub fn process_disk_read(&self, pid: Pid) -> Option<f32> {
        #[cfg(target_os = "macos")]
        {
            let read_bytes = self
                .sysinfo_system
                .as_ref()?
                .process(sysinfo::Pid::from_u32(pid))?
                .disk_usage()
                .read_bytes;
            Some(read_bytes as f32 / self.last_duration.as_secs_f32())
        }

        #[cfg(target_os = "windows")]
        {
            // TODO
            let _ = pid;
            None
        }

        #[cfg(target_os = "linux")]
        {
            let _ = pid;
            None
        }
    }

    pub fn process_disk_write(&self, pid: Pid) -> Option<f32> {
        #[cfg(target_os = "macos")]
        {
            let written_bytes = self
                .sysinfo_system
                .as_ref()?
                .process(sysinfo::Pid::from_u32(pid))?
                .disk_usage()
                .written_bytes;
            Some(written_bytes as f32 / self.last_duration.as_secs_f32())
        }

        #[cfg(target_os = "windows")]
        {
            // TODO
            let _ = pid;
            None
        }

        #[cfg(target_os = "linux")]
        {
            let _ = pid;
            None
        }
    }

    pub fn process_name(&self, pid: Pid) -> Option<&str> {
        Some(
            self.sysinfo_system
                .as_ref()?
                .process(sysinfo::Pid::from_u32(pid))?
                .name(),
        )
    }

    pub fn process_command(&self, pid: Pid) -> Option<&[String]> {
        Some(
            self.sysinfo_system
                .as_ref()?
                .process(sysinfo::Pid::from_u32(pid))?
                .cmd(),
        )
    }

    pub fn process_responsible(&self, pid: Pid) -> Option<Pid> {
        #[cfg(target_os = "macos")]
        {
            let pid_responsible = (get_pid_responsible()?)(pid as _);
            if pid_responsible < 0 {
                None
            } else {
                Some(pid_responsible as _)
            }
        }

        #[cfg(target_os = "windows")]
        {
            let _ = pid;
            None
        }

        #[cfg(target_os = "linux")]
        {
            let _ = pid;
            None
        }
    }

    pub fn system_cpu_frequency(&self) -> Result<Vec<f32>, Error> {
        #[cfg(target_os = "macos")]
        {
            Ok(self
                .command_source
                .as_ref()
                .ok_or(Error::FeatureMissing(Features::CPU_FREQUENCY))?
                .cpu_frequency())
        }
        #[cfg(target_os = "windows")]
        {
            let processor_info: Vec<ProcessorInfo> = self.wmi_conn.as_ref().ok_or(Error::FeatureMissing(Features::CPU_FREQUENCY))?.raw_query("SELECT Name, PercentProcessorPerformance, ProcessorFrequency FROM Win32_PerfFormattedData_Counters_ProcessorInformation WHERE NOT Name LIKE '%_Total\'
")?;
            Ok(processor_info
                .into_iter()
                .map(|p| p.processor_frequency * p.percent_processor_performance / 100.0)
                .collect())
        }

        #[cfg(target_os = "linux")]
        {
            Ok(self
                .sysinfo_system
                .as_ref()
                .ok_or(Error::FeatureMissing(Features::CPU_FREQUENCY))?
                .cpus()
                .into_iter()
                .map(|cpu| cpu.frequency() as f32)
                .collect())
        }
    }

    pub fn process_gpu_usage(&mut self, pid: Pid, calc: GpuCalculation) -> Option<f32> {
        #[cfg(target_os = "macos")]
        {
            let _ = (pid, calc);
            Some(0.0)
        }

        #[cfg(target_os = "windows")]
        {
            self.pdh.as_mut().unwrap().poll_gpu_usage(
                platform::windows::GpuCounterType::Utilization,
                Some(pid),
                calc,
            )
        }

        #[cfg(target_os = "linux")]
        {
            let _ = (pid, calc);
            None
        }
    }

    pub fn process_vram(&mut self, pid: Pid, calc: GpuCalculation) -> Option<f32> {
        #[cfg(target_os = "macos")]
        {
            let _ = (pid, calc);
            None
        }

        #[cfg(target_os = "windows")]
        {
            self.pdh.as_mut().unwrap().poll_gpu_usage(
                platform::windows::GpuCounterType::VRam,
                Some(pid),
                calc,
            )
        }

        #[cfg(target_os = "linux")]
        {
            let _ = (pid, calc);
            None
        }
    }

    pub fn process_fps(&mut self, pid: Pid) -> f32 {
        #[cfg(target_os = "macos")]
        {
            self.command_source
                .as_ref()
                .unwrap()
                .process_frame_per_sec(pid)
                .unwrap_or(0.0)
        }

        #[cfg(target_os = "windows")]
        {
            self.etw_trace.as_mut().unwrap().fps(pid)
        }

        #[cfg(target_os = "linux")]
        {
            let _ = pid;
            0.
        }
    }

    pub fn process_net_traffic_in(&self, pid: Pid) -> Option<u32> {
        #[cfg(target_os = "macos")]
        {
            self.command_source.as_ref()?.process_net_traffic_in(pid)
        }
        #[cfg(target_os = "windows")]
        {
            Some(self.etw_trace.as_ref()?.net_recv_per_sec(pid))
        }

        #[cfg(target_os = "linux")]
        {
            let _ = pid;
            None
        }
    }

    pub fn process_net_traffic_out(&self, pid: Pid) -> Option<u32> {
        #[cfg(target_os = "macos")]
        {
            self.command_source.as_ref()?.process_net_traffic_out(pid)
        }
        #[cfg(target_os = "windows")]
        {
            Some(self.etw_trace.as_ref()?.net_send_per_sec(pid))
        }

        #[cfg(target_os = "linux")]
        {
            let _ = pid;
            None
        }
    }

    pub fn system_cpu_usage(&self) -> Result<Vec<f32>, Error> {
        let sysinfo_system = self
            .sysinfo_system
            .as_ref()
            .ok_or(Error::FeatureMissing(Features::PROCESS))?;
        Ok(sysinfo_system
            .cpus()
            .into_iter()
            .map(|cpu| cpu.cpu_usage())
            .collect())
    }

    pub fn system_gpu_usage(&mut self, calc: GpuCalculation) -> Option<f32> {
        #[cfg(target_os = "macos")]
        {
            let _ = calc;
            Some(self.ioreg.as_ref().unwrap().sys_gpu_usage())
        }

        #[cfg(target_os = "windows")]
        {
            self.pdh.as_mut().unwrap().poll_gpu_usage(
                platform::windows::GpuCounterType::Utilization,
                None,
                calc,
            )
        }

        #[cfg(target_os = "linux")]
        {
            let _ = calc;
            None
        }
    }

    pub fn system_cpu_temperature(&mut self) -> Result<Vec<f32>, Error> {
        #[cfg(target_os = "macos")]
        {
            let smc = self
                .smc
                .as_ref()
                .ok_or(Error::FeatureMissing(Features::SMC))?;
            let mut cpus_temp = vec![];

            // From https://github.com/exelban/stats
            for i in [
                // Intel
                "TC0C", "TC1C", "TC2C", "TC3C", "TC4C", "TC5C", "TC6C", "TC7C", "TC8C", "TC9C",
                // Apple Silicon
                "Tp09", "Tp0T", "Tp01", "Tp05", "Tp0D", "Tp0H", "Tp0L", "Tp0P", "Tp0X", "Tp0b",
                // M2
                "Tp0j", "Tp0r", "Tp0f", "Tp0n",
            ] {
                match smc.temperature(i.into()) {
                    Ok(t) if t > 0. => cpus_temp.push(t as f32),
                    _ => {}
                }
            }

            Ok(cpus_temp)
        }
        #[cfg(target_os = "windows")]
        {
            Err(Error::UnsupportedFeatures(Features::SMC))
        }

        #[cfg(target_os = "linux")]
        {
            Err(Error::UnsupportedFeatures(Features::SMC))
        }
    }

    pub fn system_power(&self) -> Result<f32, Error> {
        #[cfg(target_os = "macos")]
        {
            let smc = self
                .smc
                .as_ref()
                .ok_or(Error::FeatureMissing(Features::SMC))?;
            Ok(smc.read_key::<f32>("PSTR".into())?)
        }
        #[cfg(target_os = "windows")]
        {
            let battery = self
                .battery
                .as_ref()
                .ok_or(Error::UnsupportedFeatures(Features::SMC))?;
            battery.rate()
        }

        #[cfg(target_os = "linux")]
        {
            Err(Error::UnsupportedFeatures(Features::SMC))
        }
    }

    pub fn system_npu_power(&self) -> Result<f32, Error> {
        #[cfg(target_os = "macos")]
        {
            Ok(self
                .command_source
                .as_ref()
                .ok_or(Error::FeatureMissing(Features::CPU_FREQUENCY))?
                .ane_power()
                / 1000.)
        }
        #[cfg(not(target_os = "macos"))]
        Err(Error::UnsupportedFeatures(Features::CPU_FREQUENCY))
    }
}

bitflags! {
    #[derive(Default)]
    pub struct Features: u32 {
        const PROCESS =         1 << 0;
        const GPU =             1 << 1;
        const CPU_FREQUENCY =   1 << 2;
        const FPS =             1 << 3;
        const SMC =             1 << 4;
        const NET_TRAFFIC =     1 << 5;
        const K_OBJECT =        1 << 6;
    }
}
