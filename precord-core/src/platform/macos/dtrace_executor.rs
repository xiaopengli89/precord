use super::dtrace;
use crate::Error;
use std::ffi::{c_char, c_int, c_void};
use std::{mem, ptr, slice};

pub struct Dtrace {
    dh: *mut dtrace::dtrace_hdl_t,
    started: bool,
}

impl Drop for Dtrace {
    fn drop(&mut self) {
        unsafe {
            if self.started {
                dtrace::dtrace_stop(self.dh);
            }
            dtrace::dtrace_close(self.dh);
        }
    }
}

unsafe impl Send for Dtrace {}

impl Dtrace {
    pub fn new(script: *const c_char) -> Result<Self, Error> {
        unsafe {
            let mut err = 0;
            let dh = dtrace::dtrace_open(dtrace::DTRACE_VERSION as _, 0, &mut err);
            if dh.is_null() {
                return Err(Error::Dtrace);
            }

            let mut dt = Self { dh, started: false };

            dtrace::dtrace_setopt(dt.dh, "strsize\0".as_ptr() as _, "4096\0".as_ptr() as _);
            dtrace::dtrace_setopt(dt.dh, "bufsize\0".as_ptr() as _, "4m\0".as_ptr() as _);

            let prog = dtrace::dtrace_program_strcompile(
                dt.dh,
                script,
                dtrace::dtrace_probespec_DTRACE_PROBESPEC_NAME,
                dtrace::DTRACE_C_ZDEFS,
                0,
                ptr::null_mut(),
            );
            if prog.is_null() {
                return Err(Error::Dtrace);
            }

            let mut info: dtrace::dtrace_proginfo_t = mem::zeroed();
            let mut r = dtrace::dtrace_program_exec(dt.dh, prog, &mut info);
            if r < 0 {
                return Err(Error::Dtrace);
            }

            r = dtrace::dtrace_go(dt.dh);
            if r < 0 {
                return Err(Error::Dtrace);
            }

            dt.started = true;
            Ok(dt)
        }
    }

    pub fn run(self, mut cb: impl FnMut(&str) -> bool) {
        unsafe {
            let mut buf = ptr::null_mut();
            let mut size = 0;
            let f = libc::open_memstream(&mut buf, &mut size);

            loop {
                libc::fseeko(f, 0, libc::SEEK_SET);
                dtrace::dtrace_sleep(self.dh);

                match dtrace::dtrace_work(
                    self.dh,
                    f as _,
                    Some(chew),
                    Some(chewrec),
                    ptr::null_mut(),
                ) {
                    dtrace::dtrace_workstatus_t_DTRACE_WORKSTATUS_DONE => {
                        break;
                    }
                    dtrace::dtrace_workstatus_t_DTRACE_WORKSTATUS_OKAY => {
                        libc::fflush(f);

                        if let Ok(s) =
                            std::str::from_utf8(slice::from_raw_parts(buf as *const u8, size))
                        {
                            if !cb(s) {
                                break;
                            }
                        }
                    }
                    dtrace::dtrace_workstatus_t_DTRACE_WORKSTATUS_ERROR => {
                        break;
                    }
                    _ => {
                        break;
                    }
                }
            }

            libc::fclose(f);
            libc::free(buf as _);
        }
    }
}

unsafe extern "C" fn chew(_data: *const dtrace::dtrace_probedata_t, _arg: *mut c_void) -> c_int {
    dtrace::DTRACE_CONSUME_THIS as _
}

unsafe extern "C" fn chewrec(
    _data: *const dtrace::dtrace_probedata_t,
    rec: *const dtrace::dtrace_recdesc_t,
    _arg: *mut c_void,
) -> c_int {
    if rec.is_null() {
        dtrace::DTRACE_CONSUME_NEXT as _
    } else {
        dtrace::DTRACE_CONSUME_THIS as _
    }
}

#[link(name = "dtrace")]
extern "C" {}
