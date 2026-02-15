use common::proto::{
    CpuMetrics, DiskMetrics, MemoryMetrics, NetworkMetrics, ProcessMetrics, SystemMetrics,
    SystemInfo,
};
use sysinfo::{System, Networks, Disks};

/// 采集系统指标
pub fn collect_metrics() -> SystemMetrics {
    let mut sys = System::new_all();
    sys.refresh_all();

    SystemMetrics {
        cpu: Some(collect_cpu_metrics(&sys)),
        memory: Some(collect_memory_metrics(&sys)),
        disks: collect_disk_metrics(),
        network: Some(collect_network_metrics()),
        processes: collect_process_metrics(&sys),
        system_info: Some(collect_system_info(&sys)),
    }
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
