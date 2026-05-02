# VM Session 架构

## 设计原则

### 核心原则

1. **统一执行路径**: `epkg vm start` 和 `epkg run --isolate=vm` 必须使用相同的底层代码 `fork_and_execute()`。
   - VM 启动逻辑统一，避免代码分歧
   - 两个命令行为一致，便于维护和调试

2. **统一 Session 管理**: VM session 管理机制完全相同。
   - `epkg vm list` 显示所有活跃的 VM session，无论是 `vm start` 还是 `run --isolate=vm` 启动的
   - Session 文件格式、位置、发现机制统一
   - Timeout/extend 等配置语义一致

3. **自动复用优先**: VM 模式默认检测并复用现有 session。
   - 无需任何 flag，自动检测并复用
   - Session file 是跨进程协调的唯一机制
   - 一个 env_root 只有一个 VM 实例（ONE VM per env_root）

4. **vm_keep_timeout 控制生命周期**:
   - `vm_keep_timeout` 是控制 VM 存活时间的**唯一**参数
   - 有 timeout: VM 空闲 timeout 秒后关闭
   - 无 timeout: 命令完成后立即关闭
   - timeout=0: 永不关闭

## 问题背景

pir.py 连续调用多个 epkg run 命令，每次都有 ~2-3s VM 启动开销（macOS libkrun backend）。

**根本原因**：libkrun VM 是 host 进程内的线程，进程退出时 VM 立即死亡。

**解决方案**：新增 `epkg vm` 子命令，VM keeper 进程独立运行，支持跨进程 VM 复用。

### Windows/WHPX 时序问题

epkg 在 Windows/WHPX 环境下遇到 vsock 握手时序问题：VM 在启动后约 4.4 秒时关闭。根本原因是 Windows/WHPX 的 vsock 握手是异步的，Host 在 Guest 还未完成初始化时就尝试连接。

**解决方案**：统一在所有平台使用 forward mode + ready port 架构。

## 概述

VM session 文件系统确保每个 env_root 只有一个活跃的 VM，支持跨进程 VM reuse，防止并发 host/guest 文件操作导致数据损坏。

## 架构设计

### Forward Mode + Ready Port

所有平台统一使用 forward mode + ready port：

- **Forward mode (port 10000)**: Guest 监听 vsock port 10000，Host 连接
- **Ready port (port 10001)**: Guest bind/listen 后连接到 Host，通知已准备好

**为什么不用 Reverse Mode**：

Reverse mode（Guest 连接到 Host）不支持跨进程 reuse：
- Host 的 listener 期望 Guest 连接
- 另一个 host 进程连接会导致协议冲突
- 无法实现"ONE VM per env_root"

**Forward Mode 流程**：

```
1. Host 创建 ready listener (port 10001)
2. Guest 启动，bind/listen port 10000
3. Guest 连接到 Host 的 ready port 通知就绪
4. Host 收到通知，连接到 Guest port 10000
5. 任何 host 进程都可以连接（跨进程 reuse）
```

### 架构图

```
┌────────────────────────────────────────────────────────────┐
│                              Host                          │
│  ┌─────────────────┐      ┌─────────────────────────────┐  │
│  │   epkg (main)   │      │        libkrun/QEMU         │  │
│  │                 │      │                             │  │
│  │  setup_ready    │◄────►│  1. Create listen socket    │  │
│  │  _listener()    │      │     (Unix socket /          │  │
│  │                 │      │      Named Pipe)            │  │
│  │  wait_guest     │◄────►│  2. Wait for Guest connect  │  │
│  │  _ready()       │      │     on ready port 10001     │  │
│  └─────────────────┘      └─────────────────────────────┘  │
│           │                                                │
│           │ vsock 桥接 (Unix socket / Named Pipe)          │
│           ▼                                                │
│  ┌──────────────────────────────────────────────────────┐  │
│  │              Guest (Linux VM)                        │  │
│  │  ┌─────────────┐    ┌─────────────────────────────┐  │  │
│  │  │    init     │───►│  3. Start vm_daemon         │  │  │
│  │  └─────────────┘    └─────────────────────────────┘  │  │
│  │         │                                            │  │
│  │         ▼                                            │  │
│  │  ┌─────────────┐    ┌─────────────────────────────┐  │  │
│  │  │  vm-daemon  │───►│  4. bind/listen port 10000  │  │  │
│  │  │             │    │     (forward mode server)   │  │  │
│  │  └─────────────┘    └─────────────────────────────┘  │  │
│  │         │                                            │  │
│  │         ▼                                            │  │
│  │  ┌─────────────┐    ┌─────────────────────────────┐  │  │
│  │  │  ready notif│───►│  5. Connect to ready socket │  │  │
│  │  │ (port 10001)│    │     (Unix socket / pipe)    │  │  │
│  │  └─────────────┘    └─────────────────────────────┘  │  │
│  └──────────────────────────────────────────────────────┘  │
└────────────────────────────────────────────────────────────┘
```

### 平台实现差异

| 方面 | libkrun (macOS/Windows) | QEMU (Linux) |
|------|-------------------------|--------------|
| vsock 实现 | Unix socket / Named Pipe 桥接 | 原生 AF_VSOCK |
| socket 文件 | 有 | 无 |
| ready socket | Unix socket / Named Pipe 文件路径 | AF_VSOCK port |
| Keeper 启动 | fork() + setsid() / CreateProcess(DETACHED_PROCESS) | N/A (QEMU 是独立进程) |

**libkrun 模式**：

```
krun_add_vsock_port2(port, unix_socket_path, listen)
→ 创建 Unix socket 文件作为 vsock 桥接
```

**QEMU 模式**：

```
Host: socket(AF_VSOCK) + bind(port 10001)
Guest: connect to vsock port 10001
```

## epkg vm 命令

```bash
epkg vm start <env> [key=value ...]   # 启动 VM
epkg vm stop <env>                    # 停止 VM
epkg vm list                          # 列出 VM
epkg vm status <env>                  # YAML dump
```

### 启动选项

**--vmm** 指定 VMM 后端：
- `libkrun` (默认，macOS/Windows)
- `qemu` (Linux)

### key=value 参数

| 参数 | 说明 | 默认值 |
|------|------|--------|
| `timeout` | 空闲超时（秒）- 从命令执行完成开始计时，0 = 永不超时 | 0 |
| `extend` | 每次 run 完成后延长的时间（秒） | 10 |
| `cpus` | VM CPU 数 | 2 |
| `memory` | VM 内存（MiB） | 1024 |

### 使用示例

```bash
# 启动 VM（后台运行，永不超时）
epkg vm start fuzz-alpine cpus=4 memory=2048

# 启动 VM（带超时）
epkg vm start fuzz-alpine timeout=120 cpus=4 memory=2048

# 在 Linux 上使用 QEMU 后端
epkg vm start fuzz-alpine --vmm qemu cpus=4

# 查看运行中的 VM
epkg vm list

# 查看 VM 详情（YAML）
epkg vm status fuzz-alpine

# 停止 VM
epkg vm stop fuzz-alpine
```

## Session 文件格式

**位置**: `{epkg_run}/vm-sessions/{env_name}.json`

```json
{
  "version": 2,
  "env_name": "fuzz-alpine",
  "env_root": "/Users/aa/.epkg/envs/fuzz-alpine",
  "daemon_pid": 12345,
  "socket_path": "/Users/aa/.epkg/run/vsock-fuzz-alpine.sock",
  "backend": "libkrun",
  "config": {
    "timeout": 0,
    "extend": 10,
    "cpus": 4,
    "memory_mib": 2048,
    "backend": "libkrun"
  },
  "created_at": 1712345678,
  "last_activity": 1712345678
}
```

## 流程图

### vm start 流程

```
┌─────────────────────────────────────────────────────────────────────────┐
│  epkg vm start fuzz-alpine timeout=120                                  │
│                                                                         │
│  主进程:                                                                │
│  1. 检查 session → 不存在                                               │
│  2. Unix: fork() / Windows: spawn DETACHED_PROCESS                      │
│  3. 子进程执行 keeper 逻辑                                              │
│  4. 主进程等待 session ready (最多 30s)                                 │
│  5. 主进程退出                                                          │
│                                                                         │
│  子进程 (keeper):                                                       │
│  1. 创建 VM (forward mode)                                              │
│  2. 等待 Guest ready                                                    │
│  3. 注册 session file                                                   │
│  4. krun_start_enter() 阻塞                                             │
│  5. 空闲 120s 后 VM 关闭（或 timeout=0 永不关闭）                       │
│  6. 清理 session file，退出                                             │
└─────────────────────────────────────────────────────────────────────────┘
```

### epkg run 流程（统一流程）

```
┌─────────────────────────────────────────────────────────────────────────┐
│  epkg run --isolate=vm -- ls                                            │
│                                                                         │
│  epkg run 关注: client<>daemon 请求/响应交互                            │
│  VM lifecycle 关注: VM 存活时间 (>= epkg run lifecycle)                 │
│                                                                         │
│  统一流程:                                                              │
│  1. 检查 session file 是否存在                                          │
│     ├─ 存在 → 复用现有 VM                                               │
│     │   - 连接 socket，发送命令                                         │
│     │   - 使用 session config 的 timeout                                │
│     │                                                                   │
│     └─ 不存在 → 启动新 VM                                               │
│     │   - 注册 session file                                             │
│     │   - 连接 socket，发送命令                                         │
│     │   - vm_keep_timeout 参数决定 VM 存活时间                          │
│     │                                                                   │
│  2. 发送命令请求（包含 vm_keep_timeout_secs）                           │
│  3. Guest daemon 执行命令，返回结果                                     │
│  4. epkg run 收到结果后退出                                             │
│                                                                         │
│  VM lifecycle (独立于 epkg run):                                        │
│  Guest daemon 根据 vm_keep_timeout_secs 决定:                           │
│     ├─ > 0: 等待 idle timeout (可能服务后续 epkg run)                   │
│     ├─ = 0: 永不关闭                                                    │
│     └─ 未设置: 立即 shutdown                                            │
└─────────────────────────────────────────────────────────────────────────┘
```

## 安全保证

- **ONE VM per env_root**: session file + socket path collision prevention
- **Stale cleanup**: PID liveness check prevents zombie sessions
- **Socket lock**: connection success indicates VM is truly alive
- **Cross-process safe**: any host process can connect to Guest

## Timeout 语义

### vm_keep_timeout 参数

**控制 VM 存活时间的唯一参数**：

| 值 | 语义 |
|----|----|
| `Some(N)` (N > 0) | 空闲 N 秒后自动关闭 |
| `Some(0)` | 永不超时，VM 一直运行直到手动停止 |
| `None` | 命令完成后立即关闭 |

**空闲计时**：从命令执行**完成**开始计时，不是从开始运行。

**Session 复用**：复用现有 session 时使用 session config 中的 timeout 值。

### extend 机制

每次 `epkg run` 完成后可延长存活时间 `extend` 秒（session config 中配置）。

## Session 发现与复用机制

### 统一原则

**Session file ALWAYS**:
- 执行命令前：必须检查 session file（其他进程可能已启动 VM）
- 启动新 VM 时：必须注册 session file（其他进程可能需要复用）

这处理了所有并发场景：多个终端同时运行 `epkg run` 或 `epkg vm start`。

### 发现流程

```rust
// src/vm/client.rs: try_execute_via_existing_vm_session()
1. 检查 session 文件是否存在
2. 解析 JSON 内容
3. 验证 env_name 匹配
4. 检查 daemon_pid 是否存活（kill(pid, 0)）
5. 尝试连接 socket（验证 VM 真正可用）
6. 如果可用 → 发送命令，使用 session 的 timeout
7. 如果不可用 → 清理 stale session，返回 None
```

### Session 文件位置

```
{epkg_run}/vm-sessions/{env_name}.json
{epkg_run}/vsock-{env_name}.sock
```

env_name 就是环境名称，不关心/假设它来自 `-e` 还是 `-r`。

### Stale Session 清理

Session 文件可能因进程崩溃而残留。清理条件：
1. daemon_pid 不存活
2. socket 不可连接

```rust
if !is_process_alive(info.daemon_pid) {
    cleanup_vm_session_files(&session_file, &socket_path);
}
```

## 与 pir.py 集成

```python
def run_fuzz_iteration(os_name, env_name, ...):
    # 启动 VM（永不超时）
    run_epkg(['vm', 'start', env_name], 'self')

    # 测试 executables（自动复用 VM）
    for exe in executables:
        # 无需任何 flag，自动检测 VM session
        run_epkg(['run', '--isolate=vm', '--', exe, '--help'], env_name)

    # 停止 VM
    run_epkg(['vm', 'stop', env_name], 'self')
```

## 相关文件

| 文件 | 功能 |
|------|------|
| `src/libkrun/mod.rs` | libkrun 模块入口 |
| `src/libkrun/core.rs` | libkrun VM 核心逻辑 |
| `src/libkrun/bridge.rs` | vsock 桥接（ready listener、connect） |
| `src/libkrun/stream.rs` | 命令流处理（send_command_via_vsock、streaming I/O） |
| `src/vm/mod.rs` | vm 模块入口 |
| `src/vm/session.rs` | Session 文件管理 |
| `src/vm/start.rs` | vm start 实现 |
| `src/vm/stop.rs` | vm stop 实现 |
| `src/vm/keeper.rs` | VM keeper 进程逻辑 |
| `src/vm/list.rs` | vm list 实现 |
| `src/vm/status.rs` | vm status 实现 |
| `src/vm/guest_daemon.rs` | Guest 端 vm_daemon |
| `src/vm/client.rs` | VM session client（try_execute_via_existing_vm_session） |
| `src/qemu.rs` | QEMU backend |
| `src/run.rs` | RunOptions 和 fork_and_execute |
| `src/vmm.rs` | VMM backend 选择 |

## VM-Host 通信协议

### StreamMessage 消息格式

Host 和 Guest 之间通过 vsock 传递 JSON 格式的 StreamMessage。每条消息以换行符 `\n` 结束：

```rust
/**
 * StreamMessage - Host <-> Guest 通信协议
 *
 * 格式: JSON + '\n' 换行分隔
 * 编码: stdout/stderr/stdin 数据使用 base64 编码
 *
 * 流程:
 *   1. Host 发送 CommandRequest (命令 + 环境 + cwd + stdin + vm_keep_timeout_secs)
 *   2. Guest 执行命令，实时发送 Stdout/Stderr chunks
 *   3. Guest 发送 Exit 或 Error 表示执行结束
 *   4. Host 收到 Exit/Error 后关闭连接
 *
 * 流量控制:
 *   - Guest 使用 blocking write + yield_now() 确保数据不丢失
 *   - 每次写入后 yield，让 kernel 处理 vsock buffer
 *   - 避免 256KB guest vsock buffer 溢出
 */
enum StreamMessage {
    // ═══════════════════════════════════════════════════════════
    // Guest → Host: 命令输出 (流式传输)
    // ═══════════════════════════════════════════════════════════
    /**
     * Stdout - 标准输出数据块
     * - data: base64 编码的原始数据
     * - seq: 序号（用于调试/排序）
     *
     * 流量控制关键: write 后必须 yield_now()
     * 否则会丢失数据（vsock buffer ~256KB）
     */
    Stdout { data: String, seq: u64 },

    /**
     * Stderr - 标准错误数据块
     * 格式同 Stdout，用于分离 stdout/stderr
     */
    Stderr { data: String, seq: u64 },

    // ═══════════════════════════════════════════════════════════
    // Guest → Host: 执行结果 (终止消息)
    // ═══════════════════════════════════════════════════════════
    /**
     * Exit - 正常退出
     * - code: 命令退出码 (0=成功, >0=错误, 128+N=信号)
     *
     * 必须在命令执行完成后发送，即使 stdout/stderr 为空
     */
    Exit { code: i32 },

    /**
     * Error - 异常终止
     * - message: 错误描述
     *
     * 用于替代 Exit，表示 guest daemon 内部错误:
     * - spawn 失败
     * - poll/read 错误
     * - 资源不足
     */
    Error { message: String },

    // ═══════════════════════════════════════════════════════════
    // Host → Guest: 输入转发 (stream 模式)
    // ═══════════════════════════════════════════════════════════
    /**
     * Stdin - 标准输入数据块
     * - data: base64 编码的原始数据
     * - seq: 序号
     *
     * 仅在 stream 模式使用（batch 模式 stdin 在 CommandRequest 中）
     * Host stdin thread 持续读取并转发
     */
    Stdin { data: String, seq: u64 },

    /**
     * StdinEof - stdin 结束
     * 当 host stdin 关闭时发送，通知 guest 关闭 stdin pipe
     *
     * 关键: 必须发送此消息，否则 guest stdin pipe 保持打开
     * 导致 cat 等命令永远等待输入
     */
    StdinEof { seq: u64 },

    // ═══════════════════════════════════════════════════════════
    // Host → Guest: 信号/终端控制 (PTY/交互模式)
    // ═══════════════════════════════════════════════════════════
    /** Signal - 信号转发 (INT/TERM/HUP/QUIT/KILL/WINCH) */
    Signal { signal: String },

    /** Resize - 终端大小变化 */
    Resize { rows: u16, cols: u16 },
}
```

### 命令请求格式

```rust
/**
 * CommandRequest - Host 发送给 Guest 的命令请求
 */
{
    "command": ["ls", "-la"],          // 命令和参数
    "cwd": "/home/user",               // 工作目录
    "env": { "PATH": "...", ... },     // 环境变量
    "pty": false,                      // 是否使用 PTY
    "batch": false,                    // 是否 batch 模式
    "stdin": "",                       // stdin 数据 (base64)
    "vm_keep_timeout_secs": 60,        // 空闲超时（秒）
    // vm_keep_timeout_secs:
    //   - Some(N) > 0: 空闲 N 秒后关闭
    //   - Some(0): 永不关闭
    //   - None: 命令完成后立即关闭
    "host_time": 1712345678000000000   // host 时间（纳秒，用于 guest 时钟同步）
}
```

### 消息流程

```
Host                              Guest
  │                                 │
  │──── CommandRequest (JSON) ─────►│  命令 + 环境 + cwd + stdin + vm_keep_timeout_secs
  │                                 │
  │◄──── Stdout/Stderr ────────────│  实时输出流 (blocking write + yield)
  │◄──── Stdout/Stderr ────────────│  (base64 编码, 每块 <4KB)
  │        ...                      │
  │                                 │
  │──── Stdin (stream模式) ────────►│  stdin 数据块 (host stdin thread)
  │──── StdinEof ──────────────────►│  stdin EOF (关闭 guest stdin pipe)
  │                                 │
  │◄──── Exit/Error ───────────────│  执行结果
  │                                 │
```

### Guest Daemon 行为

根据 `vm_keep_timeout_secs` 决定命令完成后的行为：

```rust
// src/vm/guest_daemon.rs: handle_connection()
match request.vm_keep_timeout_secs {
    Some(0) => {
        // 永不超时 - 无限等待下一个连接
        ConnectionDisposition::ReuseWait { idle_timeout_ms: 0 }
    }
    Some(secs) if secs > 0 => {
        // 空闲 secs 秒后关闭
        ConnectionDisposition::ReuseWait { idle_timeout_ms: secs * 1000 }
    }
    None => {
        // 立即关闭
        ConnectionDisposition::Shutdown
    }
}
```

### Batch vs Stream 模式

**Batch 模式**：
```
特点: Guest 先收集所有输出，再一次性发送
流程:
  1. Guest 执行命令，收集 stdout/stderr 到 buffer
  2. 命令完成后，按 chunk 发送 Stdout/Stderr
  3. 发送 Exit
优点: 简单，适合小输出
缺点: 大输出占用大量内存，不适合交互

stdin: 在 CommandRequest 中传递 (一次性)
```

**Stream 模式**：
```
特点: 实时流式传输，支持 stdin 转发
流程:
  1. Guest 执行命令
  2. 实时读取 stdout/stderr，发送 Stdout/Stderr chunks
  3. Host stdin thread 转发 stdin 数据
  4. Host stdin EOF → 发送 StdinEof → guest 关闭 stdin pipe
  5. 发送 Exit
优点: 实时交互，内存占用小，支持 stdin
缺点: 流量控制复杂

stdin: 独立 thread 持续转发 (实时)
关键: stream 模式必须创建 stdin pipe (即使 request.stdin 为空)
```

### 流量控制机制

**问题背景**：
- Guest kernel vsock buffer ~256KB
- Guest 写入速度 > Host 接收速度
- buffer 溢出 → 数据丢失 (约 ~540KB 处截断)

**解决方案**：blocking write + yield_now()

```rust
// src/vm/guest_daemon.rs: write_stream_message()
fn write_stream_message(stream: &mut TcpStream, msg: &StreamMessage) {
    // 1. 切换到 blocking 模式 (kernel 处理 backpressure)
    stream.set_nonblocking(false)?;

    // 2. 写入数据 (会阻塞直到 buffer 有空间)
    stream.write_all(json.as_bytes());
    stream.write_all(b"\n");
    stream.flush();

    // 3. 关键: yield 让 kernel 处理数据
    //    - vsock kernel thread 发送数据到 host
    //    - host 读取数据，发送 credit update
    //    - guest kernel 更新可用 buffer 空间
    std::thread::yield_now();

    // 4. 恢复 non-blocking 模式 (poll loop 需要)
    stream.set_nonblocking(true)?;
}
```

**工作原理**：
```
┌─────────────────────────────────────────────────────────────────┐
│                      流量控制流程                               │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  Guest                          Host                            │
│    │                              │                             │
│    │ write()                      │                             │
│    │──► vsock buffer ─────────────► read()                      │
│    │    (~256KB)                  │                             │
│    │                              │                             │
│    │ yield_now()                  │                             │
│    │──► scheduler switch          │                             │
│    │                              │                             │
│    │        ┌─────────────────────┤                             │
│    │        │ kernel vsock thread │                             │
│    │        │ - 发送数据          │                             │
│    │        │ - 等待 credit       │                             │
│    │        └─────────────────────┤                             │
│    │                              │                             │
│    │                              │◄── credit update            │
│    │◄── buffer space freed        │                             │
│    │                              │                             │
│    │ next write() succeeds        │                             │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

**为什么 yield_now() 有效**：
1. 允许调度器切换到 kernel vsock thread
2. Kernel 发送 buffer 中的数据到 host
3. Host 接收数据，libkrun 发送 credit update
4. Guest kernel 收到 credit，更新 fwd_cnt
5. vsock buffer 空间释放，下次 write 成功

### Stdin 转发机制

**问题**：stream 模式下 stdin 转发失败

**原因**：
1. `spawn_child_piped()` 只在 `request.stdin` 非空时创建 stdin pipe
2. stream 模式的 stdin 来自 stream 消息，不在 request 中
3. child 获得 `/dev/null` 作为 stdin，无法接收转发数据

**修复**：
```rust
// src/vm/guest_daemon.rs: spawn_child_piped()
let stdin_pipe = if request.batch {
    // Batch: stdin 在 request 中
    if request.stdin.is_empty() { None } else { Some(pipe) }
} else {
    // Stream: stdin 来自 stream 消息，必须创建 pipe
    Some(pipe)
};
```

**Host stdin thread**：
```rust
// src/libkrun/stream.rs: handle_streaming_simple()
// stdin thread 仅在 stream 模式运行 (!is_batch)
loop {
    if exit_code.is_some() { break; }

    // poll stdin
    poll(stdin_fd, POLLIN, timeout=50ms);

    if ready {
        read(stdin) → encode base64 → send Stdin message
    }
}

// stdin EOF → 发送 StdinEof
if read() == 0 {
    send StdinEof { seq };  // 关键: 必须发送
    break;
}
```

### 错误处理原则

**关键规则**：Guest 必须在任何退出场景发送 `Exit` 或 `Error` 消息。

Guest daemon 错误路径必须发送消息的场景：
1. `execute_without_pty` / `execute_with_pty` / `execute_batch` 内部错误
2. `handle_connection` 中的 poll 错误、read 错误、JSON 解析错误
3. 超时、进程 spawn 失败等

Host 端处理：
```rust
// src/libkrun/stream.rs
StreamMessage::Exit { code } => { got_exit = true; exit_code = code; }
StreamMessage::Error { message } => { got_exit = true; exit_code = -1; }

// 如果收到 EOF 但没有 Exit/Error，说明 VM 异常关闭
if !got_exit {
    return Err("VM connection closed prematurely");
}
```

### 相关代码位置

| 文件 | 关键函数 | 功能 |
|------|----------|------|
| `src/vm/guest_daemon.rs` | `write_stream_message()` | blocking write + yield |
| `src/vm/guest_daemon.rs` | `spawn_child_piped()` | stdin pipe 创建逻辑 |
| `src/vm/guest_daemon.rs` | `nonpty_poll_loop()` | poll loop 处理 |
| `src/vm/guest_daemon.rs` | `handle_connection()` | vm_keep_timeout_secs 处理 |
| `src/libkrun/stream.rs` | `handle_streaming_simple()` | host reader/stdin thread |
| `src/libkrun/stream.rs` | `send_command_via_vsock()` | 命令发送入口 |
| `git/libkrun/unix.rs` | `sendmsg()` | TX retry + credit update |

## Install/Upgrade 期间的 VM 复用

### 事务流程中的 VM 生命周期

```
epkg install package1 package2 ...

  1. 开始事务，检测/创建 VM session
     │
     ▼
  2. 安装包文件（复用 VM）
     │
     ▼
  3. 运行 PostTransaction hooks（复用 VM）
     │  - glib-compile-schemas
     │  - gtk-update-icon-cache
     │  - 其他 trigger hooks
     ▼
  4. 事务结束，关闭 VM session
     shutdown_vm_reuse_session_if_active()
```

### Hooks 的 VM 复用

Hooks 执行时自动继承活跃的 VM session：

```rust
// src/hooks.rs
let run_options = RunOptions {
    command,
    args,
    no_exit: !hook.action.abort_on_fail,  // 重要：不退出进程
    ..Default::default()
};

// prepare_run_options_for_command() 自动检测 VM session
// 并复用现有 session
fork_and_execute(env_root, &run_options)?;
```

## 常见问题与解决

### "connection closed without exit message"

**现象**：Host 收到 EOF 但没有收到 `Exit` 或 `Error` 消息。

**原因**：
1. Guest 进程崩溃
2. VM 资源不足（OOM）
3. vsock 连接中断
4. Guest daemon 错误路径未发送消息（已修复）

**修复**：
- 确保所有 guest daemon 错误路径发送 `Error` 消息
- Host 端将 `Error` 视为有效的终止响应

### VM 意外关闭

**可能原因**：
1. VM 内存不足 → 增加 `--vm-memory`
2. 执行命令导致 guest panic → 检查命令本身
3. virtiofs 超时 → 检查主机 IO 负载

**调试方法**：
```bash
# 启用详细日志
RUST_LOG=trace epkg run --isolate=vm -- ls

# 查看 VM console 输出
EPKG_DEBUG_LIBKRUN=1 epkg run --isolate=vm -- ls

# 检查 guest debug log
cat /opt/epkg/guest-debug.log  # 在 VM 内
```

## 验证步骤

```bash
# 启动 VM
epkg vm start fuzz-alpine timeout=120

# 查看运行中的 VM
epkg vm list

# 查看 VM 详情（YAML）
epkg vm status fuzz-alpine

# 自动复用 VM（无需任何 flag）
epkg run --isolate=vm -- ls

# 停止 VM
epkg vm stop fuzz-alpine
```
