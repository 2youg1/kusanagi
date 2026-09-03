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
| U4 身份 | `Handle` / `VerifyingKey` / `Signer` / `Signature` | 种子决定身份；`Handle` 是公钥的 BLAKE3，不是公钥；签名只在自己的 `VerifyingKey` 下验证通过 |
| U5 段 | `Segment` 及其规范字节（含签名） | 同一段编码两次字节相同；解码即验签，验签用的公钥由调用方交来 |
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
7. 把段的作者字段换成另一个 handle，解码得 `NotTheAuthor`（`a_segment_cannot_be_re_authored`）。
8. `Signer` 的 `Debug` 不打印种子。
9. **`Handle` 里没有公钥。** 全网唯一一处从公钥到 handle 的映射是 `VerifyingKey::handle`，且不可逆；`Handle` 上不存在 `verify`。
10. 用另一个人的公钥解码一个段，得 `NotTheAuthor{expected, found}` 而不是 `NotAuthentic`——「你拿错了钥匙」与「这个签名是假的」是两件事。

## 3 假设与歧义

| 歧义 | 假设 | 何时失效 |
|---|---|---|
| `Handle` 是什么 | `BLAKE3("kusanagi.handle.v1" ‖ 公钥)` 的 32 字节 | 永不失效——换签名算法只换公钥的宽度，handle 恒为 32 字节 |
| 段里带的是名字还是钥匙 | **名字。** 公钥只出现在必须当场验签的地方 | 永不失效；见 §10 步骤 6 |
| 非法曲线点能否成为 `VerifyingKey` | 能——`from_bytes` 是无误的字节封装，真伪只在 `verify` 那一刻判定 | 永不失效；这样的公钥什么也验证不了 |
| payload 是否有结构 | 不透明字节，kernel 永不解释 | 永不失效 |
| 是否需要序列化框架 | 不需要，规范字节由手写编码器产生 | 见 §10 步骤 2 |
| 时间从哪来 | 由 `Clock` 传入，kernel 自己不读 | 永不失效 |

## 4 现状分析

相对骨架期的三处实质变化，各有理由：

1. **`Handle::from_name` 删除，`Handle` 变为公钥。** 名字的哈希能指认写者，不能证明写者；`grant` 按 handle 指定 subject，没有签名则权限只能约束自愿遵守的软件。
2. **段增加 64 字节签名，域从 `v1` 升到 `v2`。** 解码即验签，因此不存在「未验证的段」这种状态供下游忘记检查。
3. **`address::public_v0` 删除。** 它由 `(author, index)` 公开派生，故意可链接；`seal` 落地后两条派生路径并存等于给隐私主张留一个后门。全仓 grep 不到 `public_v0`。

`Digest` 的十六进制解析与 `Segment` 的私有 `Reader` 上提到 `wire`，因为 `grant` 与 `kusanagi::channel` 也要解码不受信任的字节——第二个十六进制解析器就是「同一个标识符是否相等」的第二个答案。

## 5 权威信源

| 事实 | 来源 |
|---|---|
| BLAKE3 可用于内容寻址，`derive_key` 是其 KDF 模式 | `blake3` crate 文档 |
| ML-DSA-87 的公钥 2 592 / 签名 4 627 / 私钥 4 896 字节；`try_sign_with_seed` 是标准自己的确定性变体 | FIPS 204，及 `fips204` crate 文档 |
| 域分隔避免不同用途的哈希互撞 | 通行密码工程实践 |
| 禁 panic / 禁裸算术 / 禁 `as` 的清单 | `rust-hardening`，已写入根 `Cargo.toml` |

## 6 命名统一

| 代码标识符 | 文档词 | 含义 |
|---|---|---|
| `Segment` | Segment | 唯一会旅行的东西 |
| `SegmentId` | —— | 段的内容地址 |
| `DropAddr` | Drop | 恰好落一个 Segment 的不透明地址 |
| `Waypoint` | Waypoint | 能按 key 存取字节的东西 |
| `Handle` | —— | 身份的**名字**：公钥的 BLAKE3，32 字节，与签名算法无关 |
| `VerifyingKey` / `Signer` / `Signature` | —— | 公钥 / 私钥 / 签名 |
| `Instant` / `Clock` | —— | 时刻 / 时刻的来源 |

## 7 模块边界

```
lib.rs        仅模块索引与 crate 级文档
wire.rs       Hex / unhex / Reader / Incomplete —— 字节与文本的唯一权威
digest.rs     Digest<N> 与 identifier! 宏
identity.rs   Handle / VerifyingKey / Signer / Signature / NotAuthentic
trail.rs      Trail / Reveal / Commitment —— 一条流上的一次性证明
link.rs       Link / ChainHead —— 段在链上的位置与认证它的东西
segment/      mod.rs 是类型与规范字节，refusal.rs 是失败的分类
payload.rs    Payload 与三个尺寸常量：一个段能装多少
address.rs    DropAddr（只声明，不派生）
clock.rs      Instant / Clock / FixedClock
waypoint.rs   Waypoint trait、PutOutcome、WaypointError
```

kernel 无内部依赖；外部依赖只有 `blake3`、`fips204`、`subtle`、`zeroize`、`thiserror`。

**`identity.rs` 四型同居一文件**是因为它们互相依赖：`Signer::verifying_key()` 产出 `VerifyingKey`，`VerifyingKey::handle()` 产出 `Handle`，`VerifyingKey::verify` 消费 `Signature`。拆开只会在文件之间制造一个环。

**`payload.rs` 从 `segment.rs` 分出来**是因为 `segment.rs` 撞上了 400 行的单文件上限，而「一个段能装多少」本身是个完整的概念：`MAX_SEGMENT` 由 `seal::veil` 的信封定死，`MAX_PAYLOAD` 由它减出来，`Payload` 是把这条上限确立一次的那个类型。上限不上调，文件就得拆。

## 8 接口先行

```rust
pub struct Hex<'a>(pub &'a [u8]);                     // Display 即渲染，不分配
pub fn unhex(text: &str) -> Result<Vec<u8>, HexError>;
pub struct Reader<'a> { /* 私有 */ }                  // take / take_array / take_byte / take_u32 / take_u64

pub struct Handle(Digest<32>);                        // 只是名字：没有 verify
pub struct VerifyingKey([u8; 32]);                    // handle(&self) -> Handle；verify(&self, msg, &Signature)
pub struct Signer(SigningKey);                        // from_seed / verifying_key / handle / sign；无 Clone
pub struct Signature(Digest<64>);

pub enum Link { Genesis, Follows { index: NonZeroU64, previous: SegmentId } }
pub struct ChainHead { /* 私有，无公开构造器 */ }
pub struct Segment { /* 私有字段 */ }
impl Segment {
    pub fn genesis(signer: &Signer, payload: Vec<u8>) -> Result<Self, SegmentError>;
    pub fn extend(signer: &Signer, payload: Vec<u8>, head: ChainHead) -> Result<Self, SegmentError>;
    pub fn from_canonical_bytes(bytes: &[u8], author: &VerifyingKey)
        -> Result<Self, SegmentError>;                // 含验签
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

**用类型消灭的非法状态**：`Link` 使「index 为 0 却带前驱」与「index 非 0 却无前驱」写不出来；`ChainHead` 无公开构造器，因此「链高与前驱 id 不相干」的配对无法构造；`Segment` 只有两个构造器且都签名，加上解码即验签，**未签名的段不存在**；`PutOutcome` 使「已存在」成为正常结果而非布尔或错误；`Signer` 不实现 `Clone`。`Handle` 没有 `verify`，因此**「拿一个名字去验签」写不出来**——从前那个签名可以只对着段自己带的那 32 字节验证通过、而调用方从不表态期待谁的写法，现在编译不过。

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
genesis:  tag 1 + index 8 + author 32 + commit 32 + payload_len 4 + payload + signature 64
follows:  tag 1 + index 8 + previous 32 + author 32 + reveal 32 + commit 32 + payload_len 4 + payload
```

两种形状的固定开销都是 141 字节，因此上面的信封只看见一个长度。

**只有链的第一段被签名，且签名不覆盖 payload。** 签的是 `域 ‖ author ‖ commit ‖ 0`：足以阻止持有通道秘密的对端抢占 0 号高度，不足以给任何人定一句话的罪。0 号以上的每一段由下面那一段承诺的一次性证明认证——`reveal` 哈希后必须等于前一段的 `commit`。伪造它、或抢在作者之前写到某个高度，都需要一个 BLAKE3 原像。

**步骤 3：解码时重建 body 再验签。** 解析出字段后重新编码 body 并对其验签，于是**规范性成为真实性的一部分**：一串能解出这个段、却不是这个段所编码出的字节，其签名消息不同，因而被拒。不需要额外的规范性检查。

**步骤 4：两个域分隔前缀。** `kusanagi.segment.v2` 用于 id，`kusanagi.segment.v2.sign` 用于签名。分开是为了让「一个段的标识符」永远不可能被误当成「作者签署过的东西」，两个方向都不行。

**步骤 5：`Payload` 缓存长度。** `len` 与 `bytes.len()` 是同一事实，在构造时确立一次且此后不可变，这让 `to_canonical_bytes` 保持全函数——否则段的身份在每个调用点都成了一个可失败的问题。

**步骤 6：段里带名字，公钥由调用方交来。** `author` 字段是 handle，恒为 32 字节；`from_canonical_bytes` 多收一个 `&VerifyingKey`，先核对 `key.handle() == author`，再验签。三个后果，都是要的：

1. **线路格式与签名算法脱钩。** 换成 ML-DSA-44 时公钥从 32 字节变成 1 312 字节，而段的布局、地址派生与 cairn 文件名一个字节都不动。Trail 落地后的 `v3` 追随段根本不带签名，届时它若还背着一份公钥，就是每条消息白付 1 280 字节。
2. **解码方必须说出它期待谁。** 从前解码只证明「有人签了这串字节」，期待谁是调用方各自记得去比的一步；现在这一步在解码器里，忘不掉。
3. **一个段只对已经认识作者的人自证。** 陌生人拿到密文也没有公钥可验——这与 Trail 要达到的可否认性同向，而不是相反。

公钥从哪来，是 kernel 之外的事：`grant` 的每一步自带签发者的公钥（一份凭证必须能说服陌生人），`site` 的 channel 记录存着对端的公钥（一条流只需说服认识的那一个人）。

## 11 边界枚举

| 输入 | 期望 |
|---|---|
| 空切片 / 只有 tag | `Truncated` |
| tag 为 2 | `UnknownTag { tag: 2 }` |
| tag = 0 但 index 非 0 | `GenesisIndexNotZero` |
| tag = 1 但 index 为 0 | `FollowsIndexZero` |
| payload_len 声明 1000 实际 5 | `Truncated` |
| 完整段之后多 1 字节 | `TrailingBytes { count: 1 }` |
| payload 超过 `MAX_PAYLOAD` | 构造时 `PayloadTooLarge` |
| 前驱高度为 `u64::MAX` | `ChainExhausted` |
| 任意一位被翻转 | 解码失败（`Truncated`/`UnknownTag`/`NotTheAuthor`/`NotAuthentic` 之一） |
| 用别人的公钥解码一个完好的段 | `NotTheAuthor { expected, found }`，且不做验签 |
| `Digest::from_str` 收到大写 | `Hex(Charset)`——**不做大小写归一化** |

并发：全部为值语义、无内部可变性。

## 12 错误处理

| 错误 | 谁抛 | 稳定码 |
|---|---|---|
| `HexError` | `unhex` | `hex.odd_length` / `hex.charset` |
| `Incomplete` | `Reader` | 由调用方包装 |
| `DigestParseError` | `Digest::from_str` | `digest.length` / `digest.width` / 转发 `hex.*` |
| `NotAuthentic` | `VerifyingKey::verify` | `identity.not_authentic` |
| `SegmentError` | 构造与解码 | `segment.*`（十个） |
| `WaypointError` | 适配器实现 | `waypoint.io` / `waypoint.overwrite_not_refused` / `waypoint.unusable_address` / `waypoint.redirected` / `waypoint.timeout` |

kernel 内部不做恢复——一切失败都是调用方的输入问题，一律 `Result` 上抛。恢复命令由 CLI 层附加，因为只有那一层知道用户敲了什么。

## 13 依赖选型

| 依赖 | 理由 | 替代方案与代价 |
|---|---|---|
| `blake3` 1.8 | 树形哈希、SIMD、可用于内容寻址，全仓一个哈希原语 | SHA-256 更保守但更慢且无树形结构 |
| `fips204` 0.4，特性 `ml-dsa-87` | 纯 Rust、无 C 工具链，且 `try_sign_with_seed` 能拿到确定性签名——规范字节规则要求一条消息只有一个签名 | `ring` 会引入 C 与汇编构建；随机化签名会让同一个段每次编码出不同字节 |
| `subtle` 2 / `zeroize` 1 | 定宽标识符的恒时比较，以及种子出作用域即擦除。**它们原本在 `ed25519-dalek` 下面，那个依赖走后改为直接依赖** | 手写比较会泄露首字节匹配长度；手写擦除会被优化器删掉 |
| `thiserror` 2 | 只生成 `Display` 与 `From`，无运行时足迹 | 手写约多 60 行且易与错误码脱节 |

不引入 `serde`（§10 步骤 2）、不引入 `hex`（`wire` 约 40 行，少一个依赖）。

## 14 硬编码声明

| 硬编码 | 意图 | 后续影响 |
|---|---|---|
| `b"kusanagi.handle.v1"` | handle 派生的域分隔前缀。哈希而非 `derive_key`：handle 是公开标识符，不是密钥材料 | 改它则全网身份、地址与 cairn 文件名一起换代 |
| `b"kusanagi.segment.v3"` | 段 id 的域分隔前缀。**作者字段从公钥改成 handle 没有升版**：域分隔前缀区分的是*布局*，而布局一个字节没动；且旧字节要被当成新段读通，需要一个 BLAKE3 原像。`v3` 留给 Trail | 布局变更须同步升版，否则两种格式的 id 相撞 |
| `b"kusanagi.segment.v3.sign"` | 签名域 | 同上 |
| `MAX_SEGMENT = 65_516` | 一个段的规范字节最长多长。**这才是被选定的那个数**：`kusanagi_seal::veil` 把每个密封 drop 固定在 65 536 字节，减去 16 字节认证 tag 与 4 字节长度前缀，剩下的就是它 | 两边一旦错开，`veil.rs` 里的 `const _: () = assert!(…)` 使整个 workspace 编译不过 |
| `MAX_PAYLOAD = 65_375` | 单段载荷上限，**是减出来的不是选出来的**：`MAX_SEGMENT` 减去 141 字节固定开销。更大的负载属于尚不存在的分块机制 | 超限一律拒绝而非静默切分 |
| `OVERHEAD = 141` | 两种形状恰好相等：genesis 的 32 字节 commit 与 64 字节签名，正好抵掉 follows 的 32 字节 previous、32 字节 reveal 与 32 字节 commit | 布局变了这三个数要一起算 |
| `DropAddr` 宽 20 字节 | 160 位，抗生日碰撞，且文本键长 40 字符 | 改宽度则全部既存地址失效 |

## 15 影响面

`chain`、`seal`、`grant`、`waypoint`、`kusanagi` 全部依赖本 crate。公开接口的任何改动都要求同一次提交内修改这五者与本文。

## 16 测试与约束

**解码器健壮性**（`tests/robust.rs`，H4）：任意字节只产生答案不产生崩溃；声明的载荷长度在分配之前
与到货长度比较；一个段只有一种拼法。第三条画出一条边界——**签名覆盖的每一位翻转都被拒，载荷里的
不被拒**：签名刻意不覆盖载荷（那会把「说过什么」变成可转让的证据），护住载荷的是它所在的密封信封。

单元测试：`wire` 5、`digest` 5、`identity` 9、`segment` 12、`clock` 3、`address` 1、`waypoint`（由适配器的 conformance 覆盖）。其中三个是承重的：逐位翻转规范字节无一能解码，换作者即失去真实性，以及 handle 里取不出公钥。

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
