use crate::{Error, Pid};
use ntapi::winapi::um::winnt;
use std::os::windows::prelude::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::time::Duration;
use std::{mem, ptr, thread};
use windows::core::PCWSTR;
use windows::Win32::Devices::DeviceAndDriverInstallation;
use windows::Win32::Foundation;
use windows::Win32::Storage::FileSystem;
use windows::Win32::System::Diagnostics::ToolHelp;
use windows::Win32::System::{Memory, Power, Threading, IO};

fn threads(pid: Pid) -> Vec<u32> {
    unsafe {
        let snapshot = if let Ok(snapshot) =
            ToolHelp::CreateToolhelp32Snapshot(ToolHelp::TH32CS_SNAPTHREAD, 0)
        {
            OwnedHandle::from_raw_handle(snapshot.0 as _)
        } else {
            return vec![];
        };

        let mut entry: ToolHelp::THREADENTRY32 = mem::zeroed();
        entry.dwSize = mem::size_of::<ToolHelp::THREADENTRY32>() as _;
        if !ToolHelp::Thread32First(
            super::windows_raw_handle(snapshot.as_raw_handle()),
            &mut entry,
        )
        .as_bool()
        {
            return vec![];
        }

        let mut threads = vec![];

        if entry.th32OwnerProcessID == pid {
            threads.push(entry.th32ThreadID);
        }

        entry.dwSize = mem::size_of::<ToolHelp::THREADENTRY32>() as _;
        while ToolHelp::Thread32Next(
            super::windows_raw_handle(snapshot.as_raw_handle()),
            &mut entry,
        )
        .as_bool()
        {
            if entry.th32OwnerProcessID == pid {
                threads.push(entry.th32ThreadID);
            }
            entry.dwSize = mem::size_of::<ToolHelp::THREADENTRY32>() as _;
        }

        threads
    }
}

pub struct ThreadInfo {
    id: u32,
    handle: OwnedHandle,
    cpu_usage: f32,
    last_cpu_times: u64,
    last_global_cpu_times: u64,
}

impl ThreadInfo {
    fn new(tid: u32) -> Result<Self, Error> {
        unsafe {
            let raw_handle = Threading::OpenThread(Threading::THREAD_QUERY_INFORMATION, false, tid)
                .map_err(|err| {
                    if Foundation::GetLastError() == Foundation::ERROR_ACCESS_DENIED {
                        Error::AccessDenied
                    } else {
                        Error::WinError(err)
                    }
                })?;
            let handle = OwnedHandle::from_raw_handle(raw_handle.0 as _);

            let mut ignore = mem::zeroed();
            let mut fsys = mem::zeroed();
            let mut fuser = mem::zeroed();
            let mut g_fsys = mem::zeroed();
            let mut g_fuser = mem::zeroed();

            Threading::GetThreadTimes(raw_handle, &mut ignore, &mut ignore, &mut fsys, &mut fuser)
                .ok()?;
            Threading::GetSystemTimes(None, Some(&mut g_fsys), Some(&mut g_fuser)).ok()?;

            let mut sys: winnt::ULARGE_INTEGER = mem::zeroed();
            ptr::copy(&fsys, &mut sys as *mut winnt::ULARGE_INTEGER as *mut _, 1);
            let mut user: winnt::ULARGE_INTEGER = mem::zeroed();
            ptr::copy(&fuser, &mut user as *mut winnt::ULARGE_INTEGER as *mut _, 1);

            let mut g_sys: winnt::ULARGE_INTEGER = mem::zeroed();
            ptr::copy(
                &g_fsys,
                &mut g_sys as *mut winnt::ULARGE_INTEGER as *mut _,
                1,
            );
            let mut g_user: winnt::ULARGE_INTEGER = mem::zeroed();
            ptr::copy(
                &g_fuser,
                &mut g_user as *mut winnt::ULARGE_INTEGER as *mut _,
                1,
            );

            Ok(Self {
                id: tid,
                handle,
                cpu_usage: 0.,
                last_cpu_times: *sys.QuadPart() + *user.QuadPart(),
                last_global_cpu_times: *g_sys.QuadPart() + *g_user.QuadPart(),
            })
        }
    }

    pub fn id(&self) -> u64 {
        self.id as _
    }

    pub fn cpu_usage(&self) -> f32 {
        self.cpu_usage
    }

    fn refresh_cpu_usage(&mut self, nb_cpus: u32) {
        unsafe {
            let mut ignore = mem::zeroed();
            let mut fsys = mem::zeroed();
            let mut fuser = mem::zeroed();
            let mut g_fsys = mem::zeroed();
            let mut g_fuser = mem::zeroed();

            Threading::GetThreadTimes(
                super::windows_raw_handle(self.handle.as_raw_handle()),
                &mut ignore,
                &mut ignore,
                &mut fsys,
                &mut fuser,
            )
            .ok()
            .unwrap();
            Threading::GetSystemTimes(None, Some(&mut g_fsys), Some(&mut g_fuser))
                .ok()
                .unwrap();

            let mut sys: winnt::ULARGE_INTEGER = mem::zeroed();
            ptr::copy(&fsys, &mut sys as *mut winnt::ULARGE_INTEGER as *mut _, 1);
            let mut user: winnt::ULARGE_INTEGER = mem::zeroed();
            ptr::copy(&fuser, &mut user as *mut winnt::ULARGE_INTEGER as *mut _, 1);

            let mut g_sys: winnt::ULARGE_INTEGER = mem::zeroed();
            ptr::copy(
                &g_fsys,
                &mut g_sys as *mut winnt::ULARGE_INTEGER as *mut _,
                1,
            );
            let mut g_user: winnt::ULARGE_INTEGER = mem::zeroed();
            ptr::copy(
                &g_fuser,
                &mut g_user as *mut winnt::ULARGE_INTEGER as *mut _,
                1,
            );

            let cpu_times = *sys.QuadPart() + *user.QuadPart();
            let global_cpu_times = *g_sys.QuadPart() + *g_user.QuadPart();

            let delta = cpu_times - self.last_cpu_times;
            let g_delta = global_cpu_times - self.last_global_cpu_times;

            self.cpu_usage = 100. * delta as f32 / g_delta as f32 * nb_cpus as f32;
            self.last_cpu_times = cpu_times;
            self.last_global_cpu_times = global_cpu_times;
        }
    }
}

pub fn threads_info(pid: Pid, nb_cpus: u32) -> Result<Vec<ThreadInfo>, Error> {
    let mut threads_info = vec![];
    for tid in threads(pid) {
        threads_info.push(ThreadInfo::new(tid)?);
    }

    thread::sleep(Duration::from_secs(1));

    for t in threads_info.iter_mut() {
        t.refresh_cpu_usage(nb_cpus);
    }
    Ok(threads_info)
}

pub fn _system_power() -> Result<f32, Error> {
    let rate = unsafe {
        let mut state: Power::SYSTEM_BATTERY_STATE = mem::zeroed();
        Power::CallNtPowerInformation(
            Power::SystemBatteryState,
            None,
            0,
            Some((&mut state) as *mut Power::SYSTEM_BATTERY_STATE as _),
            mem::size_of::<Power::SYSTEM_BATTERY_STATE>() as _,
        )?;
        state.Rate as i32
    };

    Ok(-rate.min(0) as f32 / 1000.)
}

pub fn system_power() -> Result<f32, Error> {
    let mut rate = 0.;

    unsafe {
        let hdev = DeviceAndDriverInstallation::SetupDiGetClassDevsW(
            Some(&DeviceAndDriverInstallation::GUID_DEVCLASS_BATTERY),
            None,
            None,
            DeviceAndDriverInstallation::DIGCF_PRESENT
                | DeviceAndDriverInstallation::DIGCF_DEVICEINTERFACE,
        )?;

        if hdev.is_invalid() {
            return Ok(0.);
        }

        let mut did: DeviceAndDriverInstallation::SP_DEVICE_INTERFACE_DATA = mem::zeroed();
        did.cbSize = mem::size_of_val(&did) as _;
        if DeviceAndDriverInstallation::SetupDiEnumDeviceInterfaces(
            hdev,
            None,
            &DeviceAndDriverInstallation::GUID_DEVCLASS_BATTERY,
            0,
            &mut did,
        )
        .as_bool()
        {
            let mut cb_required = 0;
            let mut r = DeviceAndDriverInstallation::SetupDiGetDeviceInterfaceDetailW(
                hdev,
                &did,
                None,
                0,
                Some(&mut cb_required),
                None,
            )
            .as_bool();
            assert!(!r);

            let p = Memory::LocalAlloc(Memory::LPTR, cb_required as _);
            assert!(p > 0);

            let pdidd = &mut *(p as usize
                as *mut DeviceAndDriverInstallation::SP_DEVICE_INTERFACE_DETAIL_DATA_W);
            pdidd.cbSize = mem::size_of_val(pdidd) as _;
            r = DeviceAndDriverInstallation::SetupDiGetDeviceInterfaceDetailW(
                hdev,
                &did,
                Some(pdidd),
                cb_required,
                Some(&mut cb_required),
                None,
            )
            .as_bool();
            assert!(r);

            let h_battery = FileSystem::CreateFileW(
                PCWSTR::from_raw(pdidd.DevicePath.as_ptr()),
                FileSystem::FILE_GENERIC_READ | FileSystem::FILE_GENERIC_WRITE,
                FileSystem::FILE_SHARE_READ | FileSystem::FILE_SHARE_WRITE,
                None,
                FileSystem::OPEN_EXISTING,
                FileSystem::FILE_ATTRIBUTE_NORMAL,
                None,
            )
            .unwrap();
            let h_battery = OwnedHandle::from_raw_handle(h_battery.0 as _);

            let mut bqi: Power::BATTERY_QUERY_INFORMATION = mem::zeroed();
            let mut dw_wait = 0;
            let mut dw_out = 0;

            r = IO::DeviceIoControl(
                super::windows_raw_handle(h_battery.as_raw_handle()),
                Power::IOCTL_BATTERY_QUERY_TAG,
                Some(&dw_wait as *const i32 as _),
                mem::size_of_val(&dw_wait) as _,
                Some(&mut bqi.BatteryTag as *mut u32 as _),
                mem::size_of::<u32>() as _,
                Some(&mut dw_out),
                None,
            )
            .as_bool();
            assert!(r && bqi.BatteryTag > 0);

            let mut bws: Power::BATTERY_WAIT_STATUS = mem::zeroed();
            bws.BatteryTag = bqi.BatteryTag;
            let mut bs: Power::BATTERY_STATUS = mem::zeroed();
            r = IO::DeviceIoControl(
                super::windows_raw_handle(h_battery.as_raw_handle()),
                Power::IOCTL_BATTERY_QUERY_STATUS,
                Some(&bws as *const Power::BATTERY_WAIT_STATUS as _),
                mem::size_of_val(&bws) as _,
                Some(&mut bs as *mut Power::BATTERY_STATUS as _),
                mem::size_of::<Power::BATTERY_STATUS>() as _,
                Some(&mut dw_out),
                None,
            )
            .as_bool();
            assert!(r);

            rate = -bs.Rate.min(0) as f32 / 1000.;

            Memory::LocalFree(p);
        }

        DeviceAndDriverInstallation::SetupDiDestroyDeviceInfoList(hdev);
    }

    Ok(rate)
}
