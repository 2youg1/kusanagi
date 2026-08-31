# kusanagi-SPEC

> `kusanagi` —— 二进制与唯一装配点。它是 harness 与网络之间那扇门在阶段 0 的形态。

## 1 需求拆解

| 单元 | 交付物 | 独立验收 |
|---|---|---|
| U1 装配 | `assembly::run` —— 唯一知道具体类型的地方 | 其余模块只见 trait 与值 |
| U2 走链 | `walk::chain` —— 从 waypoint 读出并验证一条链 | 常量额外状态；遇空地址即停 |
| U3 两个动词 | `send` / `read` | 阶段 0 收口条件由这两个动词达成 |
| U4 双渲染 | `Outcome::render` / `Complaint::render` | 同一结构渲染成散文与 JSON，二者不可能不一致 |

## 2 验收标准

1. 四次独立进程调用（三次 `send` 一次 `read`）产生一条连贯且可验证的链——**证明本机零常驻状态**。
2. 任意一个字节被篡改，`read` 以非零码退出并给出稳定错误码。
3. `--json` 的失败输出含 `error`、`code`、`recover` 三个字段。
4. `clippy --all-targets --all-features -- -D warnings` 零输出。

## 3 假设与歧义

| 歧义 | 假设 | 何时失效 |
|---|---|---|
| 阶段 0 如何知道自己的链高 | 从 waypoint 逐地址探测，不存本地文件 | 阶段 3 由 Bell 取代探测；本地仍不存状态 |
| 收件人是谁 | 阶段 0 没有收件人。地址由 `(author, index)` 公开派生，任何人可读 | 阶段 1 引入共享秘密派生，`read --from` 需要一份 Grant |
| payload 是什么 | 阶段 0 是命令行传入的一段文本 | 上层结构由阶段 3 之后定义；本 crate 永不解释它 |

## 4 现状分析

`kernel` 提供名词，`chain` 提供规则，`waypoint` 提供适配器。本 crate 只做装配与渲染，不新增任何领域概念。

## 5 权威信源

`ARCHITECTURE.md` §1「网络的接口是本地 socket，不是 harness」与 §6 的五条 AX 规则。阶段 0 兑现其中三条：稳定错误码、一份 schema 两个门面、错误自带恢复命令。**尚未兑现的两条**（无隐式会话状态、无游标陷阱）在阶段 6 的 `port` 落地。

## 6 命名统一

`Outcome` / `Complaint` / `Walked`。不使用 "result"、"response"、"report" 等同义词。`Complaint` 而非 `Error`，是为了与三个下层 crate 的 `*Error` 区分：**它不是一个新的失败，是一个已有失败加上恢复路径后的形态。**

## 7 模块边界

```
main.rs        参数解析与分派，零业务逻辑
assembly.rs    唯一持有具体类型的地方；将来唯一取时钟的地方
walk.rs        从 waypoint 读出一条链并验证
report.rs      成功输出的一个结构，两种渲染
complaint.rs   失败输出的一个结构，两种渲染
```

依赖 `kernel`、`chain`、`waypoint`、`clap`、`serde`、`serde_json`、`thiserror`。

## 8 接口先行

```rust
pub fn assembly::run(root: &Path, command: &Command) -> Result<Outcome, Complaint>;
pub fn walk::chain(waypoint: &impl Waypoint, author: &Handle) -> Result<Walked, Complaint>;
impl Outcome   { pub fn render(&self, json: bool) -> String; }
impl Complaint { pub fn render(&self, json: bool) -> String; }
```

`render(&self, json: bool)` 而非两个方法：**一个结构、一个入口、两种渲染**，于是散文与 JSON 不可能描述不同的事实。

## 9 工作流程

`main` 解析参数 → `assembly::run` 建 `DirWaypoint` 并分派 → `send` 先 `walk::chain` 取链头再构造段并 `put_if_absent`；`read` 只 `walk::chain` → 结果或抱怨经 `render` 输出 → 退出码 0 或 1。

## 10 实现逻辑

**步骤 1：链高从 waypoint 来，不从本地文件来。** 这是「无常驻状态法则」在阶段 0 最强的形态：本机一个字节都不存，因此 `kill -9` 无从破坏任何东西。代价是每次 `send` 要 O(n) 次读——**这个代价是阶段 3 的 Bell 必须打败的基准线，写在这里是为了让它有个对手。**

**步骤 2：`send` 遇到 `AlreadyPresent` 视为失败而非成功。** 走链拿到的链头与写入之间存在竞态：另一个写者可能已经占了下一个地址。此时正确的反应不是覆盖（做不到）也不是静默（会丢消息），而是告诉调用方**重新取链头再来**——`Complaint::DropTaken` 的 `recover` 字段就是那条命令。

**步骤 3：抱怨携带恢复命令，且只有这一层能填。** 下层三个 crate 给出失败的动作、对象与稳定码；「你现在该敲什么」只有见过命令行的这一层知道。这就是 `Complaint` 存在的全部理由，它不是一个包装器。

**为何优于替代**：把渲染分成 `to_text` 与 `to_json` 两个方法少写约 10 行，但两者会随时间漂移，而 `ARCHITECTURE.md` §6 要求 CLI 与将来 MCP 门面共用一份 schema。**一个结构两种渲染，是让那条要求在阶段 0 就无法被违反。**

## 11 边界枚举

| 情形 | 期望 |
|---|---|
| `read` 一个从未写过的名字 | `has no chain yet`，退出码 0 |
| `--root` 指向不可写目录 | `waypoint.io`，退出码 1 |
| 某段被篡改一个字节 | `chain.previous_mismatch`（或 `segment.*`），退出码 1 |
| 链中间缺一段 | `chain.index_gap` |
| payload 超过 64 KiB | `segment.payload_too_large` |
| 竞态下下一个地址被占 | `kusanagi.drop_taken`，恢复命令指向重读 |

## 12 错误处理

`Complaint` 用 `#[from]` 吸收三个下层错误，不用 `map_err(|_| …)`——**丢弃原因等于让调用方去猜**。`code()` 直接转发下层的稳定码，本层只为自己的 `DropTaken` 新增一个。

## 13 依赖选型

| 依赖 | 理由 | 代价 |
|---|---|---|
| `clap` 4（derive） | `--help` 与参数校验是 AX 的一部分；手写解析约省 60 行却要自己维护帮助文本 | 编译期依赖树较大，不进运行时 |
| `serde` + `serde_json` | JSON 门面。派生保证结构与输出同源 | 同上 |

## 14 硬编码声明

| 硬编码 | 意图 | 影响 |
|---|---|---|
| `--root` 默认 `.kusanagi` | 让第一次运行不需要任何参数 | 改动会让既有相对路径的调用指向别处 |
| 退出码 0 / 1 | 成功与失败 | 将来若要区分「网络失败」与「验证失败」，需要新增码，且那是一次接口变更 |

## 15 影响面

本 crate 是唯一的可执行产物。阶段 6 的 `port` 落地后，`assembly::run` 将同时被 CLI 与本机 socket 调用，届时它必须先脱离 `Command` 这个 clap 类型。**那次改动从这里开始，不是从 `port` 开始。**

## 16 测试与约束

阶段 0 由 `just demo` 端到端验收，尚**无**自动化集成测试——`assembly::run` 目前接收 clap 的 `Command`，从测试驱动它需要先把命令表示与解析分开。这是本 crate 已知的欠账，在 `port` 落地前必须还上，届时补：四次独立调用的连贯性、篡改检出、JSON 字段齐备三项。

约束：`main` 之外不得有任何 `println!`；渲染只产生字符串，由 `main` 决定往哪里写。

## 17 文档同步

1. 本文。
2. `ARCHITECTURE.md` §4.2 的行数表与 §8 的阶段表。
3. `AGENTS.md` 的 `just` 命令表，若新增动词。
