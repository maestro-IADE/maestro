//! Process-tree helpers both runtimes share.

/// Kills `pid` and everything under it (Windows: the CLI's shells and MCP servers hang below it; a plain kill
/// would leave them running). Best effort: a process already gone is no error.
pub fn kill_tree(pid: u32) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
            .output();
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("pkill").args(["-KILL", "-P", &pid.to_string()]).output();
        let _ = std::process::Command::new("kill").args(["-KILL", &pid.to_string()]).output();
    }
}

/// Before the spawn: the child gets a tree of its own (`Tree::adopt`). Unix: its own process group.
pub fn contain(cmd: &mut std::process::Command) -> &mut std::process::Command {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    cmd
}

/// A child and everything it starts, killable as one even after the child itself is gone: `taskkill /T` can't find
/// the orphans of a dead parent, and an orphaned claude would go on writing the conversation. Windows: a job object
/// that allows no breakaway (closing it, Maestro's exit included, kills them all, detached ones too); Unix: the
/// process group `contain` made.
pub struct Tree {
    #[cfg(windows)]
    job: usize,
    #[cfg(unix)]
    pgid: u32,
}

impl Tree {
    /// None: it couldn't be set up (the caller falls back to `kill_tree` while the root is there).
    pub fn adopt(child: &std::process::Child) -> Option<Tree> {
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            // SAFETY: plain Win32 calls on a job we own and the child's live handle; a failure closes the job
            unsafe {
                let job = win::CreateJobObjectW(std::ptr::null(), std::ptr::null());
                if job.is_null() {
                    return None;
                }
                let mut info = win::ExtendedLimit::default();
                // no breakaway: libuv (node, Bun's claude) puts each child in a job of its own that lets *its*
                // children break away silently, and a silent breakaway climbs past every parent job that allows
                // one. Without it here they stop at this job. libuv never asks for CREATE_BREAKAWAY_FROM_JOB.
                info.basic.limit_flags = win::KILL_ON_JOB_CLOSE;
                let set = win::SetInformationJobObject(job, win::EXTENDED_LIMIT_INFORMATION, &info as *const _ as *const _, std::mem::size_of::<win::ExtendedLimit>() as u32);
                if set == 0 || win::AssignProcessToJobObject(job, child.as_raw_handle() as win::Handle) == 0 {
                    win::CloseHandle(job);
                    return None;
                }
                Some(Tree { job: job as usize })
            }
        }
        #[cfg(unix)]
        {
            Some(Tree { pgid: child.id() })
        }
        #[cfg(not(any(windows, unix)))]
        {
            let _ = child;
            None
        }
    }

    /// Every process in the tree, now.
    pub fn kill(&self) {
        #[cfg(windows)]
        // SAFETY: the job handle stays open until drop
        unsafe {
            win::TerminateJobObject(self.job as win::Handle, 1);
        }
        #[cfg(unix)]
        {
            let _ = std::process::Command::new("kill").args(["-KILL", "--", &format!("-{}", self.pgid)]).output();
        }
    }

    /// No process of the tree is left. Exact, unlike a parent-id walk (Windows reuses pids and never updates
    /// parent ids). None: it couldn't be asked.
    pub fn empty(&self) -> Option<bool> {
        #[cfg(windows)]
        // SAFETY: the job handle stays open until drop; the struct is the class's own
        unsafe {
            let mut info = win::BasicAccounting::default();
            let ok = win::QueryInformationJobObject(self.job as win::Handle, win::BASIC_ACCOUNTING_INFORMATION, &mut info as *mut _ as *mut _, std::mem::size_of::<win::BasicAccounting>() as u32, std::ptr::null_mut());
            (ok != 0).then_some(info.active_processes == 0)
        }
        #[cfg(unix)]
        {
            // signal 0 to the group: fails once no member is left
            let out = std::process::Command::new("kill").args(["-0", "--", &format!("-{}", self.pgid)]).output().ok()?;
            Some(!out.status.success())
        }
        #[cfg(not(any(windows, unix)))]
        None
    }
}

#[cfg(windows)]
impl Drop for Tree {
    fn drop(&mut self) {
        // SAFETY: closed once; with KILL_ON_JOB_CLOSE whatever is left goes with it
        unsafe {
            win::CloseHandle(self.job as win::Handle);
        }
    }
}

#[cfg(windows)]
mod win {
    use std::ffi::c_void;

    pub type Handle = *mut c_void;
    pub const EXTENDED_LIMIT_INFORMATION: i32 = 9; // JobObjectExtendedLimitInformation
    pub const BASIC_ACCOUNTING_INFORMATION: i32 = 1; // JobObjectBasicAccountingInformation
    pub const KILL_ON_JOB_CLOSE: u32 = 0x2000;

    /// JOBOBJECT_BASIC_LIMIT_INFORMATION
    #[repr(C)]
    #[derive(Default)]
    pub struct BasicLimit {
        pub per_process_user_time: i64,
        pub per_job_user_time: i64,
        pub limit_flags: u32,
        pub min_working_set: usize,
        pub max_working_set: usize,
        pub active_process_limit: u32,
        pub affinity: usize,
        pub priority_class: u32,
        pub scheduling_class: u32,
    }

    /// JOBOBJECT_EXTENDED_LIMIT_INFORMATION (IO_COUNTERS inline as six u64)
    #[repr(C)]
    #[derive(Default)]
    pub struct ExtendedLimit {
        pub basic: BasicLimit,
        pub io: [u64; 6],
        pub process_memory_limit: usize,
        pub job_memory_limit: usize,
        pub peak_process_memory: usize,
        pub peak_job_memory: usize,
    }

    /// JOBOBJECT_BASIC_ACCOUNTING_INFORMATION
    #[repr(C)]
    #[derive(Default)]
    pub struct BasicAccounting {
        pub total_user_time: i64,
        pub total_kernel_time: i64,
        pub this_period_user_time: i64,
        pub this_period_kernel_time: i64,
        pub total_page_faults: u32,
        pub total_processes: u32,
        pub active_processes: u32,
        pub total_terminated: u32,
    }

    #[link(name = "kernel32")]
    extern "system" {
        pub fn CreateJobObjectW(attrs: *const c_void, name: *const u16) -> Handle;
        pub fn SetInformationJobObject(job: Handle, class: i32, info: *const c_void, len: u32) -> i32;
        pub fn AssignProcessToJobObject(job: Handle, process: Handle) -> i32;
        pub fn TerminateJobObject(job: Handle, code: u32) -> i32;
        pub fn QueryInformationJobObject(job: Handle, class: i32, info: *mut c_void, len: u32, ret: *mut u32) -> i32;
        pub fn CloseHandle(h: Handle) -> i32;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn the_layout_matches_win32() {
        // sizeof(JOBOBJECT_EXTENDED_LIMIT_INFORMATION) on x64
        if cfg!(target_pointer_width = "64") {
            assert_eq!(std::mem::size_of::<win::ExtendedLimit>(), 144);
        }
        assert_eq!(std::mem::size_of::<win::BasicAccounting>(), 48);
    }

    /// A child's own child outlives it; killing the tree still takes it.
    #[cfg(windows)]
    #[test]
    #[ignore = "starts processes and reads the live process list"]
    fn the_tree_takes_orphans_along() {
        use std::os::windows::process::CommandExt;
        // cmd waits a moment (so it's in the job by then), starts a long ping in the background and exits: the
        // ping is an orphan
        let mut cmd = std::process::Command::new("cmd");
        cmd.raw_arg("/C ping -n 2 127.0.0.1 >NUL & start \"\" /B ping -n 30 127.0.0.1 >NUL");
        let mut child = crate::cli::quiet(contain(&mut cmd)).spawn().unwrap();
        let tree = Tree::adopt(&child).expect("a job");
        let _ = child.wait();
        let pings = || crate::agent::processes().unwrap_or_default().into_iter().filter(|p| p.ppid == child.id() && p.name.eq_ignore_ascii_case("ping.exe")).count();
        // the orphan may take a moment to show up
        let start = std::time::Instant::now();
        while pings() == 0 && start.elapsed() < std::time::Duration::from_secs(5) {
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        assert!(pings() > 0, "the orphan never started");
        assert_eq!(tree.empty(), Some(false), "the orphan is still in the job");
        tree.kill();
        let start = std::time::Instant::now();
        while pings() > 0 && start.elapsed() < std::time::Duration::from_secs(5) {
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        assert_eq!(pings(), 0);
        assert_eq!(tree.empty(), Some(true));
    }
}
