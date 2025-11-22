use sysinfo::{ProcessesToUpdate, System};

use crate::LoggingError;

#[derive(Debug, Clone)]
pub struct SysInfo {
    pub pid: u32,
    pub memory: u64,
    pub cpu_usage: f32,
    pub threads: usize,
    pub disk_read_bytes: u64,
    pub disk_written_bytes: u64,
}

pub fn collect_sysinfo() -> Result<SysInfo, LoggingError> {
    let mut sys = System::new();

    let pid = sysinfo::get_current_pid().map_err(|e| LoggingError::MissingPid(e.to_string()))?;

    sys.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);

    let process = sys
        .process(pid)
        .ok_or_else(|| LoggingError::MissingPid(format!("Process {} not found", pid)))?;

    let disk_usage = process.disk_usage();

    Ok(SysInfo {
        pid: pid.as_u32(),
        memory: process.memory(),
        cpu_usage: process.cpu_usage(),
        threads: process.tasks().map(|tasks| tasks.len()).unwrap_or(0),
        disk_read_bytes: disk_usage.total_read_bytes,
        disk_written_bytes: disk_usage.total_written_bytes,
    })
}

#[cfg(test)]
mod tests {
    use crate::sysinfo::collect_sysinfo;

    #[test]
    fn test_collect_sysinfo_success() {
        let result = collect_sysinfo();
        assert!(result.is_ok(), "collect_sysinfo should succeed");

        let sysinfo = result.unwrap();

        // Verify that we got a valid PID (should be non-zero)
        assert!(sysinfo.pid > 0, "PID should be greater than 0");

        // Memory should typically be non-zero for a running process
        assert!(sysinfo.memory > 0, "Memory usage should be greater than 0");

        // CPU usage should be non-negative
        assert!(sysinfo.cpu_usage >= 0.0, "CPU usage should be non-negative");

        #[cfg(target_os = "linux")]
        assert!(
            sysinfo.threads >= 1,
            "Should have at least 1 thread on Linux"
        );

        #[cfg(not(target_os = "linux"))]
        assert_eq!(
            sysinfo.threads, 0,
            "Thread count should be 0 on non-Linux platforms"
        );
    }
}
