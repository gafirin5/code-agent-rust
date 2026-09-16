//! Pure-Rust Windows Win32 FFI telemetry provider.
//!
//! Queries process memory counters, CPU execution times, active threads,
//! and process handle count using standard Win32 APIs with 0 external dependencies.

use std::ffi::c_void;
use std::mem::size_of;

#[repr(C)]
#[allow(dead_code, non_snake_case, clippy::upper_case_acronyms)]
struct PROCESS_MEMORY_COUNTERS {
    cb: u32,
    PageFaultCount: u32,
    PeakWorkingSetSize: usize,
    WorkingSetSize: usize,
    QuotaPeakPagedPoolUsage: usize,
    QuotaPagedPoolUsage: usize,
    QuotaPeakNonPagedPoolUsage: usize,
    QuotaNonPagedPoolUsage: usize,
    PagefileUsage: usize,
    PeakPagefileUsage: usize,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
#[allow(dead_code, non_snake_case, clippy::upper_case_acronyms)]
struct FILETIME {
    dwLowDateTime: u32,
    dwHighDateTime: u32,
}

const TH32CS_SNAPTHREAD: u32 = 0x00000004;
const INVALID_HANDLE_VALUE: *mut c_void = -1isize as *mut c_void;

#[repr(C)]
#[allow(dead_code, non_snake_case, clippy::upper_case_acronyms)]
struct THREADENTRY32 {
    dwSize: u32,
    cntUsage: u32,
    th32ThreadID: u32,
    th32OwnerProcessID: u32,
    tpBasePri: i32,
    tpDeltaPri: i32,
    dwFlags: u32,
}

#[link(name = "kernel32")]
extern "system" {
    fn GetCurrentProcess() -> *mut c_void;
    fn GetCurrentProcessId() -> u32;
    fn K32GetProcessMemoryInfo(
        hProcess: *mut c_void,
        ppmc: *mut PROCESS_MEMORY_COUNTERS,
        cb: u32,
    ) -> i32;
    fn GetProcessTimes(
        hProcess: *mut c_void,
        lpCreationTime: *mut FILETIME,
        lpExitTime: *mut FILETIME,
        lpKernelTime: *mut FILETIME,
        lpUserTime: *mut FILETIME,
    ) -> i32;
    fn CreateToolhelp32Snapshot(dwFlags: u32, th32ProcessID: u32) -> *mut c_void;
    fn Thread32First(hSnapshot: *mut c_void, lpte: *mut THREADENTRY32) -> i32;
    fn Thread32Next(hSnapshot: *mut c_void, lpte: *mut THREADENTRY32) -> i32;
    fn CloseHandle(hObject: *mut c_void) -> i32;
    fn GetProcessHandleCount(hProcess: *mut c_void, pdwHandleCount: *mut u32) -> i32;
}

/// Retrieves process physical memory (RSS), peak memory, and virtual committed memory in bytes.
pub fn get_memory() -> (u64, u64, u64) {
    unsafe {
        let mut pmc = std::mem::zeroed::<PROCESS_MEMORY_COUNTERS>();
        pmc.cb = size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        let proc = GetCurrentProcess();
        if K32GetProcessMemoryInfo(proc, &mut pmc, pmc.cb) != 0 {
            let rss = pmc.WorkingSetSize as u64;
            let peak = (pmc.PeakWorkingSetSize as u64).max(rss);
            let virt = pmc.PagefileUsage as u64;
            (rss, peak, virt)
        } else {
            (0, 0, 0)
        }
    }
}

/// Retrieves cumulative user and kernel CPU execution times in milliseconds.
pub fn get_cpu_times() -> (u64, u64) {
    unsafe {
        let mut create = FILETIME::default();
        let mut exit = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        let proc = GetCurrentProcess();
        if GetProcessTimes(proc, &mut create, &mut exit, &mut kernel, &mut user) != 0 {
            let kernel_100ns =
                ((kernel.dwHighDateTime as u64) << 32) | (kernel.dwLowDateTime as u64);
            let user_100ns =
                ((user.dwHighDateTime as u64) << 32) | (user.dwLowDateTime as u64);
            // 1 millisecond = 10,000 * 100ns
            let user_ms = user_100ns / 10_000;
            let kernel_ms = kernel_100ns / 10_000;
            (user_ms, kernel_ms)
        } else {
            (0, 0)
        }
    }
}

/// Enumerates and counts active OS threads belonging to the current process.
pub fn get_active_threads() -> usize {
    unsafe {
        let pid = GetCurrentProcessId();
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
        if snap == INVALID_HANDLE_VALUE || snap.is_null() {
            return 1;
        }
        let mut count = 0usize;
        let mut te = std::mem::zeroed::<THREADENTRY32>();
        te.dwSize = size_of::<THREADENTRY32>() as u32;
        if Thread32First(snap, &mut te) != 0 {
            loop {
                if te.th32OwnerProcessID == pid {
                    count += 1;
                }
                if Thread32Next(snap, &mut te) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snap);
        count.max(1)
    }
}

/// Retrieves the total number of open handles for the current process.
pub fn get_process_handles() -> Option<usize> {
    unsafe {
        let mut count: u32 = 0;
        let proc = GetCurrentProcess();
        if GetProcessHandleCount(proc, &mut count) != 0 {
            Some(count as usize)
        } else {
            None
        }
    }
}
