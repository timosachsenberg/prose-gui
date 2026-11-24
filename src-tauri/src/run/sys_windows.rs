//! The tool's processes as a job object: every process the tool starts joins the job, which
//! `TerminateJobObject` ends at once, and which ends with the app (`KILL_ON_JOB_CLOSE`: the app's
//! handle is the only one, and Windows closes it when the app ends, however it ends).

use std::ffi::c_void;
use std::os::windows::io::AsRawHandle;
use std::process::{Command, Stdio};

type Handle = *mut c_void;

#[repr(C)]
#[derive(Default)]
struct BasicLimitInformation {
    per_process_user_time_limit: i64,
    per_job_user_time_limit: i64,
    limit_flags: u32,
    minimum_working_set_size: usize,
    maximum_working_set_size: usize,
    active_process_limit: u32,
    affinity: usize,
    priority_class: u32,
    scheduling_class: u32,
}

/// JOBOBJECT_EXTENDED_LIMIT_INFORMATION
#[repr(C)]
#[derive(Default)]
struct ExtendedLimitInformation {
    basic: BasicLimitInformation,
    io_info: [u64; 6],
    process_memory_limit: usize,
    job_memory_limit: usize,
    peak_process_memory_used: usize,
    peak_job_memory_used: usize,
}

const JOB_OBJECT_EXTENDED_LIMIT_INFORMATION: i32 = 9;
const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: u32 = 0x2000;

#[link(name = "kernel32")]
extern "system" {
    fn CreateJobObjectW(attributes: *mut c_void, name: *const u16) -> Handle;
    fn SetInformationJobObject(job: Handle, class: i32, info: *mut c_void, length: u32) -> i32;
    fn AssignProcessToJobObject(job: Handle, process: Handle) -> i32;
    fn TerminateJobObject(job: Handle, exit_code: u32) -> i32;
    fn CloseHandle(handle: Handle) -> i32;
}

/// The exit code of a tool ended by cancel.
const KILLED: u32 = 1;

/// A job with the tool in it; without one (it could not be made) the tool's tree is ended with
/// `taskkill /T /F`.
pub struct Group {
    pid: u32,
    job: Option<Handle>,
}

// SAFETY: a job handle is a kernel handle, usable from any thread; it is closed only in Drop.
unsafe impl Send for Group {}
unsafe impl Sync for Group {}

/// A job that ends its processes when its last handle is closed.
fn new_job() -> Option<Handle> {
    // SAFETY: plain Win32 calls with valid pointers; the handle is closed on failure.
    unsafe {
        let job = CreateJobObjectW(std::ptr::null_mut(), std::ptr::null());
        if job.is_null() {
            return None;
        }
        let mut info = ExtendedLimitInformation::default();
        info.basic.limit_flags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let ok = SetInformationJobObject(
            job,
            JOB_OBJECT_EXTENDED_LIMIT_INFORMATION,
            (&mut info as *mut ExtendedLimitInformation).cast(),
            std::mem::size_of::<ExtendedLimitInformation>() as u32,
        );
        if ok == 0 {
            CloseHandle(job);
            return None;
        }
        Some(job)
    }
}

impl Group {
    /// Puts the child into a job of its own. (The child runs from its start; a process it starts
    /// before it is assigned, within microseconds, would not be in the job. ProSE starts none.)
    pub fn new(child: &std::process::Child) -> Group {
        let job = new_job().filter(|job| {
            // SAFETY: the child's process handle is valid while `child` lives.
            let assigned =
                unsafe { AssignProcessToJobObject(*job, child.as_raw_handle() as Handle) };
            if assigned == 0 {
                // SAFETY: the job was made above and is not used elsewhere.
                unsafe { CloseHandle(*job) };
            }
            assigned != 0
        });
        Group {
            pid: child.id(),
            job,
        }
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// Windows has no request to end for a console tool without a console: ends the job.
    pub fn terminate(&self) {
        self.kill();
    }

    /// Ends every process of the job.
    pub fn kill(&self) {
        match self.job {
            // SAFETY: the job handle is open until Drop.
            Some(job) => unsafe {
                TerminateJobObject(job, KILLED);
            },
            None => {
                let _ = Command::new("taskkill")
                    .args(["/PID", &self.pid.to_string(), "/T", "/F"])
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
            }
        }
    }
}

impl Drop for Group {
    fn drop(&mut self) {
        if let Some(job) = self.job.take() {
            // SAFETY: the handle was opened by `new_job` and is closed once.
            unsafe { CloseHandle(job) };
        }
    }
}

pub fn exit_signal(_status: &std::process::ExitStatus) -> Option<i32> {
    None
}

/// (The console window is suppressed for every run of the tool: `tool::command`.)
pub fn prepare(_command: &mut std::process::Command) {}

/// Only `Placement::HardLink` reaches the folder on Windows; a symbolic link needs a privilege.
pub fn link_input(original: &std::path::Path, link: &std::path::Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_file(original, link)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The job is made and its limit set (else cancel falls back to taskkill unnoticed): the
    /// structure has the size Windows expects (JOBOBJECT_EXTENDED_LIMIT_INFORMATION, x64: 144 bytes).
    #[test]
    fn a_job_object_that_ends_its_processes_is_made_and_holds_the_tool() {
        assert_eq!(std::mem::size_of::<ExtendedLimitInformation>(), 144);
        let job = new_job().expect("a job object with KILL_ON_JOB_CLOSE");
        // SAFETY: the handle was just made.
        unsafe { CloseHandle(job) };
        let mut child = Command::new("cmd")
            .args(["/C", "ping -n 30 127.0.0.1 >NUL"])
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        let group = Group::new(&child);
        assert!(group.job.is_some(), "the child is in a job");
        group.kill();
        let status = child.wait().unwrap();
        assert_eq!(status.code(), Some(KILLED as i32), "ended by the job");
    }
}
