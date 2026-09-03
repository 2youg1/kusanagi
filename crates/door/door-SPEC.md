# door-SPEC

**门。** 一个 kusanagi 动词说出口的一切都从这里出去：成功是 `Outcome`，失败是 `Complaint`，
两者各有两种渲染——给程序的 JSON 与给人的散文。

本 crate 碰不到 waypoint、磁盘与时钟，这不是巧合而是它成为 crate 的理由：**输出契约是公开
承诺，而承诺与执行它的代码住在一起，就会在改执行的时候被顺手改掉。**

## 1 需求拆解

| 单元 | 内容 |
|---|---|
| U1 | `Outcome`——十个动词各自的成功形状，一个值,两种渲染 |
| U2 | `Complaint`——失败 + 稳定码 + **恢复命令** |
| U3 | 散文渲染——列宽、措辞、警告，与机器读的值分开 |
| U4 | 与执行层的边界：door 不知道 walk、place、site 的存在方式 |

## 2 验收标准

- `just check` 绿；`crates/door/src` ≤ 900 行；`crates/kusanagi/src` ≤ 1 700 行。
- `kusanagi::run` 的签名不变，`crates/kusanagi/tests/*.rs` 与 `adversary/` 零改动。
- `--json` 输出与拆分前逐字节相同。

## 3 假设与歧义

**歧义:`Outcome::read` 曾接收 `&Walked`。** `Walked` 是「走一条流」的结果，属执行层；
door 若认识它，就等于 door 能触发一次网络往返。**裁决:door 接收 `(index, payload)` 的迭代器,
`--after` 的过滤留在动词那侧**（`traffic::reported`）。`impl From<Walked> for Outcome` 装不下
`name`/`author`/`after` 三个参数，故不采用。

## 4 现状分析

拆分前 `report.rs` 368 + `prose.rs` 155 + `complaint.rs` 334 = 857 行住在 `crates/kusanagi/src`，
该 crate 已到 2 485 / 2 500，任何新动词的第一行代码都会打红门禁。拆分后 kusanagi 1 639、door 886。

## 5 权威信源

`ARCHITECTURE.md` §5（crate 图与行数预算）、§7 法则；`AGENTS.md`「机器持有的规则」；
`kusanagi-SPEC.md` §12 的错误处理原文迁入本文 §12。

## 6 命名统一

`Outcome` / `Complaint` / `Entry` / `Carried` / `Summary` / `Measured` 保持原名。**「door」一词
早已在 `ARCHITECTURE.md` §5 与 `crates/kusanagi/tests/door.rs` 中使用**，不是新造的词。

## 7 模块边界

```
lib.rs        模块索引与再导出
report.rs     Outcome —— 一个值，两种渲染
prose.rs      同一个值，说给人听
complaint.rs  Complaint —— 失败 + 稳定码 + 恢复命令
```

依赖：kernel / chain / grant / seal / site / waypoint + `serde` + `serde_json` + `thiserror`。
**六个内部 crate 全部是只读的类型来源**：door 引用 `Handle`、`Instant`、`Channel`、`Standing`、
`Certificate` 与各家的错误类型，不调用任何执行函数。

依赖方向：`kusanagi → door → {kernel, chain, grant, seal, site, waypoint}`。door 是叶子之上的
一层，没有反向边。

## 8 接口先行

```rust
pub enum Outcome { Identity{..}, Channels{..}, Invited{..}, Joined{..}, Sent{..},
                   Read{..}, Revoked{..}, Forgotten{..}, Examined{..}, Hosted{..} }

impl Outcome {
    pub fn summarise(name: &str, channel: &Channel, who: &Handle,
                     now: Instant, revoked: &Revocations) -> Summary;
    pub fn read<'a>(name: &str, author: &str, height: Option<u64>,
                    segments: impl IntoIterator<Item = (u64, &'a [u8])>) -> Self;
    pub fn examined(waypoint: &str, kind: &'static str, certificate: &Certificate) -> Self;
    pub fn render(&self, json: bool) -> String;
}

pub enum Carried { Text(String), Payload(String) }   // 二选一，不可能同时出现

pub enum Complaint { /* 18 个变体 */ }
impl Complaint {
    pub fn code(&self) -> &'static str;
    pub fn render(&self, json: bool) -> String;
}
```

`Outcome` 与 `Complaint` 都是 `#[non_exhaustive]`：动词集合会长，匹配它的下游不该因此崩。

## 9 工作流程

`kusanagi::run` 返回 `Result<Outcome, Complaint>` → `main.rs` 对两者各调一次 `render(json)` →
成功进 stdout，失败进 stderr 且退出码非零。**渲染是这条路径上唯一发生的事**，没有第二个格式化点。

## 10 实现逻辑

1. **一个值，两种渲染。** `render(json)` 分派到 `serde_json::to_string_pretty` 或 `prose::render`。
   两者从同一个值出发，所以人读到的与机器读到的**不可能对同一件事说两句话**。
2. **`Carried` 是枚举而不是两个字段。** 合法 UTF-8 装进 JSON 字符串本就无损；并存的十六进制
   只是把每条普通消息的体积翻倍。枚举让「两者同时出现」与「两者同时缺席」都不可表示。
3. **`Authority` 私有枚举。** 「持有能力 + 何时到期」与「什么都不持有 + 为什么」是互斥的两件事,
   在类型里分开，扁平化只在边界的 `Summary` 发生一次。
4. **恢复命令由种类推出，`Argument` 除外。** 只有写下那个旗标的地方知道该传什么，所以
   `Complaint::Argument` 是唯一自带 `instead` 文本的变体。

## 11 边界枚举

| 边界 | 行为 |
|---|---|
| 载荷不是 UTF-8 | `Carried::Payload(hex)`，散文里印「\<N bytes that are not text\>」 |
| 流为空 | `height: None`，散文「has written nothing yet」 |
| `serde_json` 序列化失败 | 退回 `{"error":"…"}` / `{"code":"…"}`，**不 panic**（工作区禁 `unwrap`） |
| 通道无 peer | `peer: None`，散文「(nobody met yet)」 |
| peer 被撤销 | `peer_refused` 有值；`can` 与 `refused` 恰好一个非空 |

## 12 错误处理

`Complaint` 十八个变体，每个带稳定码与**恢复命令**。「格式不对」是三个变体而不是一个——
`BadName`（你打的名字）、`BadInvitation`（你贴的那行）、`BadRecord`（你盘上的文件）——
三者**共用已公开的稳定码 `kusanagi.malformed`**，因为码是脚本匹配的东西，
而恢复命令必须各说各的：把名字打错的人被告知去拷贝邀请码，是把他送进另一个错误。四条与众不同：

- `kusanagi.argument` 是唯一把恢复文字**随变体带进来**的（`instead` 字段）。
- `seal.rejected` / `chain.*` / `segment.*` / `not_the_peer` 的建议是「留着这些字节并报告」——
  它们不是瞬时故障，而是损坏或干预。
- `grant.*` 的建议是「去要一份新的邀请」，因为本端无法自行修复权限。
- `waypoint.*` 的建议是 `kusanagi doctor <waypoint>`，把诊断交给会实测的那个动词。

`From<SiteError> for Complaint` 是唯一的跨层翻译：site 说「做什么时出了什么错」，door 说
「这叫什么码、该跑哪条命令」。合成一个类型就等于把 `kusanagi channels` 这句话写进一个没有动词的 crate。

## 13 依赖选型

| 依赖 | 理由 |
|---|---|
| `serde` + `serde_json` | **只用于 `--json` 输出**。任何被哈希或签名的东西一律手写编码 |
| `thiserror` | `Complaint` 的 `Display` 与 `#[from]`；仓库已有 |

不新增任何外部供应商：door 的依赖集是 kusanagi 原有依赖的子集。

## 14 硬编码声明

| 硬编码 | 意图 | 后续影响 |
|---|---|---|
| handle 缩写取前 12 字符 | 列表可读；完整值始终在 `handle`/`author` 字段里 | 渲染决策，不进记录 |
| `lasting()` 的 90s / 5 400s / 172 800s 分档 | 用还说得出意思的最大单位 | 只影响散文 |
| 列宽 16/8/30/40 | waypoint 放最后：它是唯一无宽度上限的列 | 只影响散文 |
| `payload` 用小写十六进制 | 全仓只有一套十六进制编解码（`kernel::wire`） | 体积翻倍，只落在非文本载荷上 |

## 15 影响面

- `crates/kusanagi/src/{assembly,membership,traffic,walk,world}.rs`：`use crate::…` → `use kusanagi_door::…`。
- `crates/kusanagi/src/lib.rs`：模块索引少三行，改为再导出。
- `crates/kusanagi/src/traffic.rs`：新增私有 `reported()`，承接 `--after` 过滤。
- `Cargo.toml` 工作区成员与依赖各加一行；`ARCHITECTURE.md` §5 crate 图加一行。

## 16 测试与约束

door 自身不带测试目录：它的每条性质都由 `crates/kusanagi/tests/` 从二进制外侧断言
（`door.rs` 帮助文案与参数、`payload.rs` 两种渲染、`complaint.rs` 码与恢复）。
**测试不进构建物**，也不为一个纯渲染层再复制一遍那些断言。

约束：本 crate 不得依赖 `kusanagi`（会成环），不得出现 IO、时钟或随机数。

## 17 文档同步

1. 本文。
2. `crates/kusanagi/kusanagi-SPEC.md` §7 模块边界、§12 指向本文。
3. `ARCHITECTURE.md` §5 crate 图。
4. 根 `Cargo.toml` 的 workspace 成员表。
