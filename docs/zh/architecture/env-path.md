# 环境选择与环境路径确定规则

本文档定义 epkg 环境选择机制、路径确定规则，适用于所有执行场景。

## 环境选择机制

### 机制 1：显式 CLI 参数 (-e/-r)

**语义**：用户明确指定要使用的环境。

**来源**：命令行参数

**格式**：
```bash
epkg -e <NAME> <command>   # 指定环境名
epkg --root <DIR> <command> # 指定环境根目录
```

**优先级**：最高，不可被任何其他机制覆盖

### 机制 2：EPKG_ACTIVE_ENV 环境变量

**语义**：当用户未在 CLI 中指定 -e/-r 时，EPKG_ACTIVE_ENV **等同于**用户显式指定 `-e $EPKG_ACTIVE_ENV`。

**来源**：
- 用户执行 `eval "$(epkg env activate myenv)"` 设置
- 测试脚本显式设置（模拟用户行为）

**重要**：Rust 代码**不**自动设置 EPKG_ACTIVE_ENV。即使 VM guest 模式，也不从 `-e` 参数自动转换。EPKG_ACTIVE_ENV 只反映用户需求。

**格式**：
```bash
EPKG_ACTIVE_ENV=main           # 单环境
EPKG_ACTIVE_ENV=env2:env1      # 环境栈（最新激活在前）
EPKG_ACTIVE_ENV=main!          # Pure 模式（环境隔离）
```

**优先级**：低于 CLI (-e/-r)，高于所有其他机制

**关键语义**：
```
EPKG_ACTIVE_ENV 表示用户需求，而非内部 helper。
用户理解：EPKG_ACTIVE_ENV = "我想用的环境"
实现：EPKG_ACTIVE_ENV 优先级高于 registered search、/etc/epkg/env.yaml 等
```

### 机制 3：Registered Environment Search

**语义**：用户运行命令时，在已注册环境中查找该命令。

**来源**：用户通过 `epkg env register` 注册的环境

**适用范围**：
- 仅对 `epkg run <command>` 有效
- 仅当 `-e/-r` 未指定 **且** `EPKG_ACTIVE_ENV` 未设置时生效

**优先级**：低于 EPKG_ACTIVE_ENV

### 机制 4：.eenv 目录发现

**语义**：项目目录中的 `.eenv` 标记项目环境。

**适用范围**：
- 仅对 `epkg run <path>` 有效（命令是路径，如 `./script.sh`）
- 仅当 `-e/-r` 未指定 **且** `EPKG_ACTIVE_ENV` 未设置时生效

**优先级**：与 Registered Search 同级（针对不同命令类型）

### 机制 5：/etc/epkg/env.yaml

**语义**：当前 rootfs 是某个环境（namespace/VM guest）。

**适用范围**：
- 仅当 `-e/-r` 未指定 **且** `EPKG_ACTIVE_ENV` 未设置时生效

### 机制 6：MAIN_ENV

**语义**：最终默认环境。

**优先级**：最低

## 环境选择优先级

```
优先级    机制              适用命令
─────────────────────────────────────────────────
最高      -e/-r CLI        所有命令
高        EPKG_ACTIVE_ENV  所有命令（未指定 -e/-r 时）
中        .eenv            epkg run <path>（无 EPKG_ACTIVE_ENV）
中        Registered       epkg run <name>（无 EPKG_ACTIVE_ENV）
低        /etc/epkg/env.yaml  所有命令（无 EPKG_ACTIVE_ENV）
最低      MAIN_ENV         所有命令（无其他机制）
```

**统一规则**：
- 非 `epkg run` 命令：仅使用 -e/-r 和 EPKG_ACTIVE_ENV，or fallback to MAIN_ENV
- `epkg run` 命令：以上基础上，增加两个环节：.eenv search + registered paths search

## 环境选择流程

```
┌─────────────────────────────────────────────────────┐
│              用户执行 epkg 命令                      │
└─────────────────────────────────────────────────────┘
                        │
                        ▼
           ┌────────────────────────┐
           │ CLI 指定了 -e/-r?       │
           └────────────────────────┘
                │ Yes → 使用指定环境
                │ No
                ▼
           ┌────────────────────────┐
           │ EPKG_ACTIVE_ENV 设置?  │
           └────────────────────────┘
                │ Yes → 使用 EPKG_ACTIVE_ENV
                │ No
                ▼
           ┌────────────────────────┐
           │ 命令是 epkg run?       │
           └────────────────────────┘
                │ No → go to MAIN_ENV fallback
                │ Yes
                ▼
           ┌────────────────────────┐
           │ 命令是路径? (.eenv)    │
           │ 有项目环境? (.eenv)    │
           │ 注册环境? (Registered) │
           └────────────────────────┘
                │ 发现 → 使用发现的环境
                │ No
                ▼
           ┌────────────────────────┐
           │ /etc/epkg/env.yaml?    │
           └────────────────────────┘
                │ Yes → 使用该环境
                │ No
                ▼
           ┌────────────────────────┐
           │ MAIN_ENV fallback      │
           └────────────────────────┘
```

## 内部 Helper 变量

以下变量由 Rust 代码设置，不反映用户需求：

### EPKG_USER

**语义**：原始用户名，用于路径计算。

**来源**：Rust 代码自动设置（检测当前用户或从 VM 传递）

**用途**：
- 计算 host 侧 home 目录：`/home/$EPKG_USER`
- 在 VM guest 中使用 host 的路径布局

**实现建议**：
```rust
// dirs.rs 中使用 EPKG_USER 替代 USER
let user = env::var("EPKG_USER").unwrap_or_else(|| env::var("USER").unwrap_or("root"));
let home_epkg = PathBuf::from(format!("/home/{}/.epkg", user));
```

### EPKG_SHARED_STORE

**语义**：传递 shared_store 配置到 VM guest。

**来源**：Rust 代码自动设置（读取 host 配置）

**用途**：VM guest 使用与 host 一致的 layout

**注意**：这是配置传递，非用户意图

## 路径一致性方案（所有模式）

### 核心思想

Mount host 路径到 sandbox/guest 的**相同路径**，而非映射到 root 的路径。
这样 env.yaml 中的 `env_root` 主机路径在所有模式下都可访问，无需路径转换。

### 问题

在 Fs 模式（pivot_root）和 VM 模式（virtiofs）中，进程的 root 变为 env root，
主机路径（如 `/home/wfg/.epkg/envs/xxx`）不再可直接访问。

Env 模式（bind mounts）虽然主机文件系统仍然可见，但如果错误地将 `env_root`
简化为 `/`，会导致 generation 写入主机的 `/generations/` tmpfs 而非 env 的
generations 目录。

### 解决：路径一致性 Mount

在所有模式中将 `~/.epkg` 等主机路径 mount 到**相同路径**：

| 模式 | 实现 | 路径访问方式 |
|------|------|-------------|
| Env | 已隐式可见（主机文件系统未隔离） | 主机路径直接可用 |
| Fs (pivot_root) | 先 bind mount 主机路径到 `$env_root/相同路径`，再 pivot_root | pivot_root 后 mount 保留 |
| VM (virtiofs) | 先 bind mount 主机路径到 `$env_root/相同路径`，再通过 virtiofs 共享 | VM 内路径可用 |

**实现**（`src/namespace.rs` 中的 `add_epkg_mount_spec_strings()`)：
```rust
// 将主机路径 bind mount 到 env_root 下的相同路径
/home/wfg/.epkg      : $env_root/home/wfg/.epkg      # 路径一致性！
/home/wfg/.cache/epkg: $env_root/home/wfg/.cache/epkg
/opt/epkg            : $env_root/opt/epkg
```

Fs 模式 pivot_root 前执行这些 bind mount，pivot_root 后 mount 保留，路径仍然可访问。
VM 模式 virtiofs 共享前执行这些 bind mount，VM 内路径仍然可访问。

**结果**：
- `env_root: /home/wfg/.epkg/envs/xxx` 在**所有模式**下有效
- 代码只需使用 env.yaml 中的真实 `env_root`，无需简化为 `/`
- 无需模式判断，无需路径转换，无需 EPKG_ENV_ROOT

### EPKG_ENV_ROOT 的作用

**当前状态**：workaround，当上述方案未完全实现时使用

**语义**：内部 helper，提供 VM guest 侧的 env_root 路径

**理想状态**：移除，通过路径一致性 mount + EPKG_USER 解决

## 数据结构

### env_name_explicit

**定义位置**：`src/models.rs` - `EPKGConfig.common.env_name_explicit`

**语义**：CLI 参数 `-e NAME` 是否被使用

**用途**：
- 阻止其他机制覆盖用户显式选择
- 确定是否执行 registered/.eenv search

```rust
pub struct EPKGConfigCommon {
    pub env_name: String,
    pub env_name_explicit: bool,  // -e 使用时为 true
    ...
}
```

## 场景详解

### 场景 1：普通用户 Host 操作

```bash
epkg -e myenv install jq        # 显式指定
epkg install jq                 # 报错"请指定环境"
```

### 场景 2：Shell 激活环境

```bash
eval "$(epkg env activate myenv)"
# EPKG_ACTIVE_ENV=myenv

epkg install jq                 # 安装到 myenv（等同于 -e myenv）
epkg -e other install jq        # 安装到 other（ignore EPKG_ACTIVE_ENV）
```

### 场景 3：项目目录 .eenv

```bash
# 无 EPKG_ACTIVE_ENV
cd /project/src
epkg run ./script.sh            # 使用 .eenv 环境

# 有 EPKG_ACTIVE_ENV
EPKG_ACTIVE_ENV=myenv
epkg run ./script.sh            # 使用 myenv（不扫描 .eenv ）
```

### 场景 4：Registered Environment Search

```bash
# 无 EPKG_ACTIVE_ENV
epkg run jq --version           # 搜索 registered 环境

# 有 EPKG_ACTIVE_ENV
EPKG_ACTIVE_ENV=myenv
epkg run jq --version           # 使用 myenv（不搜索 registered）
```

### 场景 5：VM Guest 执行

```bash
# Host 设置（Rust 自动设置，非 test harness）
EPKG_USER=wfg                   # host 用户名
EPKG_SHARED_STORE=false         # host 配置
EPKG_ACTIVE_ENV=myenv           # 用户意图传递

# Guest mount（路径一致性）
/home/wfg/.epkg:/home/wfg/.epkg

# Guest 执行
epkg install jq                 # 安装到 myenv
# 配置文件路径 /home/wfg/.epkg/envs/myenv 在 guest 中有效
```

## 规则总结

| 规则 | 说明 |
|------|------|
| 规则 1 | CLI (-e/-r) 最高优先级 |
| 规则 2 | EPKG_ACTIVE_ENV = 未指定 -e/-r 时的用户意图 |
| 规则 3 | 非 `epkg run` 无用户需求时, simply fall back to MAIN_ENV |
| 规则 4 | `epkg run` 可搜索 .eenv/Registered paths |
| 规则 5 | EPKG_USER 用于路径一致性，非用户意图 |
| 规则 6 | VM 用路径一致性 mount + EPKG_USER，而非路径转换 |

## 相关代码

| 函数/结构 | 文件 | 说明 |
|-----------|------|------|
| `env_name_explicit` | src/models.rs | CLI -e 使用标记 |
| `determine_environment_final()` | src/main.rs | 环境选择流程 |
| `get_env_root()` | src/dirs.rs | env_root 计算 |
| `get_home()` | src/dirs.rs | 支持 EPKG_HOME/EPKG_USER |
| `EPKG_USER/EPKG_HOME` | src/run.rs | VM 路径一致性 |

## 已完成改进

1. **移除 EPKG_ENV_ROOT** ✓：使用路径一致性 mount + EPKG_HOME/EPKG_USER 替代
2. **dirs.rs 支持 EPKG_HOME/EPKG_USER** ✓：计算 host 侧路径
3. **vm.sh mount 到相同路径** ✓：`$HOME/.epkg:/home/$EPKG_USER/.epkg`

## 待完善项

1. **更多测试脚本适配 EPKG_HOME**

## VM 模式 Mounts 统一方案

### 问题

不同 VMM backend 的 virtiofs mounts 方案不一致：
- QEMU: 只共享 env_root，VM 内看不到 host 的 home_epkg/home_cache/opt_epkg
- libkrun: 添加额外的 virtiofs mounts，但需要为每个目录启动独立的 virtiofs device

### 统一方案

**Linux (QEMU + libkrun)**：
- 使用 bind mounts 将 home_epkg/home_cache/opt_epkg 挂载到 env_root 内对应路径
- 然后启动单个 virtiofs 共享 env_root
- VM guest 自然能看到这些目录
- 需要 CAP_SYS_ADMIN capability 执行 bind mounts

**macOS/Windows (libkrun)**：
- 不支持 bind mounts
- 使用多个 virtiofs mounts（每个目录一个）
- 通过 kernel cmdline `epkg.vol_N=tag:guest_path[:ro]` 配置
- guest init 在启动时挂载这些 volumes

### 实现位置

| 函数 | 文件 | 说明 |
|------|------|------|
| `vm_bind_mount_spec_strings()` | src/namespace.rs | 生成 bind mount spec strings |
| `setup_qemu_vm()` | src/qemu.rs | TODO: 调用 bind mounts |
| `run_command_in_krun()` | src/libkrun/core.rs | TODO: Linux 上调用 bind mounts |
| `build_virtiofs_mount_specs()` | src/libkrun/core.rs | macOS/Windows 生成多个 virtiofs mounts |
| `mount_virtiofs_volumes()` | src/busybox/init.rs | guest init 挂载 virtiofs volumes |

### 路径转换

当 host 命令路径（如 `/home/wfg/.epkg/envs/myenv/usr/bin/env`）发送给 VM guest 时，需要转换为 guest 内路径（`/usr/bin/env`）：

| 函数 | 文件 | 说明 |
|------|------|------|
| `convert_host_path_to_guest_path()` | src/namespace.rs | namespace 路径的路径转换 |
| `run.rs:791-804` | src/run.rs | VM 直接调用时的路径转换 |

转换逻辑：
```rust
let guest_cmd_path = cmd_path.strip_prefix(env_root)
    .map(|rel| PathBuf::from(format!("/{}", rel)))
    .unwrap_or_else(|_| cmd_path.clone());
```

### virtiofs 挂载机制

virtiofs 将 env_root 挂载为 VM 根目录 `/`：
- Host: `/home/wfg/.epkg/envs/myenv`
- Guest: `/` (virtiofs mount point)
- 路径映射: `/home/wfg/.epkg/envs/myenv/usr/bin/env` → `/usr/bin/env`
