# kernel-SPEC

> `kusanagi-kernel` —— 网络的名词层。只有类型、编码与 seam 声明，没有 I/O，没有策略。
>
> 权威顺序：用户裁决 → `ARCHITECTURE.md` → 本文 → 代码与测试。本文先于代码改动。

## 1 需求拆解

阶段 0 的收口条件是「同机两个身份经一个本地目录投递一段，链可离线验证，无网络、无密码学」。拆成 kernel 必须独立完成、独立验收的五个最小单元：

| 单元 | 交付物 | 独立验收方式 |
|---|---|---|
| U1 定长摘要 | `Digest<N>`：渲染、解析、往返 | 属性测试：`parse(render(d)) == d`；非法长度/大写/非十六进制一律拒绝 |
| U2 身份 | `Handle`（32 字节） | 同名得同 `Handle`；异名得异 `Handle` |
| U3 段 | `Segment` 及其**规范字节** | 同一段编码两次字节相同；编解码往返恒等 |
| U4 段标识 | `SegmentId` = 域分隔 BLAKE3(规范字节) | 改动任意一个字段则 id 改变 |
| U5 投递地址与 seam | `DropAddr`、`Waypoint` trait、`PutOutcome` | trait 可被两个适配器实现（见 `waypoint-SPEC.md`） |

kernel **不**负责：追加与验证链（属 `chain`）、真正的存取（属 `waypoint`）、密钥与派生（阶段 1–2）。

## 2 验收标准

1. `cargo clippy --all-targets --all-features -- -D warnings` 零输出。
2. 非测试代码中不存在 `unwrap`/`expect`/`panic!`/裸索引/裸算术/`as` 转换。
3. `Segment::to_canonical_bytes` 对同一段的两次调用返回逐字节相同的结果。
4. `Segment::from_canonical_bytes(to_canonical_bytes(s)) == s` 对所有构造得出的段成立。
5. 截断、超长、尾部多余字节、payload 长度字段与实际长度不符——四种畸形输入各返回一个具名错误，且都不 panic。
6. `Segment::genesis` 与 `Segment::follows` 之外没有第二条构造路径；「index 非零却无前驱」与「index 为零却有前驱」在类型上写不出来。

## 3 假设与歧义

| 歧义 | 我的假设 | 何时失效 |
|---|---|---|
| 阶段 0 的 `Handle` 是什么 | 由名字经域分隔 BLAKE3 得到的 32 字节，**不是密钥** | 阶段 2 换成公钥，字节宽度不变，因此线路格式不动 |
| `DropAddr` 如何得出 | 阶段 0 用 `(author, index)` 公开派生，**故意可链接**，仅为让阶段 0 能跑通 | 阶段 1 换成 `HKDF(共享秘密, 序号)`，`address::public_v0` 届时删除而非保留 |
| payload 是否需要结构 | 阶段 0 视为不透明字节 | 阶段 3 之后由上层定义，kernel 永不解释它 |
| 是否需要序列化框架 | 不需要。规范字节由手写编码器产生 | 见 §13 |

`ARCHITECTURE.md` §2 要求「不可链接」是阶段 1 的收口条件，不是阶段 0 的。阶段 0 的地址派生是公开的，这一点必须在代码注释与本文同时写明，避免它被误当成成品。

## 4 现状分析

全新 crate，无既有代码。参照物是同作者的 sprawling：其 `memory` crate 的 Ledger 用 JSONL 分段加链式校验。本项目改为定长二进制规范字节，理由在 §10 步骤 2。

## 5 权威信源

| 事实 | 来源 |
|---|---|
| BLAKE3 是树形哈希，可用于内容寻址 | `blake3` crate 文档；iroh-blobs 采用同一选择 |
| 域分隔（domain separation）避免不同用途的哈希互相碰撞 | 通行密码工程实践；本项目对每一类摘要使用不同前缀 |
| 禁 panic / 禁裸算术 / 禁 `as` 的具体清单 | `rust-hardening` skill，已写入根 `Cargo.toml` 的 `[workspace.lints]` |
| 「一个适配器是假想的 seam，两个才是真的」 | sprawling `ARCHITECTURE.md` §4 |

## 6 命名统一

沿用 `ARCHITECTURE.md` §4.1 的六个词，一名一义：

| 代码标识符 | 文档词 | 含义 |
|---|---|---|
| `Segment` | Segment | 唯一会旅行的东西 |
| `SegmentId` | —— | 段的内容地址 |
| `DropAddr` | Drop | 恰好落一个 Segment 的不透明地址 |
| `Waypoint` | Waypoint | 能按 key 存取字节的东西 |
| `Handle` | —— | 身份 |

`Grant`、`Cohort`、`Bell` 在阶段 0 不出现——**没有实现的词不进代码**。

## 7 模块边界

kernel 是最内层，**无内部依赖**；外部依赖只有 `blake3` 与 `thiserror`。

```
lib.rs        仅模块索引与 crate 级文档，零逻辑
digest.rs     Digest<N>：渲染、解析、相等
handle.rs     Handle
segment.rs    Segment / SegmentId / 规范字节编解码
address.rs    DropAddr 与阶段 0 的公开派生
waypoint.rs   Waypoint trait（seam）、PutOutcome、WaypointError
error.rs      KernelError
```

数据流：`Segment` --规范字节--> `SegmentId`；`(Handle, index)` --阶段0派生--> `DropAddr`。kernel 内部不存在反向依赖。

## 8 接口先行

```rust
pub struct Digest<const N: usize>([u8; N]);          // Display = 小写十六进制；FromStr 校验长度与字符集
pub struct Handle(Digest<32>);                        // Handle::from_name(&str)
pub struct SegmentId(Digest<32>);
pub struct DropAddr(Digest<20>);

pub enum Link {                                       // 让两种非法状态写不出来
    Genesis,
    Follows { index: NonZeroU64, previous: SegmentId },
}

pub struct ChainHead { /* 私有 */ }                 // 只能由 Segment::head() 产生的见证
pub struct Segment { /* 私有字段 */ }
impl Segment {
    pub fn genesis(author: Handle, payload: Vec<u8>) -> Result<Self, SegmentError>;
    pub fn extend(author: Handle, payload: Vec<u8>, head: ChainHead) -> Result<Self, SegmentError>;
    pub fn head(&self) -> ChainHead;
    pub fn id(&self) -> SegmentId;
    pub fn index(&self) -> u64;
    pub fn previous(&self) -> Option<SegmentId>;
    pub fn to_canonical_bytes(&self) -> Vec<u8>;
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, SegmentError>;
}

pub enum PutOutcome { Stored, AlreadyPresent }        // 穷尽枚举，不是 bool
pub trait Waypoint {
    fn put_if_absent(&self, addr: &DropAddr, bytes: &[u8]) -> Result<PutOutcome, WaypointError>;
    fn get(&self, addr: &DropAddr) -> Result<Option<Vec<u8>>, WaypointError>;
}
```

**用类型消灭的四种非法状态**：`Link` 使「index 为 0 却带前驱」与「index 非 0 却无前驱」无法书写；`ChainHead` 字段私有且**只能由一个真实存在的段产出**，因此「链高与前驱 id 不相干」的配对无法构造；`Segment` 字段私有且只有两个构造器；`PutOutcome` 使「已存在」成为一个正常结果而非错误或布尔。

**为什么是 `extend(head)` 而不是 `follows(&previous)`**（实现中修正，理由记在此）：后者要求调用方持有整个前驱段，而一条百万段的链就会把百万段拖进内存——直接撞上 `ARCHITECTURE.md` §3 的「内存不随工作量增长」。`ChainHead` 只带 40 字节（id 与链高），且因为它无公开构造器，拿到一个 `ChainHead` 就等于拿到一个“这个段确实存在过”的证据。**安全性没有降低，内存从 O(n) 降到 O(1)。**

## 9 工作流程

发送方向：`Segment::genesis`/`follows` 构造段 → `to_canonical_bytes` 得到字节 → `SegmentId` 由字节导出 → `address::public_v0(author, index)` 得到地址 → 交给 `Waypoint::put_if_absent`。

接收方向：`Waypoint::get(addr)` 取回字节 → `from_canonical_bytes` 解析 → 交给 `chain` 验证。

kernel 只提供这条路上的名词与两次转换，不驱动流程。

## 10 实现逻辑

**步骤 1：`Digest<N>` 先行。** 三个标识符类型共享「定长字节 + 小写十六进制往返」这一条不变量。把它放在一处，是因为**解析器有一份就够了**；把它做成泛型常量参数，是因为 20 字节与 32 字节的差别只在长度。这不是语法改名——它拥有「非法长度必须被拒绝」这条策略。

**步骤 2：规范字节手写，不用序列化框架。** 哈希必须建立在一个**确定的**编码上；serde 的 JSON 字段序、浮点渲染、映射顺序都不是逐字节确定的，而本项目的完整性依赖于「同一段两次编码必然相同」。定长大端布局如下：

```
tag         1 字节   0 = Genesis, 1 = Follows
index       8 字节   大端；tag = 0 时恒为 0
previous   32 字节   仅当 tag = 1 时存在
author     32 字节
payload_len 4 字节   大端
payload    payload_len 字节
```

`Handle` 现在是名字的哈希、将来是公钥，两者都是 32 字节，所以**阶段 2 的替换不会改变线路格式**。这是有意的前向设计，不是投机的通用化。

**步骤 3：`SegmentId` 域分隔。** `BLAKE3(b"kusanagi.segment.v1" || 规范字节)`。前缀带版本，是因为格式若改，旧段的 id 必须与新段不同，否则两种格式会在同一个地址空间里相撞。

**步骤 4：解码器逐字段做边界检查。** 私有 `Reader` 持有偏移量，每次推进都走 `checked_add`，每次取字节都走 `slice::get`。解析完毕后**必须校验没有尾部剩余字节**——否则同一个段会有无穷多种带尾巴的表示，内容寻址随之失效。

**为什么优于替代**：用 serde + bincode 可少写约 80 行，但会把「字节确定性」这条承重不变量交给一个外部 crate 的版本策略去保证；用文本格式（如 sprawling 的 JSONL）可读性更好，但 §3 的 payload 是任意字节，文本格式需要额外转义层，且长度不定与 `ARCHITECTURE.md` §2 第 4 项的定长分桶目标相悖。

## 11 边界枚举

| 输入 | 期望 |
|---|---|
| 空字节切片 | `SegmentError::Truncated` |
| 只有 1 字节 tag | `SegmentError::Truncated` |
| tag 为 2 | `SegmentError::UnknownTag` |
| tag = 0 但 index 非 0 | `SegmentError::GenesisIndexNotZero` |
| payload_len 声明 1000 实际 10 字节 | `SegmentError::Truncated` |
| 完整段之后多 1 字节 | `SegmentError::TrailingBytes` |
| payload 长度超过 `u32::MAX` | 构造时 `SegmentError::PayloadTooLarge` |
| `previous.index()` 为 `u64::MAX` | 构造时 `SegmentError::ChainExhausted`（`checked_add` 返回 `None`） |
| `Digest::from_str` 收到大写十六进制 | `DigestParseError::NotLowercaseHex`——**大小写不做归一化**，因为标识符若有两种写法就有两个身份 |

并发：kernel 全部类型是值语义、无内部可变性，因此没有并发冲突面。

## 12 错误处理

三个错误枚举，各自 `#[non_exhaustive]`，均由 `thiserror` 生成 `Display`：

| 错误 | 谁抛 | 谁接 | 稳定错误码 |
|---|---|---|---|
| `DigestParseError` | `Digest::from_str` | CLI 参数解析层 | `digest.length` / `digest.charset` |
| `SegmentError` | 构造与解码 | `chain` 与 CLI | `segment.truncated` 等 |
| `WaypointError` | 适配器实现 | 调用方 | `waypoint.io` / `waypoint.rejected` |

每个错误提供 `code() -> &'static str`。稳定错误码是给 Agent 读的：`ARCHITECTURE.md` §6 要求「每个错误是类型化的：失败的动作、对象、稳定错误码，以及能恢复它的那条确切命令」。kernel 提供前三项，恢复命令由 CLI 层附加，因为只有那一层知道用户敲了什么。

kernel 内部不做恢复——它没有可恢复的东西，一切失败都是调用方的输入问题，一律 `Result` 上抛。

## 13 依赖选型

| 依赖 | 理由 | 替代方案与代价 | 维护成本 |
|---|---|---|---|
| `blake3` 1.8 | 树形哈希、SIMD 加速、可用于内容寻址；与将来 `depot` 的分块校验同源，全仓一个哈希 | SHA-256 更保守但更慢且无树形结构；BLAKE2 无 SIMD 优势 | 单一活跃上游，`default-features = false` 只取 `std` |
| `thiserror` 2 | 只生成 `Display` 与 `From`，不引入运行时 | 手写 `impl Display` 约多 60 行且易与错误码脱节 | 编译期，无运行时足迹 |

不引入 `serde`（理由见 §10 步骤 2）、不引入 `hex`（渲染用 `write!("{:02x}")`，解析用 `chunks_exact(2)`，共约 25 行，少一个依赖）。

## 14 硬编码声明

| 硬编码 | 意图 | 后续影响 |
|---|---|---|
| `b"kusanagi.segment.v1"` | 段 id 的域分隔前缀 | 规范字节布局若变更，此处必须同步升到 `v2`，否则两种格式的 id 会相撞 |
| `b"kusanagi.handle.v1"` | 阶段 0 由名字派生 Handle 的前缀 | 阶段 2 换成公钥时整条派生路径删除，前缀随之消失 |
| `b"kusanagi.drop.v0"` | 阶段 0 的公开地址派生前缀 | **`v0` 是刻意的**：它标记这是一条会被删除的路径，而不是一个会被升级的格式 |
| `DropAddr` 宽度 20 字节 | 160 位，足以抵抗生日攻击下的地址碰撞，同时让十六进制键长 40 字符 | 若改宽度，全部既存地址失效 |

## 15 影响面

阶段 0 无既有调用方。本 crate 落地后，`chain`、`waypoint`、`kusanagi` 三者全部依赖它，因此其公开接口的任何改动都要求同一次提交内修改这三者与本文。

## 16 测试与约束

| 测试 | 断言什么 |
|---|---|
| `digest_roundtrip` | 渲染后解析回原值 |
| `digest_rejects_uppercase` / `_wrong_length` / `_non_hex` | 三种非法输入各得具名错误 |
| `handle_from_name_is_deterministic` | 同名同值，异名异值 |
| `segment_canonical_bytes_are_stable` | 同一段编码两次逐字节相同 |
| `segment_roundtrip` | 编解码恒等（genesis 与 follows 各一） |
| `segment_id_changes_with_每个字段` | 作者、payload、前驱、index 各改一次，id 必变 |
| `decode_rejects_*`（6 个） | §11 表中每一行 |
| `extend_links_to_previous` | `previous() == Some(prev.id())` 且 `index == prev.index + 1` |

约束：非测试代码零 panic 构造；`missing_docs` 为 warn 且必须清零；测试模块以 `#[allow(..., reason = "test code")]` 局部放开。

## 17 文档同步

完成后必须同步：

1. 本文——若接口与实现产生分歧，先改本文。
2. `ARCHITECTURE.md` §4.2 的 crate 行数表——填入实测行数。
3. `ARCHITECTURE.md` §4.5 的 seam 表——`Waypoint` 行的「声明于」确认为 `kernel::waypoint`。
4. 根 `AGENTS.md` 的模块表——新增本 crate 的六个模块文件。
