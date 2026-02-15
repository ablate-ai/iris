# CPU 信息获取调试指南

## 问题描述

Iris 项目在 Debian 服务器上无法正确识别 CPU 型号和频率信息，显示为 N/A。

## sysinfo 库在 Linux 上的实现原理

根据 [sysinfo 源码](https://github.com/GuillaumeGomez/sysinfo/blob/master/src/unix/linux/cpu.rs)，在 Linux 平台上获取 CPU 信息的方式：

### CPU 型号 (brand)
- 从 `/proc/cpuinfo` 读取 `model name` 字段
- 对于 ARM 处理器，使用 `CPU implementer` 和 `CPU part` 映射

### CPU 频率 (frequency)
按优先级尝试以下方法：
1. **优先方案**：读取 `/sys/devices/system/cpu/cpu{index}/cpufreq/scaling_cur_freq`（除以 1000 转为 MHz）
2. **备用方案**：从 `/proc/cpuinfo` 解析 `cpu MHz`、`CPU MHz`、`BogoMIPS` 等字段
3. **默认值**：如果都失败则返回 0

## 诊断步骤

### 1. 在 Debian 服务器上运行诊断脚本

```bash
cat > /tmp/check_cpu_info.sh << 'EOF'
#!/bin/bash
echo "=== 检查 CPU 信息来源 ==="
echo ""
echo "1. /proc/cpuinfo 中的 model name:"
grep "model name" /proc/cpuinfo | head -1
echo ""
echo "2. /proc/cpuinfo 中的 cpu MHz:"
grep "cpu MHz" /proc/cpuinfo | head -1
echo ""
echo "3. cpufreq 目录是否存在:"
ls -la /sys/devices/system/cpu/cpu0/cpufreq/ 2>&1 | head -5
echo ""
echo "4. scaling_cur_freq 文件:"
cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq 2>&1
echo ""
echo "5. 完整的 /proc/cpuinfo (前 30 行):"
head -30 /proc/cpuinfo
EOF

chmod +x /tmp/check_cpu_info.sh
/tmp/check_cpu_info.sh
```

### 2. 预期输出

正常情况下应该看到：
```
1. /proc/cpuinfo 中的 model name:
model name      : AMD EPYC 9655 96-Core Processor

2. /proc/cpuinfo 中的 cpu MHz:
cpu MHz         : 2596.096

3. cpufreq 目录是否存在:
[目录列表或错误信息]

4. scaling_cur_freq 文件:
[频率值或错误信息]
```

## 可能的问题原因

### 1. 虚拟化环境限制
- 某些虚拟化平台（如 KVM、OpenVZ）可能不暴露 `/sys/devices/system/cpu/cpu0/cpufreq/` 目录
- 虚拟机中的 CPU 信息可能被虚拟化层过滤

### 2. 权限问题
- 某些系统文件需要 root 权限才能读取
- 容器环境可能限制了对硬件信息的访问

### 3. sysinfo 版本问题
- 当前使用的是 sysinfo 0.32 版本
- 可能存在已知 bug 或平台兼容性问题

## 已实施的修改

### 1. Proto 定义更新 (`proto/probe.proto`)
```protobuf
message SystemInfo {
  string os_name = 1;
  string os_version = 2;
  string kernel_version = 3;
  string arch = 4;
  uint64 uptime = 5;
  string cpu_model = 6;         // 新增
  double cpu_frequency = 7;     // 新增
  string hostname = 8;          // 新增
}
```

### 2. 采集器更新 (`agent/src/collector.rs`)
```rust
fn collect_system_info(sys: &System) -> SystemInfo {
    // 获取 CPU 信息
    let (cpu_model, cpu_frequency) = if let Some(cpu) = sys.cpus().first() {
        (
            cpu.brand().to_string(),
            cpu.frequency() as f64,
        )
    } else {
        ("Unknown".to_string(), 0.0)
    };

    let hostname = System::host_name().unwrap_or_else(|| "Unknown".to_string());

    SystemInfo {
        os_name: System::name().unwrap_or_else(|| "Unknown".to_string()),
        os_version: System::os_version().unwrap_or_else(|| "Unknown".to_string()),
        kernel_version: System::kernel_version().unwrap_or_else(|| "Unknown".to_string()),
        arch: System::cpu_arch().unwrap_or_else(|| "Unknown".to_string()),
        uptime: System::uptime(),
        cpu_model,
        cpu_frequency,
        hostname,
    }
}
```

### 3. Web 界面更新 (`web/index.html`)
```javascript
<div className="info-item">
    <div className="info-item-label">CPU 型号</div>
    <div className="info-item-value">{sys.system_info.cpu_model || 'N/A'}</div>
</div>
<div className="info-item">
    <div className="info-item-label">CPU 频率</div>
    <div className="info-item-value">{sys.system_info.cpu_frequency?.toFixed(0) || 'N/A'} MHz</div>
</div>
```

## 下一步行动

1. **运行诊断脚本**：在 Debian 服务器上执行上述脚本，收集系统信息
2. **重新编译部署**：
   ```bash
   make build-agent
   # 将编译好的 iris-agent 部署到 Debian 服务器
   ```
3. **测试验证**：启动 agent 并检查 Web 界面是否正确显示 CPU 信息
4. **如果仍然失败**：考虑直接读取 `/proc/cpuinfo` 作为备用方案

## 解决方案：已实施

已添加备用方案，当 sysinfo 库无法获取 CPU 信息时，直接读取 `/proc/cpuinfo`：

```rust
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
```

这个备用方案会在以下情况下自动启用：
- sysinfo 返回空字符串或 "Unknown"
- CPU 频率为 0

## 部署步骤

1. **编译 agent**：
   ```bash
   cargo build --release --bin iris-agent
   ```

2. **部署到 Debian 服务器**：
   ```bash
   # 将 target/release/iris-agent 复制到服务器
   scp target/release/iris-agent user@debian-server:/path/to/deploy/
   ```

3. **重启 agent**：
   ```bash
   # 在 Debian 服务器上
   systemctl restart iris-agent
   # 或者直接运行
   ./iris-agent --server http://server-ip:50051 --interval 1
   ```

4. **验证**：
   - 打开 Web 界面
   - 点击 agent 卡片查看详情
   - 确认 "CPU 型号" 显示为 "AMD EPYC 9655 96-Core Processor"
   - 确认 "CPU 频率" 显示为 "2596 MHz"

## 预期结果

根据你的 Debian 服务器信息，应该显示：
- **CPU 型号**：AMD EPYC 9655 96-Core Processor
- **CPU 频率**：2596 MHz
- **主机名**：DMIT-3EpmMYJ7GY（或从 hostname 命令获取）

## 备用方案：直接读取 /proc/cpuinfo

如果 sysinfo 库无法获取信息，可以实现一个备用函数直接解析 `/proc/cpuinfo`：

```rust
#[cfg(target_os = "linux")]
fn get_cpu_info_from_proc() -> (String, f64) {
    use std::fs;

    if let Ok(content) = fs::read_to_string("/proc/cpuinfo") {
        let mut model = String::from("Unknown");
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

            if !model.is_empty() && freq > 0.0 {
                break;
            }
        }

        return (model, freq);
    }

    ("Unknown".to_string(), 0.0)
}
```

## 参考资料

- [sysinfo Rust Guide](https://generalistprogrammer.com/tutorials/sysinfo-rust-crate-guide)
- [sysinfo GitHub - Linux CPU implementation](https://github.com/GuillaumeGomez/sysinfo/blob/master/src/unix/linux/cpu.rs)
- [sysinfo-cli documentation](https://lib.rs/crates/sysinfo-cli)
