use crate::Error;
use std::mem;
use std::os::windows::prelude::{AsRawHandle, FromRawHandle, OwnedHandle};
use windows::core::PCWSTR;
use windows::Win32::Devices::DeviceAndDriverInstallation;
use windows::Win32::Storage::FileSystem;
use windows::Win32::System::{Memory, Power, IO};

pub struct Battery {
    handle: OwnedHandle,
    bws: Power::BATTERY_WAIT_STATUS,
    relative: bool,
}

impl Battery {
    pub fn new() -> Result<Self, Error> {
        unsafe {
            let hdev = DeviceAndDriverInstallation::SetupDiGetClassDevsW(
                Some(&DeviceAndDriverInstallation::GUID_DEVCLASS_BATTERY),
                None,
                None,
                DeviceAndDriverInstallation::DIGCF_PRESENT
                    | DeviceAndDriverInstallation::DIGCF_DEVICEINTERFACE,
            )?;

            if hdev.is_invalid() {
                return Err(Error::WinError(windows::core::Error::from_win32()));
            }
            let hdev = OwnedDeviceInfo(hdev);

            let mut did: DeviceAndDriverInstallation::SP_DEVICE_INTERFACE_DATA = mem::zeroed();
            did.cbSize = mem::size_of_val(&did) as _;
            if !DeviceAndDriverInstallation::SetupDiEnumDeviceInterfaces(
                hdev.0,
                None,
                &DeviceAndDriverInstallation::GUID_DEVCLASS_BATTERY,
                0,
                &mut did,
            )
            .as_bool()
            {
                return Err(Error::WinError(windows::core::Error::from_win32()));
            }

            let mut cb_required = 0;
            let r = DeviceAndDriverInstallation::SetupDiGetDeviceInterfaceDetailW(
                hdev.0,
                &did,
                None,
                0,
                Some(&mut cb_required),
                None,
            )
            .as_bool();
            assert!(!r);

            let local = Memory::LocalAlloc(Memory::LPTR, cb_required as _);
            assert!(local > 0);
            let local = OwnedLocalMemory(local);

            let pdidd = &mut *(local.0 as usize
                as *mut DeviceAndDriverInstallation::SP_DEVICE_INTERFACE_DETAIL_DATA_W);
            pdidd.cbSize = mem::size_of_val(pdidd) as _;
            if !DeviceAndDriverInstallation::SetupDiGetDeviceInterfaceDetailW(
                hdev.0,
                &did,
                Some(pdidd),
                cb_required,
                Some(&mut cb_required),
                None,
            )
            .as_bool()
            {
                return Err(Error::WinError(windows::core::Error::from_win32()));
            }

            let h_battery = FileSystem::CreateFileW(
                PCWSTR::from_raw(pdidd.DevicePath.as_ptr()),
                FileSystem::FILE_GENERIC_READ | FileSystem::FILE_GENERIC_WRITE,
                FileSystem::FILE_SHARE_READ | FileSystem::FILE_SHARE_WRITE,
                None,
                FileSystem::OPEN_EXISTING,
                FileSystem::FILE_ATTRIBUTE_NORMAL,
                None,
            )?;
            let h_battery = OwnedHandle::from_raw_handle(h_battery.0 as _);

            let mut bqi: Power::BATTERY_QUERY_INFORMATION = mem::zeroed();
            let dw_wait = 0;

            if !IO::DeviceIoControl(
                super::windows_raw_handle(h_battery.as_raw_handle()),
                Power::IOCTL_BATTERY_QUERY_TAG,
                Some(&dw_wait as *const i32 as _),
                mem::size_of_val(&dw_wait) as _,
                Some(&mut bqi.BatteryTag as *mut u32 as _),
                mem::size_of::<u32>() as _,
                None,
                None,
            )
            .as_bool()
            {
                return Err(Error::WinError(windows::core::Error::from_win32()));
            }
            assert!(bqi.BatteryTag > 0);

            // Battery information
            let mut bi: Power::BATTERY_INFORMATION = mem::zeroed();
            bqi.InformationLevel = Power::BatteryInformation;
            if !IO::DeviceIoControl(
                super::windows_raw_handle(h_battery.as_raw_handle()),
                Power::IOCTL_BATTERY_QUERY_INFORMATION,
                Some(&bqi as *const Power::BATTERY_QUERY_INFORMATION as _),
                mem::size_of_val(&bqi) as _,
                Some(&mut bi as *mut Power::BATTERY_INFORMATION as _),
                mem::size_of::<Power::BATTERY_INFORMATION>() as _,
                None,
                None,
            )
            .as_bool()
            {
                return Err(Error::WinError(windows::core::Error::from_win32()));
            }

            let mut bws: Power::BATTERY_WAIT_STATUS = mem::zeroed();
            bws.BatteryTag = bqi.BatteryTag;

            Ok(Self {
                handle: h_battery,
                bws,
                relative: bi.Capabilities & Power::BATTERY_CAPACITY_RELATIVE > 0,
            })
        }
    }

    pub fn rate(&self) -> Result<f32, Error> {
        let mut rate = 0.;

        if !self.relative {
            unsafe {
                let mut bs: Power::BATTERY_STATUS = mem::zeroed();
                if !IO::DeviceIoControl(
                    super::windows_raw_handle(self.handle.as_raw_handle()),
                    Power::IOCTL_BATTERY_QUERY_STATUS,
                    Some(&self.bws as *const Power::BATTERY_WAIT_STATUS as _),
                    mem::size_of_val(&self.bws) as _,
                    Some(&mut bs as *mut Power::BATTERY_STATUS as _),
                    mem::size_of::<Power::BATTERY_STATUS>() as _,
                    None,
                    None,
                )
                .as_bool()
                {
                    return Err(Error::WinError(windows::core::Error::from_win32()));
                }

                if bs.Rate != Power::BATTERY_UNKNOWN_RATE as i32 {
                    rate = -bs.Rate.min(0) as f32 / 1000.;
                }
            }
        }

        Ok(rate)
    }
}

struct OwnedDeviceInfo(DeviceAndDriverInstallation::HDEVINFO);

impl Drop for OwnedDeviceInfo {
    fn drop(&mut self) {
        unsafe {
            let r = DeviceAndDriverInstallation::SetupDiDestroyDeviceInfoList(self.0).as_bool();
            assert!(r);
        }
    }
}

struct OwnedLocalMemory(isize);

impl Drop for OwnedLocalMemory {
    fn drop(&mut self) {
        unsafe {
            assert_eq!(Memory::LocalFree(self.0), 0);
        }
    }
}
