//! The tool's processes as a process group (Unix): signalled as one, tied to the app on Linux
//! (`PR_SET_PDEATHSIG`), and the inputs' symbolic links.

use std::os::raw::c_int;

extern "C" {
    fn kill(pid: c_int, sig: c_int) -> c_int;
    #[cfg(target_os = "linux")]
    fn prctl(option: c_int, ...) -> c_int;
}

const SIGKILL: c_int = 9;
const SIGTERM: c_int = 15;

/// The tool's process group: the tool is started as the leader of one (`prepare`).
pub struct Group {
    pid: u32,
}

impl Group {
    pub fn new(child: &std::process::Child) -> Group {
        Group { pid: child.id() }
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// Signals every process of the group.
    fn signal(&self, signal: c_int) {
        if self.pid > 0 && self.pid <= i32::MAX as u32 {
            // SAFETY: kill(2) with a negative pid signals a process group; it has no memory effects.
            unsafe { kill(-(self.pid as c_int), signal) };
        }
    }

    /// Asks the group to end (SIGTERM).
    pub fn terminate(&self) {
        self.signal(SIGTERM);
    }

    /// Ends the group (SIGKILL).
    pub fn kill(&self) {
        self.signal(SIGKILL);
    }
}

pub fn exit_signal(status: &std::process::ExitStatus) -> Option<i32> {
    std::os::unix::process::ExitStatusExt::signal(status)
}

/// Own process group; on Linux killed with the app.
pub fn prepare(command: &mut std::process::Command) {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
    #[cfg(target_os = "linux")]
    // SAFETY: only prctl(2) runs between fork and exec, which is async-signal-safe.
    unsafe {
        command.pre_exec(|| {
            const PR_SET_PDEATHSIG: c_int = 1;
            prctl(PR_SET_PDEATHSIG, SIGKILL as std::os::raw::c_ulong);
            Ok(())
        });
    }
}

pub fn link_input(original: &std::path::Path, link: &std::path::Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(original, link)
}
