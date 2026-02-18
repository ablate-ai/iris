use common::proto::{
    AgentMetrics, CpuMetrics, DiskMetrics, MemoryMetrics, NetworkMetrics, SystemInfo,
    SystemMetrics,
};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Instant;
use sysinfo::{
    Disks, MemoryRefreshKind, Networks, Pid, ProcessRefreshKind, ProcessesToUpdate, System,
    MINIMUM_CPU_UPDATE_INTERVAL,
};

// 全局统计
static METRICS_SENT: AtomicU64 = AtomicU64::new(0);
static ERRORS_COUNT: AtomicU64 = AtomicU64::new(0);

// 探针启动时间
static AGENT_START_TIME: once_cell::sync::Lazy<Instant> = once_cell::sync::Lazy::new(Instant::now);

// 全局 System 实例，用于保持 CPU 使用率采集的状态
static SYSTEM: once_cell::sync::Lazy<Mutex<System>> = once_cell::sync::Lazy::new(|| {
    let mut sys = System::new_all();
    // 第一次刷新，为后续采集做准备
    sys.refresh_cpu_usage();
    sys.refresh_memory_specifics(MemoryRefreshKind::everything());
    Mutex::new(sys)
});

// 标记是否已经完成初始化等待
static CPU_INITIALIZED: AtomicBool = AtomicBool::new(false);

/// 采集系统指标
pub fn collect_metrics() -> SystemMetrics {
    let start = Instant::now();

    // 第一次采集时，需要等待 MINIMUM_CPU_UPDATE_INTERVAL 以获取准确的 CPU 使用率
    if !CPU_INITIALIZED.load(Ordering::Relaxed) {
        std::thread::sleep(MINIMUM_CPU_UPDATE_INTERVAL);
        CPU_INITIALIZED.store(true, Ordering::Relaxed);
    }

    let mut sys = SYSTEM.lock().unwrap();

    // 刷新 CPU 使用率（需要两次刷新之间的差值）
    sys.refresh_cpu_usage();

    // 刷新其他系统信息
    sys.refresh_memory_specifics(MemoryRefreshKind::everything());
    let collection_time_ms = start.elapsed().as_millis() as u64;

    SystemMetrics {
        cpu: Some(collect_cpu_metrics(&sys)),
        memory: Some(collect_memory_metrics(&sys)),
        disks: collect_disk_metrics(),
        network: Some(collect_network_metrics()),
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
        ProcessRefreshKind::everything(),
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
    #[cfg(target_os = "linux")]
    if let Some((total, used, available, swap_total, swap_used)) = read_memory_info_from_proc() {
        let usage_percent = if total > 0 {
            (used as f64 / total as f64) * 100.0
        } else {
            0.0
        };

        return MemoryMetrics {
            total,
            used,
            available,
            usage_percent,
            swap_total,
            swap_used,
        };
    }

    let total = sys.total_memory();
    let used = sys.used_memory();
    let available = sys.available_memory();
    let usage_percent = if total > 0 {
        (used as f64 / total as f64) * 100.0
    } else {
        0.0
    };

    MemoryMetrics {
        total,
        used,
        available,
        usage_percent,
        swap_total: sys.total_swap(),
        swap_used: sys.used_swap(),
    }
}

#[cfg(target_os = "linux")]
fn read_memory_info_from_proc() -> Option<(u64, u64, u64, u64, u64)> {
    use std::fs;

    let content = fs::read_to_string("/proc/meminfo").ok()?;
    let mut mem_total = 0u64;
    let mut mem_available = 0u64;
    let mut swap_total = 0u64;
    let mut swap_free = 0u64;

    for line in content.lines() {
        if line.starts_with("MemTotal:") {
            mem_total = parse_meminfo_kib(line)?;
        } else if line.starts_with("MemAvailable:") {
            mem_available = parse_meminfo_kib(line)?;
        } else if line.starts_with("SwapTotal:") {
            swap_total = parse_meminfo_kib(line)?;
        } else if line.starts_with("SwapFree:") {
            swap_free = parse_meminfo_kib(line)?;
        }
    }

    if mem_total == 0 {
        return None;
    }

    let used = mem_total.saturating_sub(mem_available);
    let swap_used = swap_total.saturating_sub(swap_free);
    Some((mem_total, used, mem_available, swap_total, swap_used))
}

#[cfg(target_os = "linux")]
fn parse_meminfo_kib(line: &str) -> Option<u64> {
    let value_kib = line.split_whitespace().nth(1)?.parse::<u64>().ok()?;
    Some(value_kib.saturating_mul(1024))
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
                read_bytes: 0, // sysinfo 不直接提供，需要其他方式
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

fn collect_system_info(sys: &System) -> SystemInfo {
    // 获取 CPU 信息
    let (cpu_model, cpu_frequency) = if let Some(cpu) = sys.cpus().first() {
        (cpu.brand().to_string(), cpu.frequency() as f64)
    } else {
        ("Unknown".to_string(), 0.0)
    };

    // 如果 sysinfo 获取失败（返回空字符串或 0），尝试直接读取 /proc/cpuinfo（仅 Linux）
    #[cfg(target_os = "linux")]
    let (cpu_model, cpu_frequency) = {
        if cpu_model.is_empty() || cpu_model == "Unknown" || cpu_frequency == 0.0 {
            if let Some((model, freq)) = read_cpu_info_from_proc() {
                let final_model = if !model.is_empty() && cpu_model == "Unknown" {
                    model
                } else {
                    cpu_model
                };
                let final_freq = if freq > 0.0 && cpu_frequency == 0.0 {
                    freq
                } else {
                    cpu_frequency
                };
                (final_model, final_freq)
            } else {
                (cpu_model, cpu_frequency)
            }
        } else {
            (cpu_model, cpu_frequency)
        }
    };

    // 获取主机名（优先使用环境变量 IRIS_HOSTNAME）
    let hostname = std::env::var("IRIS_HOSTNAME")
        .ok()
        .or_else(|| System::host_name())
        .unwrap_or_else(|| "Unknown".to_string());

    SystemInfo {
        os_name: System::name().unwrap_or_else(|| "Unknown".to_string()),
        os_version: System::os_version().unwrap_or_else(|| "Unknown".to_string()),
        kernel_version: System::kernel_version().unwrap_or_else(|| "Unknown".to_string()),
        arch: System::cpu_arch(),
        uptime: System::uptime(),
        cpu_model,
        cpu_frequency,
        hostname,
    }
}

/// 直接从 /proc/cpuinfo 读取 CPU 信息（Linux 备用方案）
#[cfg(target_os = "linux")]
fn read_cpu_info_from_proc() -> Option<(String, f64)> {
    use std::fs;

    let content = fs::read_to_string("/proc/cpuinfo").ok()?;
    let mut model = String::new();
    let mut freq = 0.0;

    for line in content.lines() {
        if line.starts_with("model name") {
            if let Some(value) = line.split(':').nth(1) {
                model = value.trim().to_string();
            }
        } else if line.starts_with("cpu MHz") {
            if let Some(value) = line.split(':').nth(1) {
                freq = value.trim().parse().unwrap_or(0.0);
            }
        }

        // 找到两个值就可以退出了
        if !model.is_empty() && freq > 0.0 {
            break;
        }
    }

    if !model.is_empty() || freq > 0.0 {
        Some((model, freq))
    } else {
        None
    }
}
