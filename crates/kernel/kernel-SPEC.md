# kernel-SPEC

> `kusanagi-kernel` —— 网络的名词层。只有类型、编码与 seam 声明，没有 I/O，没有策略。
>
> 权威顺序：用户裁决 → `ARCHITECTURE.md` → 本文 → 代码与测试。本文先于代码改动。

## 1 需求拆解

kernel 要独立完成、独立验收的七个最小单元：

| 单元 | 交付物 | 独立验收 |
|---|---|---|
| U1 文本编码 | `Hex` / `unhex`，全网唯一的字节-文本规则 | 逐字节往返；大写、奇数长度各得具名错误 |
| U2 有界读取 | `Reader` | 越界读取返回 `Incomplete` 而不是 panic；失败的读取不消耗游标 |
| U3 定长标识符 | `Digest<N>` 与 `identifier!` | 渲染、解析、比较、哈希一套实现供全部标识符类型复用 |
| U4 身份 | `Handle` / `Signer` / `Signature` | 种子决定身份；签名只在自己的 handle 下验证通过 |
| U5 段 | `Segment` 及其规范字节（含签名） | 同一段编码两次字节相同；解码即验签 |
| U6 段标识 | `SegmentId` | 任一字段改变则 id 改变 |
| U7 两个 seam | `Waypoint` / `Clock`（含 `FixedClock`） | 见 `waypoint-SPEC.md`；`FixedClock` 是 `Clock` 的第二实现 |

kernel **不**负责：链的规则（`chain`）、地址派生（`seal`）、权限（`grant`）、真正的存取（`waypoint`）。

## 2 验收标准

1. `cargo clippy --all-targets --all-features -- -D warnings` 零输出。
2. 非测试代码中不存在 `unwrap`/`expect`/`panic!`/裸索引/裸算术/`as` 转换。
3. `to_canonical_bytes` 对同一段两次调用逐字节相同。
4. `from_canonical_bytes(to_canonical_bytes(s)) == s`。
5. 截断、超长、尾随字节、payload 长度撒谎、未知 tag、genesis 带高度——六种畸形输入各得一个具名错误，均不 panic。
6. **规范字节逐位翻转，无一能解码成功**（`every_flipped_payload_byte_breaks_the_signature`）。
7. 把段的作者字段换成另一个 handle，解码得 `NotAuthentic`（`a_segment_cannot_be_re_authored`）。
8. `Signer` 的 `Debug` 不打印种子。

## 3 假设与歧义

| 歧义 | 假设 | 何时失效 |
|---|---|---|
| `Handle` 是什么 | Ed25519 验证公钥的 32 字节 | 换签名算法时失效；宽度若变，线路格式随之变 |
| 非法曲线点能否成为 `Handle` | 能——解析 handle 是文本操作，保持无误；真伪只在 `verify` 那一刻判定 | 永不失效；这样的 handle 什么也验证不了 |
| payload 是否有结构 | 不透明字节，kernel 永不解释 | 永不失效 |
| 是否需要序列化框架 | 不需要，规范字节由手写编码器产生 | 见 §10 步骤 2 |
| 时间从哪来 | 由 `Clock` 传入，kernel 自己不读 | 永不失效 |

## 4 现状分析

相对阶段 0 的三处实质变化，各有理由：

1. **`Handle::from_name` 删除，`Handle` 变为公钥。** 名字的哈希能指认写者，不能证明写者；`grant` 按 handle 指定 subject，没有签名则权限只能约束自愿遵守的软件。
2. **段增加 64 字节签名，域从 `v1` 升到 `v2`。** 解码即验签，因此不存在「未验证的段」这种状态供下游忘记检查。
3. **`address::public_v0` 删除。** 它由 `(author, index)` 公开派生，故意可链接；`seal` 落地后两条派生路径并存等于给隐私主张留一个后门。全仓 grep 不到 `public_v0`。

`Digest` 的十六进制解析与 `Segment` 的私有 `Reader` 上提到 `wire`，因为 `grant` 与 `kusanagi::channel` 也要解码不受信任的字节——第二个十六进制解析器就是「同一个标识符是否相等」的第二个答案。

## 5 权威信源

| 事实 | 来源 |
|---|---|
| BLAKE3 可用于内容寻址，`derive_key` 是其 KDF 模式 | `blake3` crate 文档 |
| `verify_strict` 拒绝小阶点与签名可延展 | `ed25519-dalek` 文档 |
| 域分隔避免不同用途的哈希互撞 | 通行密码工程实践 |
| 禁 panic / 禁裸算术 / 禁 `as` 的清单 | `rust-hardening`，已写入根 `Cargo.toml` |

## 6 命名统一

| 代码标识符 | 文档词 | 含义 |
|---|---|---|
| `Segment` | Segment | 唯一会旅行的东西 |
| `SegmentId` | —— | 段的内容地址 |
| `DropAddr` | Drop | 恰好落一个 Segment 的不透明地址 |
| `Waypoint` | Waypoint | 能按 key 存取字节的东西 |
| `Handle` / `Signer` / `Signature` | —— | 公钥 / 私钥 / 签名 |
| `Instant` / `Clock` | —— | 时刻 / 时刻的来源 |

## 7 模块边界

```
lib.rs        仅模块索引与 crate 级文档
wire.rs       Hex / unhex / Reader / Incomplete —— 字节与文本的唯一权威
digest.rs     Digest<N> 与 identifier! 宏
identity.rs   Handle / Signer / Signature / NotAuthentic
segment.rs    Segment / SegmentId / ChainHead / Link / 规范字节
address.rs    DropAddr（只声明，不派生）
clock.rs      Instant / Clock / FixedClock
waypoint.rs   Waypoint trait、PutOutcome、WaypointError
```

kernel 无内部依赖；外部依赖只有 `blake3`、`ed25519-dalek`、`thiserror`。

**`identity.rs` 三型同居一文件**是因为它们互相依赖：`Signer::handle()` 产出 `Handle`，`Handle::verify` 消费 `Signature`。拆开只会在文件之间制造一个环。

## 8 接口先行

```rust
pub struct Hex<'a>(pub &'a [u8]);                     // Display 即渲染，不分配
pub fn unhex(text: &str) -> Result<Vec<u8>, HexError>;
pub struct Reader<'a> { /* 私有 */ }                  // take / take_array / take_byte / take_u32 / take_u64

pub struct Handle(Digest<32>);                        // verify(&self, msg, &Signature) -> Result<(), NotAuthentic>
pub struct Signer(SigningKey);                        // from_seed / seed / handle / sign；无 Clone
pub struct Signature(Digest<64>);

pub enum Link { Genesis, Follows { index: NonZeroU64, previous: SegmentId } }
pub struct ChainHead { /* 私有，无公开构造器 */ }
pub struct Segment { /* 私有字段 */ }
impl Segment {
    pub fn genesis(signer: &Signer, payload: Vec<u8>) -> Result<Self, SegmentError>;
    pub fn extend(signer: &Signer, payload: Vec<u8>, head: ChainHead) -> Result<Self, SegmentError>;
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, SegmentError>;  // 含验签
}

pub struct Instant(u64);
pub trait Clock { fn now(&self) -> Instant; }
pub struct FixedClock { /* 私有 */ }

pub enum PutOutcome { Stored, AlreadyPresent }
pub trait Waypoint {
    fn put_if_absent(&self, addr: &DropAddr, bytes: &[u8]) -> Result<PutOutcome, WaypointError>;
    fn get(&self, addr: &DropAddr) -> Result<Option<Vec<u8>>, WaypointError>;
}
```

**用类型消灭的非法状态**：`Link` 使「index 为 0 却带前驱」与「index 非 0 却无前驱」写不出来；`ChainHead` 无公开构造器，因此「链高与前驱 id 不相干」的配对无法构造；`Segment` 只有两个构造器且都签名，加上解码即验签，**未签名的段不存在**；`PutOutcome` 使「已存在」成为正常结果而非布尔或错误；`Signer` 不实现 `Clone`。

**为什么 `extend(head)` 而不是 `follows(&previous)`**：后者要求调用方持有整个前驱段，百万段的链就把百万段拖进内存，直接撞上「内存不随工作量增长」。`ChainHead` 只带 40 字节，且因无公开构造器，拿到一个就等于拿到「这个段确实存在过」的证据。安全性没降低，内存从 O(n) 降到 O(1)。

## 9 工作流程

```
发送：Segment::genesis/extend（签名）→ to_canonical_bytes → 交给 seal 封装
接收：seal 解封 → from_canonical_bytes（验签）→ 交给 chain 验证顺序
```

kernel 只提供这条路上的名词与两次转换，不驱动流程。

## 10 实现逻辑

**步骤 1：`wire` 先行。** 十六进制的渲染做成 `Hex` 这个 `Display` 视图而不是返回 `String` 的函数：它可以直接写进别人的 formatter 不必分配，也顺带避开了「把 nibble 映射成字符时那个不可达的兜底分支」——那种分支写出来就是一句谎话。

**步骤 2：规范字节手写，不用序列化框架。** 哈希与签名必须建立在确定的编码上；serde 的字段序与映射顺序不是逐字节确定的。定长大端布局：

```
tag          1   0 = Genesis, 1 = Follows
index        8   大端；tag = 0 时恒为 0
previous    32   仅 tag = 1
author      32
payload_len  4   大端
payload      payload_len
signature   64   作者对以上全部（前缀域分隔后）的签名
```

**步骤 3：解码时重建 body 再验签。** 解析出字段后重新编码 body 并对其验签，于是**规范性成为真实性的一部分**：一串能解出这个段、却不是这个段所编码出的字节，其签名消息不同，因而被拒。不需要额外的规范性检查。

**步骤 4：两个域分隔前缀。** `kusanagi.segment.v2` 用于 id，`kusanagi.segment.v2.sign` 用于签名。分开是为了让「一个段的标识符」永远不可能被误当成「作者签署过的东西」，两个方向都不行。

**步骤 5：`Payload` 缓存长度。** `len` 与 `bytes.len()` 是同一事实，在构造时确立一次且此后不可变，这让 `to_canonical_bytes` 保持全函数——否则段的身份在每个调用点都成了一个可失败的问题。

## 11 边界枚举

| 输入 | 期望 |
|---|---|
| 空切片 / 只有 tag | `Truncated` |
| tag 为 2 | `UnknownTag { tag: 2 }` |
| tag = 0 但 index 非 0 | `GenesisIndexNotZero` |
| tag = 1 但 index 为 0 | `FollowsIndexZero` |
| payload_len 声明 1000 实际 5 | `Truncated` |
| 完整段之后多 1 字节 | `TrailingBytes { count: 1 }` |
| payload 超过 64 KiB | 构造时 `PayloadTooLarge` |
| 前驱高度为 `u64::MAX` | `ChainExhausted` |
| 任意一位被翻转 | 解码失败（`Truncated`/`UnknownTag`/`NotAuthentic` 之一） |
| `Digest::from_str` 收到大写 | `Hex(Charset)`——**不做大小写归一化** |

并发：全部为值语义、无内部可变性。

## 12 错误处理

| 错误 | 谁抛 | 稳定码 |
|---|---|---|
| `HexError` | `unhex` | `hex.odd_length` / `hex.charset` |
| `Incomplete` | `Reader` | 由调用方包装 |
| `DigestParseError` | `Digest::from_str` | `digest.length` / `digest.width` / 转发 `hex.*` |
| `NotAuthentic` | `Handle::verify` | `identity.not_authentic` |
| `SegmentError` | 构造与解码 | `segment.*`（九个） |
| `WaypointError` | 适配器实现 | `waypoint.io` / `waypoint.overwrite_not_refused` / `waypoint.unusable_address` |

kernel 内部不做恢复——一切失败都是调用方的输入问题，一律 `Result` 上抛。恢复命令由 CLI 层附加，因为只有那一层知道用户敲了什么。

## 13 依赖选型

| 依赖 | 理由 | 替代方案与代价 |
|---|---|---|
| `blake3` 1.8 | 树形哈希、SIMD、可用于内容寻址，全仓一个哈希原语 | SHA-256 更保守但更慢且无树形结构 |
| `ed25519-dalek` 2.1 | 纯 Rust、无 C 工具链、有 `verify_strict` | `ring` 会引入 C 与汇编构建 |
| `thiserror` 2 | 只生成 `Display` 与 `From`，无运行时足迹 | 手写约多 60 行且易与错误码脱节 |

不引入 `serde`（§10 步骤 2）、不引入 `hex`（`wire` 约 40 行，少一个依赖）。

## 14 硬编码声明

| 硬编码 | 意图 | 后续影响 |
|---|---|---|
| `b"kusanagi.segment.v2"` | 段 id 的域分隔前缀 | 布局变更须同步升版，否则两种格式的 id 相撞 |
| `b"kusanagi.segment.v2.sign"` | 签名域 | 同上 |
| `MAX_PAYLOAD = 65_536` | 单段上限；更大的负载属于尚不存在的分块机制 | 超限一律拒绝而非静默切分 |
| `DropAddr` 宽 20 字节 | 160 位，抗生日碰撞，且文本键长 40 字符 | 改宽度则全部既存地址失效 |

## 15 影响面

`chain`、`seal`、`grant`、`waypoint`、`kusanagi` 全部依赖本 crate。公开接口的任何改动都要求同一次提交内修改这五者与本文。

## 16 测试与约束

34 个单元测试：`wire` 5、`digest` 5、`identity` 7、`segment` 12、`clock` 3、`address` 1、`waypoint`（由适配器的 conformance 覆盖）。其中两个是承重的：逐位翻转规范字节无一能解码，以及换作者即失去真实性。

约束：非测试代码零 panic 构造；`missing_docs` 必须清零；测试模块以 `#[allow(..., reason = "test code")]` 局部放开。

## 17 文档同步

1. 本文。
2. `ARCHITECTURE.md` §4 词表、§5 行数表、§6 seam 表、§8「签名」条目。
3. `AGENTS.md` 若模块清单变化。


---

## 附：见证之外的第二种来源

`ChainHead` 此前只有一个 `pub(crate)` 构造器，理由写在它自己的文档里：**持有一个链头意味着持有过那个段**，这正是 `Segment::extend` 只带 40 字节却不可能让链高与前驱互相矛盾的原因。

现在多了 `ChainHead::recorded`，来源更弱：它是本端点从自己磁盘上读回的、关于自己曾持有某段的记录。两者同为 40 字节、同一个类型，所以这个差别只在那一处可见，而且是写出来的而非藏起来的。

准许它存在的论证只有一条，条件性地：**链头的每一次使用都是一次比较**——链不接上它就被拒，从它延伸出的段由本端点签名、并被任何自身链不同意的读者拒绝。因此一个被篡改的记录只能让本端点拒绝本该接受的链，不可能让它接受伪造的段。这个不对称一旦不再成立，这个构造器就必须删除。

被否决的替代方案是把最后一个段整个存下来：它解码时自验签名，因此不需要放宽任何不变量。否决理由不在 kernel——见 `ARCHITECTURE.md` §8 与 `crates/kusanagi/tests/at_rest.rs`，代价是每条 channel 最近一条消息的明文永久留在磁盘上。
