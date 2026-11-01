//! The few Win32 functions the DLL needs.

#[link(name = "kernel32")]
unsafe extern "system" {
    pub fn VirtualAlloc(address: *mut u8, size: usize, alloc_type: u32, protect: u32) -> *mut u8;
    pub fn VirtualProtect(address: *mut u8, size: usize, protect: u32, old: *mut u32) -> i32;
    pub fn FlushInstructionCache(process: isize, address: *const u8, size: usize) -> i32;
    pub fn GetCurrentProcess() -> isize;
    pub fn GetModuleFileNameW(module: *mut u8, buffer: *mut u16, size: u32) -> u32;
    fn GetLocalTime(time: *mut SystemTime);
    pub fn CreateJobObjectW(attributes: *const u8, name: *const u16) -> isize;
    pub fn SetInformationJobObject(job: isize, class: u32, info: *const u8, size: u32) -> i32;
    pub fn AssignProcessToJobObject(job: isize, process: isize) -> i32;
}

/// SYSTEMTIME
#[repr(C)]
#[derive(Default)]
pub struct SystemTime {
    pub year: u16,
    pub month: u16,
    pub day_of_week: u16,
    pub day: u16,
    pub hour: u16,
    pub minute: u16,
    pub second: u16,
    pub milliseconds: u16,
}

pub fn local_time() -> SystemTime {
    let mut t = SystemTime::default();
    unsafe { GetLocalTime(&mut t) };
    t
}

#[repr(C)]
#[derive(Default)]
struct JobBasicLimits {
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
struct JobExtendedLimits {
    basic: JobBasicLimits,
    io_counters: [u64; 6],
    process_memory_limit: usize,
    job_memory_limit: usize,
    peak_process_memory_used: usize,
    peak_job_memory_used: usize,
}

/// Puts a child process in a job that kills it when the job handle closes. The handle is never closed by hand, so it
/// closes when this process (the game) exits, however it exits.
pub fn kill_with_this_process(child: &std::process::Child) -> bool {
    use std::os::windows::io::AsRawHandle;
    const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: u32 = 0x2000;
    const JOB_OBJECT_EXTENDED_LIMIT_INFORMATION: u32 = 9;
    unsafe {
        let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if job == 0 {
            return false;
        }
        let mut info = JobExtendedLimits::default();
        info.basic.limit_flags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let size = std::mem::size_of::<JobExtendedLimits>() as u32;
        SetInformationJobObject(job, JOB_OBJECT_EXTENDED_LIMIT_INFORMATION, &info as *const _ as *const u8, size) != 0
            && AssignProcessToJobObject(job, child.as_raw_handle() as isize) != 0
    }
}

#[link(name = "user32")]
unsafe extern "system" {
    pub fn GetAsyncKeyState(vkey: i32) -> i16;
    pub fn GetForegroundWindow() -> isize;
    pub fn GetWindowThreadProcessId(hwnd: isize, pid: *mut u32) -> u32;
}

pub const MEM_COMMIT: u32 = 0x1000;
pub const MEM_RESERVE: u32 = 0x2000;
pub const PAGE_EXECUTE_READWRITE: u32 = 0x40;

pub fn game_focused() -> bool {
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(GetForegroundWindow(), &mut pid) };
    pid == std::process::id()
}

pub fn key_down(vk: i32) -> bool {
    unsafe { GetAsyncKeyState(vk) < 0 }
}
