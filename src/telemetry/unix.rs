//! Pure-Rust Unix/Linux/POSIX telemetry provider.
//!
//! Queries process memory counters, CPU execution times, active threads,
//! and open file descriptors via `/proc/self/*` on Linux, with standard
//! POSIX `getrusage` fallback for other Unix environments (e.g. macOS).

use std::fs;

#[repr(C)]
#[derive(Default, Copy, Clone)]
#[allow(non_camel_case_types)]
struct timeval {
    tv_sec: i64,
    tv_usec: i64,
}

#[repr(C)]
#[derive(Default, Copy, Clone)]
#[allow(non_camel_case_types, dead_code)]
struct rusage {
    ru_utime: timeval,
    ru_stime: timeval,
    ru_maxrss: i64,
    ru_ixrss: i64,
    ru_idrss: i64,
    ru_isrss: i64,
    ru_minflt: i64,
    ru_majflt: i64,
    ru_nswap: i64,
    ru_inblock: i64,
    ru_oublock: i64,
    ru_msgsnd: i64,
    ru_msgrcv: i64,
    ru_nsignals: i64,
    ru_nvcsw: i64,
    ru_nivcsw: i64,
}

const RUSAGE_SELF: i32 = 0;

extern "C" {
    fn getrusage(who: i32, usage: *mut rusage) -> i32;
}

/// Retrieves process physical memory (RSS), peak memory, and virtual memory in bytes.
pub fn get_memory() -> (u64, u64, u64) {
    // 1. Try reading /proc/self/status for accurate VmRSS, VmHWM, VmSize on Linux
    if let Ok(status_str) = fs::read_to_string("/proc/self/status") {
        let mut rss_kb = 0u64;
        let mut peak_kb = 0u64;
        let mut virt_kb = 0u64;

        for line in status_str.lines() {
            if let Some(rest) = line.strip_prefix("VmRSS:") {
                rss_kb = parse_kb(rest);
            } else if let Some(rest) = line.strip_prefix("VmHWM:") {
                peak_kb = parse_kb(rest);
            } else if let Some(rest) = line.strip_prefix("VmSize:") {
                virt_kb = parse_kb(rest);
            }
        }

        if rss_kb > 0 {
            let rss = rss_kb * 1024;
            let peak = (peak_kb * 1024).max(rss);
            let virt = virt_kb * 1024;
            return (rss, peak, virt);
        }
    }

    // 2. Try reading /proc/self/statm (pages: total resident shared text lib data dt)
    if let Ok(statm_str) = fs::read_to_string("/proc/self/statm") {
        let parts: Vec<&str> = statm_str.split_whitespace().collect();
        if parts.len() >= 2 {
            if let (Ok(total_pages), Ok(resident_pages)) =
                (parts[0].parse::<u64>(), parts[1].parse::<u64>())
            {
                let page_size = 4096u64;
                let virt = total_pages * page_size;
                let rss = resident_pages * page_size;
                return (rss, rss, virt);
            }
        }
    }

    // 3. Fallback: POSIX getrusage
    let mut usage = rusage::default();
    if unsafe { getrusage(RUSAGE_SELF, &mut usage) } == 0 {
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        let peak_rss = usage.ru_maxrss.max(0) as u64; // macOS reports bytes
        #[cfg(not(any(target_os = "macos", target_os = "ios")))]
        let peak_rss = (usage.ru_maxrss.max(0) as u64) * 1024; // Linux/BSD reports kB

        (peak_rss, peak_rss, peak_rss)
    } else {
        (0, 0, 0)
    }
}

/// Retrieves cumulative user and kernel CPU execution times in milliseconds.
pub fn get_cpu_times() -> (u64, u64) {
    // 1. Try reading /proc/self/stat
    if let Ok(stat_str) = fs::read_to_string("/proc/self/stat") {
        // Field 2 is (comm) which may contain spaces or parentheses.
        // Find the last ')' to safely parse fields 3 onwards.
        if let Some(paren_idx) = stat_str.rfind(')') {
            let rest = &stat_str[paren_idx + 1..];
            let fields: Vec<&str> = rest.split_whitespace().collect();
            // In the remaining fields:
            // field 11 corresponds to utime (token 14)
            // field 12 corresponds to stime (token 15)
            if fields.len() > 12 {
                if let (Ok(utime_ticks), Ok(stime_ticks)) =
                    (fields[11].parse::<u64>(), fields[12].parse::<u64>())
                {
                    // Standard Linux clock ticks are 100 Hz (10 ms per tick)
                    let user_ms = utime_ticks * 10;
                    let kernel_ms = stime_ticks * 10;
                    return (user_ms, kernel_ms);
                }
            }
        }
    }

    // 2. Fallback: POSIX getrusage
    let mut usage = rusage::default();
    if unsafe { getrusage(RUSAGE_SELF, &mut usage) } == 0 {
        let user_ms = (usage.ru_utime.tv_sec.max(0) as u64) * 1000
            + (usage.ru_utime.tv_usec.max(0) as u64) / 1000;
        let kernel_ms = (usage.ru_stime.tv_sec.max(0) as u64) * 1000
            + (usage.ru_stime.tv_usec.max(0) as u64) / 1000;
        (user_ms, kernel_ms)
    } else {
        (0, 0)
    }
}

/// Enumerates and counts active OS threads belonging to the current process.
pub fn get_active_threads() -> usize {
    // 1. Check /proc/self/task
    if let Ok(entries) = fs::read_dir("/proc/self/task") {
        let count = entries.flatten().count();
        if count > 0 {
            return count;
        }
    }

    // 2. Check /proc/self/status for "Threads: <N>"
    if let Ok(status_str) = fs::read_to_string("/proc/self/status") {
        for line in status_str.lines() {
            if let Some(rest) = line.strip_prefix("Threads:") {
                if let Ok(n) = rest.trim().parse::<usize>() {
                    return n.max(1);
                }
            }
        }
    }

    1
}

/// Retrieves the total number of open file descriptors for the current process.
pub fn get_process_handles() -> Option<usize> {
    // On Unix, file descriptors represent open handles: /proc/self/fd
    if let Ok(entries) = fs::read_dir("/proc/self/fd") {
        Some(entries.flatten().count())
    } else {
        None
    }
}

fn parse_kb(s: &str) -> u64 {
    s.split_whitespace()
        .next()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0)
}
