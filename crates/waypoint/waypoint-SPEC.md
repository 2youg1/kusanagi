# waypoint-SPEC

> `kusanagi-waypoint` —— `kernel::Waypoint` 这条 seam 的适配器，以及**替换实现必须通过的那套 conformance suite**。

## 1 需求拆解

| 单元 | 交付物 | 独立验收 |
|---|---|---|
| U1 目录适配器 | `DirWaypoint` | 阶段 0 的收口条件由它承担 |
| U2 内存适配器 | `MemoryWaypoint` | 让这条 seam 成为真 seam（一个适配器是假想的，两个才是真的） |
| U3 一致性套件 | `conformance::run(&impl Waypoint)` | 两个适配器同时通过；将来的 S3 适配器调用同一函数 |

## 2 验收标准

1. 两个适配器都通过 `conformance::run`，且**同一个函数**，不是两份相似的测试。
2. 写入-已存在的第二次写入返回 `AlreadyPresent`，且 `get` 仍返回**第一次**的字节——这是一次性写入语义，是全设计唯一当作前提的原语。
3. `DirWaypoint` 的写入是原子的：并发或崩溃不产生半个文件。
4. 空地址 `get` 返回 `Ok(None)`，不是错误。

## 3 假设与歧义

| 歧义 | 假设 | 何时失效 |
|---|---|---|
| 一次性写入靠谁保证 | 阶段 0 靠文件系统的 `create_new`；阶段 3 靠 S3 的 `If-None-Match: *` | MinIO 的语义与 S3 分歧（见 `ARCHITECTURE.md` §3.3），故阶段 3 必须由 `doctor` 实测而非相信文档 |
| 目录如何分片 | 地址前 2 个十六进制字符作一级目录 | 单目录文件数过多时文件系统性能崩塌；256 路分片足以覆盖阶段 0 到阶段 3 |
| 是否需要删除 | 阶段 0 不需要 | TTL 与回收在阶段 3 由对象存储的生命周期规则承担，不由本 crate |

## 4 现状分析

新 crate。`kernel::Waypoint` 已定义两个方法与 `PutOutcome`，本 crate 只实现，不扩展接口。

## 5 权威信源

| 事实 | 来源 |
|---|---|
| `create_new(true)` 在 open 时原子地要求文件不存在 | `std::fs::OpenOptions` 文档 |
| S3 `If-None-Match: *` 表示仅当 key 不存在时写入 | AWS S3 用户指南 *Conditional writes*（2024-08） |
| MinIO 只接受精确 ETag，不接受通配符 | minio/minio#20346 |
| 「一个适配器是假想的 seam，两个才是真的」 | sprawling `ARCHITECTURE.md` §4 |

## 6 命名统一

`DirWaypoint` / `MemoryWaypoint` / `conformance`。不使用 "backend"、"store"、"provider" —— 这条 seam 的名字是 Waypoint，适配器只在前面加限定词。

## 7 模块边界

依赖 `kernel`。`conformance` 依赖前两者但反向依赖为零。

```
lib.rs         模块索引，零逻辑
dir.rs         DirWaypoint
memory.rs      MemoryWaypoint
conformance.rs 契约本身
```

## 8 接口先行

```rust
pub struct DirWaypoint { /* root: PathBuf */ }
impl DirWaypoint { pub fn new(root: impl Into<PathBuf>) -> Self; }

pub struct MemoryWaypoint { /* Mutex<BTreeMap<DropAddr, Vec<u8>>> */ }
impl MemoryWaypoint { pub fn new() -> Self; }

pub mod conformance {
    pub fn run(waypoint: &impl Waypoint) -> Result<(), Failure>;
    pub struct Failure { /* clause: &'static str, detail: String */ }
}
```

`conformance::run` 返回 `Result` 而非 `assert!`：它将来要被 `kusanagi doctor` 在**真实宿主**上调用，那时失败必须是一条能打印给人和 Agent 的数据，而不是一次 panic。**这是它不写成 `#[test]` 的全部理由。**

## 9 工作流程

写：`put_if_absent(addr, bytes)` → 算出分片路径 → 建父目录 → `create_new` 打开 → 写入 → `Stored`；若 `AlreadyExists` → `AlreadyPresent`。

读：`get(addr)` → 算出路径 → 读；`NotFound` → `Ok(None)`。

## 10 实现逻辑

**步骤 1：路径由地址决定，与内容无关。** `root/<前2字符>/<后38字符>`。分片是为了避免单目录承载全部地址；取前 2 个字符而非哈希取模，是因为地址本身已经均匀分布——**再哈希一次是多余的工作，不是额外的安全**。

**步骤 2：一次性写入交给 `create_new`。** 不是「先 `exists()` 再写」——那是一个检查与使用之间的竞态窗口。`create_new` 把判断和创建合成一次系统调用，这也正是阶段 3 要在 S3 上用 `If-None-Match: *` 复现的语义。**两个适配器实现同一条语义，是这条 seam 能成立的原因。**

**步骤 3：`MemoryWaypoint` 用 `Mutex` 而非 `RefCell`。** `Waypoint::get` 取 `&self`，所以需要内部可变性；而适配器要能被多线程持有，`RefCell` 不满足。锁中毒时用 `PoisonError::into_inner` 取回守卫：我们在持锁期间只做 `BTreeMap` 的插入与查询，不可能留下不一致的映射，因此中毒不代表数据损坏。**这一句是理由，不是借口——若将来持锁期间出现多步更新，必须改成上抛错误。**

**步骤 4：conformance 是一串带名字的子句。** 每条子句失败时返回自己的名字与实测细节。子句名进入 `Failure`，将来直接成为 `doctor` 输出里的那一行。

**为何优于替代**：把一致性检查写成两份 `#[test]` 少写约 25 行，但外部适配器（企业自研的 S3 变体）就无法运行它，而 `ARCHITECTURE.md` §4.5 说需求 1 的完成判据正是「替换实现必须通过 conformance suite」。**测试写成函数，契约才是可移植的。**

## 11 边界枚举

| 情形 | 期望 |
|---|---|
| 读一个从未写过的地址 | `Ok(None)` |
| 同地址写两次，第二次内容不同 | `AlreadyPresent`，且读回第一次的内容 |
| 写空字节串 | `Stored`，读回空字节串（不是 `None`） |
| 根目录不存在 | 写入时按需创建 |
| 根路径指向一个文件而非目录 | `WaypointError::Io` |
| 两个线程同时写同一地址 | 恰有一个得 `Stored`，另一个得 `AlreadyPresent` |

## 12 错误处理

复用 `kernel::WaypointError`，不新增错误类型——**seam 的错误属于 seam，不属于适配器**，否则调用方要为每个适配器写一遍匹配。`std::io::Error` 经 `Io { action, source }` 携带，`action` 是静态字符串，用于让调用方说清失败在做什么。

## 13 依赖选型

只有 `kusanagi-kernel` 与 `thiserror`（后者供 `conformance::Failure`）。不引入 `tempfile`：测试用的临时目录由 `std::env::temp_dir` 加一个由测试名与计数器构成的子目录，测试结束时删除。多一个依赖不值得。

## 14 硬编码声明

| 硬编码 | 意图 | 影响 |
|---|---|---|
| 分片取前 2 个字符 | 256 路，单层 | 改动会使既有目录布局失效，需要迁移或重新投递 |
| conformance 的子句名（如 `"write-once"`） | 将成为 `doctor` 的输出字段 | 一旦发布即为稳定标识，改名等于改公开接口 |

## 15 影响面

`kusanagi` CLI 直接使用 `DirWaypoint`。阶段 3 的 S3 适配器将实现同一 seam 并调用同一 `conformance::run`。

## 16 测试与约束

`conformance::run` 对两个适配器各跑一次，构成两个 `#[test]`；外加 §11 表中 conformance 未覆盖的三行（根路径是文件、并发写、按需建目录）。

约束：`DirWaypoint` 不得在读路径上创建任何目录——读不应有副作用。

## 17 文档同步

1. 本文。
2. `ARCHITECTURE.md` §4.5 的 seam 表——填入两个适配器的实际名字。
3. `ARCHITECTURE.md` §4.2 的行数表。
4. 阶段 3 落地时，本文 §3 关于 MinIO 的假设行必须由 `doctor` 的实测结果替换。
