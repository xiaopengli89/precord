#[cfg(target_os = "macos")]
use crate::platform::macos::{CommandSource, IOKitRegistry};
#[cfg(target_os = "windows")]
use crate::platform::windows::{EtwTrace, Pdh, ProcessorInfo, ThermalZoneInformation, VmCounter};
use crate::{Error, Pid};
use bitflags::bitflags;
use std::fmt::{self, Display, Formatter};
use sysinfo::{ProcessExt, SystemExt};

#[derive(Default)]
pub struct System {
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
}

impl System {
    #[allow(unused_variables)]
    pub fn new<T: IntoIterator<Item = Pid> + Clone>(
        features: Features,
        pids: T,
    ) -> Result<Self, Error> {
        let mut system = System::default();
        system.features = features;

        let mut use_sysinfo_system = false;
        if features.contains(Features::PROCESS) {
            system.refresh_kind = system.refresh_kind.with_processes();
            use_sysinfo_system = true;

            #[cfg(target_os = "windows")]
            {
                system.vm_counter = Some(VmCounter::new(pids.clone())?);
            }
        }
        if features.contains(Features::SMC) {
            system.refresh_kind = system.refresh_kind.with_cpu();
            use_sysinfo_system = true;
        }
        if use_sysinfo_system {
            system.sysinfo_system = Some(sysinfo::System::new_with_specifics(system.refresh_kind));
        }

        if features.contains(Features::GPU) {
            #[cfg(target_os = "macos")]
            {
                system.ioreg = Some(IOKitRegistry::new());
            }
            #[cfg(target_os = "windows")]
            {
                system.pdh = Some(Pdh::new(pids)?);
            }
        }

        if features.contains(Features::CPU_FREQUENCY) {
            #[cfg(target_os = "macos")]
            {
                system.command_source = Some(CommandSource::new(pids.clone()));
            }
            #[cfg(target_os = "windows")]
            {
                system.wmi_conn = Some(wmi::WMIConnection::new(wmi::COMLibrary::new()?.into())?);
            }
        }

        if features.contains(Features::FPS) {
            #[cfg(target_os = "windows")]
            {
                system.etw_trace = Some(EtwTrace::new());
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
                    system.wmi_conn =
                        Some(wmi::WMIConnection::new(wmi::COMLibrary::new()?.into())?);
                }
            }
        }

        if features.contains(Features::NET_TRAFFIC) {
            #[cfg(target_os = "macos")]
            {
                system.command_source = Some(
                    system
                        .command_source
                        .unwrap_or_else(|| CommandSource::new(pids)),
                );
            }
        }

        Ok(system)
    }

    pub fn update(&mut self) {
        if let Some(sysinfo_system) = &mut self.sysinfo_system {
            sysinfo_system.refresh_specifics(self.refresh_kind);
        }

        #[cfg(target_os = "macos")]
        if let Some(command_source) = &mut self.command_source {
            if self.features.contains(Features::CPU_FREQUENCY) {
                command_source.update_cpu_frequency();
            }
            if self.features.contains(Features::NET_TRAFFIC) {
                command_source.update_net_traffic();
            }
        }

        #[cfg(target_os = "macos")]
        if let Some(ioreg) = &mut self.ioreg {
            ioreg.poll();
        }

        #[cfg(target_os = "windows")]
        if let Some(pdh) = &mut self.pdh {
            pdh.update();
        }
    }

    pub fn sysinfo_system(&self) -> Option<&sysinfo::System> {
        self.sysinfo_system.as_ref()
    }

    pub fn process_cpu_usage(&self, pid: Pid) -> Option<f32> {
        Some(self.sysinfo_system.as_ref()?.process(pid)?.cpu_usage())
    }

    pub fn process_mem(&mut self, pid: Pid) -> Option<f32> {
        #[cfg(target_os = "macos")]
        unsafe {
            let mut rusage_info_data: libc::rusage_info_v2 =
                std::mem::MaybeUninit::uninit().assume_init();
            let r = libc::proc_pid_rusage(
                pid,
                libc::RUSAGE_INFO_V2,
                std::mem::transmute(&mut rusage_info_data),
            );
            if r == libc::KERN_SUCCESS {
                let mem = (rusage_info_data.ri_phys_footprint / 1024) as f32;
                Some(mem)
            } else {
                None
            }
        }

        #[cfg(target_os = "windows")]
        {
            self.vm_counter.as_mut()?.process_mem(pid)
        }
    }

    pub fn process_name(&self, pid: Pid) -> Option<&str> {
        Some(self.sysinfo_system.as_ref()?.process(pid)?.name())
    }

    pub fn process_command(&self, pid: Pid) -> Option<&[String]> {
        Some(self.sysinfo_system.as_ref()?.process(pid)?.cmd())
    }

    pub fn cpus_frequency(&self) -> Result<Vec<f32>, Error> {
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
    }

    #[allow(unused_variables)]
    pub fn process_gpu_usage(&mut self, pid: Pid) -> Option<f32> {
        #[cfg(target_os = "macos")]
        {
            Some(0.0)
        }

        #[cfg(target_os = "windows")]
        {
            self.pdh.as_mut().unwrap().poll_gpu_usage(Some(pid))
        }
    }

    #[allow(unused_variables)]
    pub fn process_fps(&mut self, pid: Pid) -> f32 {
        #[cfg(target_os = "macos")]
        {
            0.0
        }

        #[cfg(target_os = "windows")]
        {
            self.etw_trace.as_mut().unwrap().fps(pid)
        }
    }

    pub fn process_net_traffic_in(&self, pid: Pid) -> Option<f32> {
        #[cfg(target_os = "macos")]
        {
            self.command_source.as_ref()?.process_net_traffic_in(pid)
        }
        #[cfg(target_os = "windows")]
        {
            None
        }
    }

    pub fn process_net_traffic_out(&self, pid: Pid) -> Option<f32> {
        #[cfg(target_os = "macos")]
        {
            self.command_source.as_ref()?.process_net_traffic_out(pid)
        }
        #[cfg(target_os = "windows")]
        {
            None
        }
    }

    pub fn system_gpu_usage(&mut self) -> Option<f32> {
        #[cfg(target_os = "macos")]
        {
            Some(self.ioreg.as_ref().unwrap().sys_gpu_usage())
        }

        #[cfg(target_os = "windows")]
        {
            self.pdh.as_mut().unwrap().poll_gpu_usage(None)
        }
    }

    pub fn cpus_temperature(&mut self) -> Result<Vec<f32>, Error> {
        #[cfg(target_os = "macos")]
        {
            let sysinfo_system = self
                .sysinfo_system
                .as_ref()
                .ok_or(Error::FeatureMissing(Features::SMC))?;
            let smc = self
                .smc
                .as_ref()
                .ok_or(Error::FeatureMissing(Features::SMC))?;
            let mut cpus_temp = vec![];

            for i in 0..sysinfo_system
                .physical_core_count()
                .ok_or(Error::PhysicalCoreCount)?
                + 1
            {
                match smc.cpu_temperature(i as _) {
                    Ok(t) => cpus_temp.push(t as f32),
                    Err(_) => {}
                }
            }

            Ok(cpus_temp)
        }
        #[cfg(target_os = "windows")]
        {
            let wmi_conn = self
                .wmi_conn
                .as_ref()
                .ok_or(Error::FeatureMissing(Features::SMC))?;
            let thermal_zone_info: Vec<ThermalZoneInformation> = wmi_conn.raw_query(
                "Select Temperature From Win32_PerfFormattedData_Counters_ThermalZoneInformation",
            )?;
            Ok(thermal_zone_info
                .into_iter()
                .map(|p| p.temperature - 273.15)
                .collect())
        }
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
    }
}

impl Display for Features {
    #[allow(unused_assignments)]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut first = true;

        if self.contains(Features::PROCESS) {
            if !first {
                write!(f, "|")?;
            } else {
                first = false;
            }
            write!(f, "{}", "PROCESS")?;
        }
        if self.contains(Features::GPU) {
            if !first {
                write!(f, "|")?;
            } else {
                first = false;
            }
            write!(f, "{}", "GPU")?;
        }
        if self.contains(Features::CPU_FREQUENCY) {
            if !first {
                write!(f, "|")?;
            } else {
                first = false;
            }
            write!(f, "{}", "CPU_FREQUENCY")?;
        }
        if self.contains(Features::FPS) {
            if !first {
                write!(f, "|")?;
            } else {
                first = false;
            }
            write!(f, "{}", "FPS")?;
        }
        if self.contains(Features::SMC) {
            if !first {
                write!(f, "|")?;
            } else {
                first = false;
            }
            write!(f, "{}", "SMC")?;
        }

        Ok(())
    }
}
