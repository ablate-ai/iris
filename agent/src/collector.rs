use common::proto::{
    CpuMetrics, DiskMetrics, MemoryMetrics, NetworkMetrics, ProcessMetrics, SystemMetrics,
    SystemInfo, AgentMetrics,
};
use sysinfo::{System, Networks, Disks, Pid, ProcessRefreshKind, ProcessesToUpdate};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

// 全局统计
static METRICS_SENT: AtomicU64 = AtomicU64::new(0);
static ERRORS_COUNT: AtomicU64 = AtomicU64::new(0);

// 探针启动时间
static AGENT_START_TIME: once_cell::sync::Lazy<Instant> =
    once_cell::sync::Lazy::new(Instant::now);

/// 采集系统指标
pub fn collect_metrics() -> SystemMetrics {
    let start = Instant::now();

    let mut sys = System::new_all();
    sys.refresh_all();

    let collection_time_ms = start.elapsed().as_millis() as u64;

    SystemMetrics {
        cpu: Some(collect_cpu_metrics(&sys)),
        memory: Some(collect_memory_metrics(&sys)),
        disks: collect_disk_metrics(),
        network: Some(collect_network_metrics()),
        processes: collect_process_metrics(&sys),
        system_info: Some(collect_system_info(&sys)),
        agent_metrics: Some(collect_agent_metrics(&mut sys, collection_time_ms)),
    }
}

/// 采集探针自身指标
fn collect_agent_metrics(sys: &mut System, collection_time_ms: u64) -> AgentMetrics {
    let current_pid = Pid::from_u32(std::process::id());

    // 刷新当前进程信息
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[current_pid]),
        false,
        ProcessRefreshKind::everything()
    );

    let process = sys.process(current_pid);

    let (cpu_usage, memory_usage) = if let Some(proc) = process {
        (proc.cpu_usage() as f64, proc.memory())
    } else {
        (0.0, 0)
    };

    AgentMetrics {
        cpu_usage,
        memory_usage,
        collection_time_ms,
        uptime_seconds: AGENT_START_TIME.elapsed().as_secs(),
        metrics_sent: METRICS_SENT.load(Ordering::Relaxed),
        errors_count: ERRORS_COUNT.load(Ordering::Relaxed),
    }
}

/// 增加发送成功计数
pub fn increment_metrics_sent() {
    METRICS_SENT.fetch_add(1, Ordering::Relaxed);
}

/// 增加错误计数
pub fn increment_errors() {
    ERRORS_COUNT.fetch_add(1, Ordering::Relaxed);
}


fn collect_cpu_metrics(sys: &System) -> CpuMetrics {
    let cpus = sys.cpus();
    let per_core: Vec<f64> = cpus.iter().map(|cpu| cpu.cpu_usage() as f64).collect();
    let usage_percent = per_core.iter().sum::<f64>() / per_core.len() as f64;

    let load_avg = System::load_average();

    CpuMetrics {
        usage_percent,
        core_count: cpus.len() as i32,
        per_core,
        load_avg_1: load_avg.one,
        load_avg_5: load_avg.five,
        load_avg_15: load_avg.fifteen,
    }
}

fn collect_memory_metrics(sys: &System) -> MemoryMetrics {
    let total = sys.total_memory();
    let used = sys.used_memory();
    let available = sys.available_memory();
    let usage_percent = (used as f64 / total as f64) * 100.0;

    MemoryMetrics {
        total,
        used,
        available,
        usage_percent,
        swap_total: sys.total_swap(),
        swap_used: sys.used_swap(),
    }
}

fn collect_disk_metrics() -> Vec<DiskMetrics> {
    let disks = Disks::new_with_refreshed_list();

    disks
        .iter()
        .map(|disk| {
            let total = disk.total_space();
            let available = disk.available_space();
            let used = total - available;
            let usage_percent = if total > 0 {
                (used as f64 / total as f64) * 100.0
            } else {
                0.0
            };

            DiskMetrics {
                mount_point: disk.mount_point().to_string_lossy().to_string(),
                device: disk.name().to_string_lossy().to_string(),
                total,
                used,
                available,
                usage_percent,
                read_bytes: 0,  // sysinfo 不直接提供，需要其他方式
                write_bytes: 0,
            }
        })
        .collect()
}

fn collect_network_metrics() -> NetworkMetrics {
    let networks = Networks::new_with_refreshed_list();

    let mut bytes_sent = 0u64;
    let mut bytes_recv = 0u64;
    let mut packets_sent = 0u64;
    let mut packets_recv = 0u64;
    let mut errors_in = 0u64;
    let mut errors_out = 0u64;

    for (_name, network) in &networks {
        bytes_sent += network.total_transmitted();
        bytes_recv += network.total_received();
        packets_sent += network.total_packets_transmitted();
        packets_recv += network.total_packets_received();
        errors_in += network.total_errors_on_received();
        errors_out += network.total_errors_on_transmitted();
    }

    NetworkMetrics {
        bytes_sent,
        bytes_recv,
        packets_sent,
        packets_recv,
        errors_in,
        errors_out,
    }
}

fn collect_process_metrics(sys: &System) -> Vec<ProcessMetrics> {
    sys.processes()
        .iter()
        .take(10) // 只取前 10 个进程
        .map(|(pid, process)| ProcessMetrics {
            pid: pid.as_u32() as i32,
            name: process.name().to_string_lossy().to_string(),
            cpu_usage: process.cpu_usage() as f64,
            memory: process.memory(),
            status: format!("{:?}", process.status()),
        })
        .collect()
}

fn collect_system_info(_sys: &System) -> SystemInfo {
    SystemInfo {
        os_name: System::name().unwrap_or_else(|| "Unknown".to_string()),
        os_version: System::os_version().unwrap_or_else(|| "Unknown".to_string()),
        kernel_version: System::kernel_version().unwrap_or_else(|| "Unknown".to_string()),
        arch: System::cpu_arch().unwrap_or_else(|| "Unknown".to_string()),
        uptime: System::uptime(),
    }
}
