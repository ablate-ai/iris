# Iris 安装指南

## 一键安装

### 快速安装（推荐）

```bash
curl -fsSL https://raw.githubusercontent.com/ablate-ai/iris/main/install.sh | bash
```

### 安全安装（先下载再执行）

```bash
curl -fsSL https://raw.githubusercontent.com/ablate-ai/iris/main/install.sh -o install.sh
chmod +x install.sh
./install.sh
```

## 自定义安装

### 安装指定版本

```bash
curl -fsSL https://raw.githubusercontent.com/ablate-ai/iris/main/install.sh | VERSION=v0.1.0 bash
```

### 只安装 Agent

```bash
curl -fsSL https://raw.githubusercontent.com/ablate-ai/iris/main/install.sh | INSTALL_SERVER=false bash
```

### 只安装 Server

```bash
curl -fsSL https://raw.githubusercontent.com/ablate-ai/iris/main/install.sh | INSTALL_AGENT=false bash
```

### 安装到自定义目录

```bash
curl -fsSL https://raw.githubusercontent.com/ablate-ai/iris/main/install.sh | INSTALL_DIR=~/.local/bin bash
```

### 组合使用

```bash
# 安装 v0.2.0 版本的 agent 到 ~/.local/bin
curl -fsSL https://raw.githubusercontent.com/ablate-ai/iris/main/install.sh | \
  VERSION=v0.2.0 INSTALL_SERVER=false INSTALL_DIR=~/.local/bin bash
```

## 环境变量

| 变量 | 说明 | 默认值 |
|------|------|--------|
| `VERSION` | 安装版本号 | `latest` |
| `INSTALL_DIR` | 安装目录 | `/usr/local/bin` |
| `INSTALL_AGENT` | 是否安装 agent | `true` |
| `INSTALL_SERVER` | 是否安装 server | `true` |

## 支持的平台

- **Linux**: x86_64, ARM64
- **macOS**: x86_64 (Intel), ARM64 (Apple Silicon)
- **Windows**: x86_64

## 手动安装

如果一键安装脚本不适用，可以手动下载：

1. 访问 [Releases 页面](https://github.com/ablate-ai/iris/releases)
2. 下载对应平台的压缩包
3. 解压并移动到 PATH 目录：

```bash
# Linux/macOS
tar -xzf iris-linux-amd64.tar.gz
sudo mv iris-agent iris-server /usr/local/bin/

# Windows (PowerShell)
Expand-Archive iris-windows-amd64.zip
Move-Item iris-agent.exe, iris-server.exe C:\Windows\System32\
```

## 验证安装

```bash
# 检查版本
iris-agent --version
iris-server --version

# 查看帮助
iris-agent --help
iris-server --help
```

## 卸载

```bash
# 删除二进制文件
sudo rm /usr/local/bin/iris-agent
sudo rm /usr/local/bin/iris-server

# 或者自定义安装目录
rm ~/.local/bin/iris-agent
rm ~/.local/bin/iris-server
```

## 故障排查

### 权限错误

如果遇到权限错误，可以安装到用户目录：

```bash
curl -fsSL https://raw.githubusercontent.com/ablate-ai/iris/main/install.sh | \
  INSTALL_DIR=~/.local/bin bash
```

然后确保 `~/.local/bin` 在 PATH 中：

```bash
echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.bashrc
source ~/.bashrc
```

### 下载失败

如果 GitHub 访问较慢，可以：

1. 使用代理
2. 手动下载后本地安装
3. 使用镜像站点（如果有）

### 命令未找到

确保安装目录在 PATH 中：

```bash
echo $PATH
```

如果不在，添加到 shell 配置文件：

```bash
# Bash
echo 'export PATH="$PATH:/usr/local/bin"' >> ~/.bashrc

# Zsh
echo 'export PATH="$PATH:/usr/local/bin"' >> ~/.zshrc
```

## 更新

重新运行安装脚本即可更新到最新版本：

```bash
curl -fsSL https://raw.githubusercontent.com/ablate-ai/iris/main/install.sh | bash
```
