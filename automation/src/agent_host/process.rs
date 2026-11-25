//! Starting agent processes, behind a trait so every path of the host can
//! run against a fake agent in tests.
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// A process to start: program, arguments, folder and extra environment.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Launch {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    /// Added to the inherited environment.
    pub env: Vec<(String, String)>,
    /// Whether the process gets a stdin pipe (else stdin is null).
    pub stdin: bool,
}

/// A started process. The pipes are taken once each.
pub trait AgentProcess: Send {
    fn take_stdin(&mut self) -> Option<Box<dyn Write + Send>>;
    fn take_stdout(&mut self) -> Option<Box<dyn Read + Send>>;
    fn take_stderr(&mut self) -> Option<Box<dyn Read + Send>>;
    /// Stop the process (only ever the one this host started).
    fn kill(&mut self) -> io::Result<()>;
    /// The exit code once it has ended (-1 when it has none).
    fn try_wait(&mut self) -> io::Result<Option<i32>>;
}

/// Starts processes. [`SystemSpawner`] starts real ones.
pub trait Spawner: Send + Sync {
    fn spawn(&self, launch: &Launch) -> io::Result<Box<dyn AgentProcess>>;
}

/// The real spawner: `std::process`, with no console window on Windows.
///
/// A `.cmd` or `.bat` install (an npm shim) is started by `std::process`
/// itself, never through `cmd /C` by hand, so the standard library's batch
/// file quoting applies; it refuses an argument it cannot pass safely, and
/// so does [`batch_safe`] for line breaks. Only fixed flags and paths the
/// host generates are ever arguments: messages and schedule text go in on
/// stdin.
///
/// On Windows every child is put in a job object that kills the whole tree
/// when it is stopped or dropped, so a grandchild (an MCP server, a
/// script) never outlives it holding a pipe open.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemSpawner;

/// Whether `program` is a batch file, which `cmd` interprets.
pub fn is_batch(program: &std::path::Path) -> bool {
    program
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("cmd") || e.eq_ignore_ascii_case("bat"))
}

/// Whether `arg` can go to a batch file: no line breaks or NUL, which
/// `cmd` cannot take inside an argument at all.
pub fn batch_safe(arg: &str) -> bool {
    !arg.contains(['\r', '\n', '\0'])
}

struct SystemProcess {
    child: std::process::Child,
    #[cfg(windows)]
    job: Option<job::Job>,
}

impl AgentProcess for SystemProcess {
    fn take_stdin(&mut self) -> Option<Box<dyn Write + Send>> {
        self.child
            .stdin
            .take()
            .map(|s| Box::new(s) as Box<dyn Write + Send>)
    }
    fn take_stdout(&mut self) -> Option<Box<dyn Read + Send>> {
        self.child
            .stdout
            .take()
            .map(|s| Box::new(s) as Box<dyn Read + Send>)
    }
    fn take_stderr(&mut self) -> Option<Box<dyn Read + Send>> {
        self.child
            .stderr
            .take()
            .map(|s| Box::new(s) as Box<dyn Read + Send>)
    }
    fn kill(&mut self) -> io::Result<()> {
        // The whole tree of this child, whether or not the child itself
        // has ended already.
        #[cfg(windows)]
        if let Some(job) = &self.job {
            job.terminate();
        }
        if self.child.try_wait()?.is_some() {
            return Ok(());
        }
        self.child.kill()
    }
    fn try_wait(&mut self) -> io::Result<Option<i32>> {
        Ok(self.child.try_wait()?.map(|s| s.code().unwrap_or(-1)))
    }
}

impl Drop for SystemProcess {
    fn drop(&mut self) {
        // Never leave a child (or its children) running.
        let _ = self.kill();
        let _ = self.child.wait();
    }
}

/// A Windows job object that ends every process in it when it is
/// terminated or closed. Declared here from kernel32 directly: the calls
/// are few and no other crate is needed.
#[cfg(windows)]
mod job {
    use std::ffi::c_void;

    type Handle = *mut c_void;

    #[repr(C)]
    #[derive(Default)]
    struct BasicLimits {
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

    #[repr(C)]
    #[derive(Default)]
    struct IoCounters {
        counts: [u64; 6],
    }

    #[repr(C)]
    #[derive(Default)]
    struct ExtendedLimits {
        basic: BasicLimits,
        io: IoCounters,
        process_memory_limit: usize,
        job_memory_limit: usize,
        peak_process_memory_used: usize,
        peak_job_memory_used: usize,
    }

    const KILL_ON_JOB_CLOSE: u32 = 0x2000;
    const EXTENDED_LIMIT_INFORMATION: i32 = 9;

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateJobObjectW(attributes: *mut c_void, name: *const u16) -> Handle;
        fn SetInformationJobObject(job: Handle, class: i32, info: *mut c_void, len: u32) -> i32;
        fn AssignProcessToJobObject(job: Handle, process: Handle) -> i32;
        fn TerminateJobObject(job: Handle, code: u32) -> i32;
        fn CloseHandle(handle: Handle) -> i32;
    }

    /// The job of one child. Owned by that child's handle only.
    pub struct Job(Handle);

    // The handle is used only through these calls, which any thread may make.
    unsafe impl Send for Job {}

    impl Job {
        /// A kill-on-close job holding `child`, or `None` when Windows
        /// refuses (the child is then stopped on its own, as before).
        pub fn hold(child: &std::process::Child) -> Option<Job> {
            use std::os::windows::io::AsRawHandle;
            // SAFETY: plain Win32 calls on handles this function owns (the
            // new job) or borrows for the call (the child's process handle,
            // alive as long as `child`); the limits struct is laid out as
            // JOBOBJECT_EXTENDED_LIMIT_INFORMATION.
            unsafe {
                let job = CreateJobObjectW(std::ptr::null_mut(), std::ptr::null());
                if job.is_null() {
                    return None;
                }
                let mut limits = ExtendedLimits::default();
                limits.basic.limit_flags = KILL_ON_JOB_CLOSE;
                let set = SetInformationJobObject(
                    job,
                    EXTENDED_LIMIT_INFORMATION,
                    &mut limits as *mut ExtendedLimits as *mut c_void,
                    std::mem::size_of::<ExtendedLimits>() as u32,
                );
                if set == 0 || AssignProcessToJobObject(job, child.as_raw_handle() as Handle) == 0 {
                    CloseHandle(job);
                    return None;
                }
                Some(Job(job))
            }
        }

        /// End every process in the job.
        pub fn terminate(&self) {
            // SAFETY: the job handle is open until Drop.
            unsafe {
                TerminateJobObject(self.0, 1);
            }
        }
    }

    impl Drop for Job {
        fn drop(&mut self) {
            // SAFETY: closes the handle this Job owns; kill-on-close ends
            // whatever is still in the job.
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
}

impl Spawner for SystemSpawner {
    fn spawn(&self, launch: &Launch) -> io::Result<Box<dyn AgentProcess>> {
        if is_batch(&launch.program) && !launch.args.iter().all(|a| batch_safe(a)) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "an argument cannot be passed to a batch file safely",
            ));
        }
        let mut command = Command::new(&launch.program);
        command
            .args(&launch.args)
            .stdin(if launch.stdin {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(cwd) = &launch.cwd {
            command.current_dir(cwd);
        }
        for (k, v) in &launch.env {
            command.env(k, v);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        let child = command.spawn()?;
        Ok(Box::new(SystemProcess {
            #[cfg(windows)]
            job: job::Job::hold(&child),
            child,
        }))
    }
}

/// Call `each` with every line `reader` gives until it ends, decoding
/// invalid UTF-8 as U+FFFD instead of stopping, without the line end.
pub fn for_each_line(reader: impl Read, mut each: impl FnMut(String)) {
    let mut reader = io::BufReader::new(reader);
    let mut bytes = vec![];
    loop {
        bytes.clear();
        match io::BufRead::read_until(&mut reader, b'\n', &mut bytes) {
            Ok(0) => return,
            Ok(_) => {
                while matches!(bytes.last(), Some(b'\n' | b'\r')) {
                    bytes.pop();
                }
                each(String::from_utf8_lossy(&bytes).into_owned());
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => return,
        }
    }
}

/// What a short command printed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Captured {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

fn drain(reader: Option<Box<dyn Read + Send>>) -> mpsc::Receiver<String> {
    let (send, receive) = mpsc::channel();
    std::thread::spawn(move || {
        let mut text = String::new();
        if let Some(mut r) = reader {
            let mut bytes = vec![];
            let _ = r.read_to_end(&mut bytes);
            text = String::from_utf8_lossy(&bytes).into_owned();
        }
        let _ = send.send(text);
    });
    receive
}

/// Run a short command (`--version`, a sign-in status) to its end, or stop
/// it after `timeout`. `None` when it could not start or did not finish.
pub fn capture(spawner: &dyn Spawner, launch: &Launch, timeout: Duration) -> Option<Captured> {
    let mut process = spawner.spawn(launch).ok()?;
    drop(process.take_stdin());
    let out = drain(process.take_stdout());
    let err = drain(process.take_stderr());
    let deadline = Instant::now() + timeout;
    let code = loop {
        match process.try_wait() {
            Ok(Some(code)) => break code,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(15)),
            _ => {
                let _ = process.kill();
                return None;
            }
        }
    };
    let left = deadline.saturating_duration_since(Instant::now()) + Duration::from_millis(500);
    Some(Captured {
        code,
        stdout: out.recv_timeout(left).unwrap_or_default(),
        stderr: err.recv_timeout(left).unwrap_or_default(),
    })
}
