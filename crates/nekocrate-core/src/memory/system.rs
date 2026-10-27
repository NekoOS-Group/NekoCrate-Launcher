//! Only physical RAM is needed; avoid initializing a process/CPU inventory.
//! Keep the accounting used by sysinfo 0.38 so Auto's allocation policy is stable.
use super::SystemMemory;

#[cfg(target_os = "windows")]
pub(super) fn read() -> Option<SystemMemory> {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    let mut status = MEMORYSTATUSEX {
        dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };
    // SAFETY: the initialized buffer has the size required by the Windows API.
    if unsafe { GlobalMemoryStatusEx(&mut status) } == 0 {
        return None;
    }
    Some(SystemMemory {
        total_mb: status.ullTotalPhys / 1048576,
        available_mb: status.ullAvailPhys / 1048576,
    })
}

#[cfg(target_os = "macos")]
#[allow(deprecated)] // libc's Mach bindings are still available on supported macOS targets.
pub(super) fn read() -> Option<SystemMemory> {
    let mut total = 0_u64;
    let mut size = std::mem::size_of_val(&total);
    let mut mib = [libc::CTL_HW, libc::HW_MEMSIZE];
    // SAFETY: sysctl receives an exact-sized u64 output buffer; no value is set.
    let result = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            2,
            (&mut total as *mut u64).cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if result != 0 || size != std::mem::size_of_val(&total) {
        return None;
    }
    // SAFETY: sysconf has no pointer parameters and reports the actual (e.g. 16K) page size.
    let page_size = u64::try_from(unsafe { libc::sysconf(libc::_SC_PAGESIZE) }).ok()?;
    if page_size == 0 {
        return None;
    }
    let mut statistics = std::mem::MaybeUninit::<libc::vm_statistics64>::zeroed();
    let mut count = libc::HOST_VM_INFO64_COUNT;
    // The host send right lives for the process lifetime. Acquire it once, not on
    // every RAM refresh (mach_host_self increments the send-right reference count).
    static HOST: std::sync::OnceLock<libc::mach_port_t> = std::sync::OnceLock::new();
    // SAFETY: obtaining the current host port requires no pointer arguments.
    let host = *HOST.get_or_init(|| unsafe { libc::mach_host_self() });
    // SAFETY: the host port is valid for this task, the statistics buffer and count
    // match HOST_VM_INFO64.
    let result = unsafe {
        libc::host_statistics64(
            host,
            libc::HOST_VM_INFO64,
            statistics.as_mut_ptr().cast(),
            &mut count,
        )
    };
    if result != libc::KERN_SUCCESS {
        return None;
    }
    // SAFETY: the successful call initialized the statistics; remaining bytes were zeroed.
    let statistics = unsafe { statistics.assume_init() };
    // free_count already includes speculative pages. Preserve sysinfo's
    // non-compressed available-memory definition, not just completely free RAM.
    let pages = u64::from(statistics.active_count)
        .saturating_add(u64::from(statistics.inactive_count))
        .saturating_add(u64::from(statistics.free_count));
    Some(SystemMemory {
        total_mb: total / 1048576,
        available_mb: pages.saturating_mul(page_size) / 1048576,
    })
}

#[cfg(target_os = "linux")]
pub(super) fn read() -> Option<SystemMemory> {
    parse_meminfo(&std::fs::read_to_string("/proc/meminfo").ok()?)
}

#[cfg(any(target_os = "linux", test))]
fn parse_meminfo(text: &str) -> Option<SystemMemory> {
    let mut total = None;
    let mut available = None;
    let (mut free, mut buffers, mut cached, mut reclaimable, mut shared) = (0_u64, 0, 0, 0, 0);
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let mut parts = value.split_whitespace();
        let Some(value) = parts.next().and_then(|value| value.parse::<u64>().ok()) else {
            continue;
        };
        if parts.next() != Some("kB") {
            continue;
        }
        // Linux calls these kB, but the values are KiB.
        match key {
            "MemTotal" => total = Some(value),
            "MemAvailable" => available = Some(value),
            "MemFree" => free = value,
            "Buffers" => buffers = value,
            "Cached" => cached = value,
            "SReclaimable" => reclaimable = value,
            "Shmem" => shared = value,
            _ => {}
        }
    }
    let available = available.unwrap_or_else(|| {
        free.saturating_add(buffers)
            .saturating_add(cached)
            .saturating_add(reclaimable)
            .saturating_sub(shared)
    });
    Some(SystemMemory {
        total_mb: total? / 1024,
        available_mb: available / 1024,
    })
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
pub(super) fn read() -> Option<SystemMemory> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linux_prefers_available_and_preserves_old_kernel_fallback() {
        let text = "MemTotal: 8192000 kB\nMemFree: 1024000 kB\nBuffers: 102400 kB\nCached: 2048000 kB\nSReclaimable: 102400 kB\nShmem: 51200 kB\n";
        let old = parse_meminfo(text).unwrap();
        assert_eq!((old.total_mb, old.available_mb), (8000, 3150));
        let current = parse_meminfo(&format!("{text}MemAvailable: 4096000 kB\n")).unwrap();
        assert_eq!((current.total_mb, current.available_mb), (8000, 4000));
        assert_eq!(
            parse_meminfo(&format!("{text}MemAvailable: 0 kB\n"))
                .unwrap()
                .available_mb,
            0
        );
        assert!(parse_meminfo("MemTotal: invalid kB").is_none());
        assert!(parse_meminfo("MemTotal: 1024 bytes").is_none());
        assert_eq!(
            parse_meminfo("MemTotal: 1024 kB\nShmem: 2048 kB")
                .unwrap()
                .available_mb,
            0
        );
    }

    #[test]
    #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
    fn native_memory_query_reports_physical_ram() {
        let memory = read().expect("OS physical-memory query");
        assert!(memory.total_mb > 0);
        assert!(memory.available_mb <= memory.total_mb);
    }
}
