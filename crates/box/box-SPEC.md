# box-SPEC

> `kusanagi-box` —— 网络里不被信任的那一半，作为一个别人能运行的程序。
>
> 权威顺序：用户裁决 → `ARCHITECTURE.md` → 本文 → 代码与测试。本文先于代码改动。

## 1 需求拆解

| 单元 | 交付物 | 独立验收 |
|---|---|---|
| U1 服务端 | `Server::serve` / `answer` / `route` | 两个进程经真实 TCP 完成写读 |
| U2 一次性写入 | `PUT` 必须带 `If-None-Match: *`，第二次得 412 | 同一地址第二次写被拒 |
| U3 条件读 | `If-None-Match` 命中返回 304 且不带正文 | 已是最新的读者不再收到字节 |
| U4 过期 | `X-Kusanagi-Ttl` 落成对象前缀的到期戳 | 写入时已过期的对象永远读不到 |
| U5 有界 | 请求头 8 KiB、正文 1 MiB、空闲 30 秒 | 陌生人无法让本进程按他的说法分配内存 |

## 2 验收标准

`crates/box/src/serve.rs` 的四个测试即验收标准：它们用**出货的客户端**（`kusanagi_waypoint::HttpWaypoint`）
经真实 socket 驱动**出货的服务端**，并对它跑一遍 `kusanagi_waypoint::conformance::run`。
`crates/kusanagi/tests/across_tcp.rs` 再从更高一层重复一次同样的事。

**这条测试就是本 crate 与 `waypoint` 分家的代价的抵押品。** 协议两半分处两个 crate 而不漂移，
靠的是它，不是靠两个文件挨着放。

## 3 假设与歧义

| 歧义 | 假设 | 何时失效 |
|---|---|---|
| 盒子是否需要认证 | 不需要 | 见 `docs/box-protocol.md`：宿主知道调用者是谁，就知道了设计承诺它不知道的东西 |
| 并发模型 | 每连接一线程，响应一律 `Connection: close` | 当一台盒子要服务上千个 agent 时；那时先测量再改 |
| TLS | 不做，前面放终结器 | 内容本就是密封的；TLS 只多藏一层地址 |
| 存储 | 复用 `DirWaypoint`，不另写一套磁盘格式 | 永不失效；两套磁盘格式就是两个权威 |

## 4 现状分析

本 crate 由 `kusanagi-waypoint` 拆出，理由记在 `crates/waypoint/waypoint-SPEC.md` §7 的「被推翻的决定」一节：
那个 crate 的 `src/` 撞上了 `ARCHITECTURE.md` §5 的 2,500 行上限，而可选的两条缝里，
**把同一个 seam 的多个实现分开更坏**，把「怎么去到一台宿主」与「怎么当一台宿主」分开更对。

## 5 权威信源

| 事实 | 来源 |
|---|---|
| 盒子协议的线路格式 | `docs/box-protocol.md` |
| 宿主不被信任，只持有密封字节 | `ARCHITECTURE.md` §1、§3 |
| 条件写的分歧朝失败开放，所以要实测 | `ARCHITECTURE.md` §8 |

## 6 命名统一

`Box` 已进入 `ARCHITECTURE.md` §4 词表：**一台别人运行的宿主**。`Server` 是它的服务端类型，
`Request`/`Response` 是一次交换的两端，都不出 crate。

## 7 模块边界

```
lib.rs       模块索引
serve.rs     Server —— 监听、路由、读写、过期信封
exchange.rs  Request / Response —— 一次交换，读进来就有界
```

依赖 `kernel`（Clock/DropAddr/Instant/PutOutcome/Waypoint）、`waypoint`（`DirWaypoint` 存盘）、
`blake3`（ETag）。开发期额外依赖 `seal`，因为测试要派生真实地址。

**不依赖 `kusanagi`。** 盒子不知道段、链、grant 或通道；它只知道地址与字节。

## 8 接口先行

```rust
pub struct Server<C> { /* 私有 */ }
impl<C: Clock> Server<C> {
    pub fn new(root: impl Into<PathBuf>, clock: C) -> Self;
    pub fn serve(&self, listener: &TcpListener) -> Result<(), io::Error> where C: Sync;
    pub fn answer(&self, stream: TcpStream) -> Result<(), io::Error>;
}
```

`serve` 只在**监听器**坏掉时返回 `Err`；应答某一个调用者时的失败写 stderr 而不打断其他人。

## 9 工作流程

```
TcpListener → serve（每连接一线程）→ answer → Request::read（有界）
             → route → read / write → Response::write → 关闭连接
```

## 10 实现逻辑

**步骤 1：盒子没有无条件写。** `PUT` 缺 `If-None-Match: *` 返回 428。
协议里不存在能覆写的请求，于是这台宿主**没有办法**意外失去一次性写入语义。

**步骤 2：过期是对象自己的前缀，不是一张表。** 写入时把到期戳放在字节前面，读的时候比一次时钟。
没有清扫线程，也没有第二份状态可以和对象本身不一致。

**步骤 3：ETag 是内容哈希。** 它按构造稳定，而不是靠宿主记得让它稳定——那正是 `doctor` 实测的四件事之一，
也是这台宿主不可能失败的那一件。

**步骤 4：每个上限都是拒绝按陌生人的说法分配内存。** 头 8 KiB、正文 1 MiB、空闲 30 秒断开。

## 11 边界枚举

| 情形 | 期望 |
|---|---|
| `GET /health` | 200 与一句自述横幅 |
| `GET` 一个不存在的地址 | 404 |
| `GET` 已过期的对象 | 404，字节仍在盘上但永不出门 |
| `PUT` 缺 `If-None-Match: *` | 428 |
| `PUT` 到已被占用的地址 | 412 |
| `X-Kusanagi-Ttl` 不是整数秒 | 400 |
| 请求头超过 8 KiB / 正文超过 1 MiB | 400 |
| 非 GET/PUT 方法 | 405 |
| 畸形请求 | 是一个响应，不是一个错误 |

## 12 错误处理

`answer` 的 `Err` 只表示连接坏了。协议层面的每一种拒绝都是一个状态码与一句话，
因为**盒子的失败必须能被一个不读 Rust 的客户端读懂**。

## 13 依赖选型

不引入任何 HTTP 框架。盒子说的是协议的一个刻意窄的子集（两个方法、四个头），
手写的解析器比一个框架少几百个依赖，而这几百个依赖会进到用户机器上运行的那个二进制里。

## 14 硬编码声明

| 硬编码 | 意图 | 后续影响 |
|---|---|---|
| `MAX_HEAD = 8 KiB` | 头部有界 | —— |
| `MAX_BODY = 1 MiB` | 正文有界，远高于一个段 | 段上限变了要一起想 |
| `IDLE = 30s` | 空闲连接断开 | —— |
| `BANNER = "kusanagi-box/1 …"` | 一句自述，**不是证据**；`doctor` 无视它并实测 | —— |
| 每连接一线程 | 最简单且不会死锁 | 有真实流量后再测量 |

## 15 影响面

上游是 `kusanagi` 的 `host` 动词与 `across_tcp` 测试。
盒子协议的任何改动同时约束 `kusanagi-waypoint::http`、本 crate 与 `docs/box-protocol.md`，
三者必须同一次改动。

## 16 测试与约束

四个测试在 `serve.rs` 内，全部经真实 socket。没有 mock：一个不经 socket 的盒子测试
测的是别的东西。

## 17 文档同步

1. 本文。
2. `docs/box-protocol.md`——协议的任何改动。
3. `ARCHITECTURE.md` §4 词表、§5 crate 图与行数表。
4. `crates/waypoint/waypoint-SPEC.md` §7——两半的边界。
