# 入门指南

本指南将引导您安装 epkg 并运行您的第一个包操作。

## 安装 epkg

**Linux、macOS、Windows WSL2：**
```bash
curl -fsSL https://raw.atomgit.com/openeuler/epkg/raw/master/bin/install.sh | bash
```

**Windows (PowerShell)：**
```powershell
irm https://raw.atomgit.com/openeuler/epkg/raw/master/bin/install.ps1 | iex
```

然后启动一个新的 shell 以更新 PATH。

## 第一个环境和包

默认情况下，您有一个 `main` 环境。使用特定 channel 创建另一个环境并在那里安装包。

1. **创建一个环境**（例如 Alpine）：

   ```bash
   epkg env create alpine -c alpine
   ```

   示例输出：

   ```
   在 $HOME/.epkg/envs/alpine 中创建环境 'alpine'
   ```

2. **在该环境中安装包**：

   ```bash
   epkg -e alpine install bash jq
   ```

   您将看到依赖计划和下载/安装进度。示例摘要：

   ```
   Packages to be freshly installed:
   DEPTH       SIZE  PACKAGE
   0       469.7 KB  bash__5.3.3-r1__x86_64
   0       147.9 KB  jq__1.8.1-r0__x86_64
   0       520.1 KB  coreutils__9.8-r1__x86_64
   ...
   Packages to be exposed:
   - jq__1.8.1-r0__x86_64
   - bash__5.3.3-r1__x86_64
   0 upgraded, 19 newly installed, 0 to remove, 2 to expose, 0 to unexpose.
   Need to get 4.6 MB archives.
   After this operation, 11.0 MB of additional disk space will be used.
   ```

3. **从该环境运行命令**：

   ```bash
   epkg -e alpine run jq --version
   # 例如 jq-1.8.1

   # 提供三种隔离模式：
   epkg -e alpine run --isolate=env bash  # 默认：bind mount
   epkg -e alpine run --isolate=fs bash   # 基于 chroot 的隔离
   epkg -e alpine run --isolate=vm bash   # 基于 VM 的隔离
   ```

   或者注册环境，以便其二进制文件在您的 PATH 上供日常 CLI 使用：

   ```bash
   epkg env register alpine
   # epkg() 将自动运行：eval "$(epkg env path)" 以在当前 shell 中更新 PATH
   jq --version
   ```

## 验证安装

- 列出环境：`epkg env list`
- 列出环境中的包：`epkg -e alpine list`
- 显示环境 PATH：`epkg env path`

## 常见工作流

### 使用来自不同发行版的包

为不同的 channel 创建单独的环境并注册它们：

```bash
epkg env create debian-env -c debian
epkg env create alpine-env -c alpine-3.23
epkg env register debian-env
epkg env register alpine-env
# 现在两个环境的二进制文件都在 PATH 中，按注册顺序
export PATH="$HOME/.epkg/envs/debian-env/ebin:$HOME/.epkg/envs/alpine-env/ebin:..."
```

### 项目特定环境

对于需要特定包的项目，在项目根目录创建环境：

```bash
cd /path/to/myproject
epkg env create --root ./.eenv -c alpine
epkg --root ./.eenv install py3-pip  # 如果您在项目目录下，--root 是可选的
epkg run ./script.py
```

下一步：[环境管理](environments.md)、[包操作](package-operations.md)、[高级用法](advanced.md)。
