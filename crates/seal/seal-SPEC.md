# seal-SPEC

> `kusanagi-seal` —— 地址派生与内容封装。整个网络的隐私主张落在这一个 crate 上。
>
> 权威顺序：用户裁决 → `ARCHITECTURE.md` → 本文 → 代码与测试。本文先于代码改动。

## 1 需求拆解

`ARCHITECTURE.md` §3 第 2 项要求「宿主看不出谁与谁通信」。拆成三个可独立验收的最小单元：

| 单元 | 交付物 | 独立验收 |
|---|---|---|
| U1 通道秘密 | `Secret`，以及由它派生的 `Stream` | 同一 `(secret, author)` 得同一 `Stream`；不同 author 得不同 `Stream` |
| U2 地址与密钥派生 | `derive(stream, index) -> (DropAddr, Key)` | 一千个高度得一千个互不相同的地址；换一个 secret 则整张地址表不相交 |
| U3 封装 | `seal` / `open` | 往返恒等；任意一位翻转被拒；换一个高度的密钥打不开 |
| U4 定长信封（`Veil`） | `veil::pad` / `veil::unpad`，以及常量 `DROP` | 任何长度的明文封出来都是 4 096 字节；非零填充被拒 |
| U5 秘密不残留 | `Secret` / `Stream` / `Key` 的 `ZeroizeOnDrop` | 三个类型都满足 `ZeroizeOnDrop` 约束；`Secret` 与 `Stream` 不可比较 |

**不负责**：谁被允许写（属 `grant`）、字节存在哪里（属 `waypoint`）、段的结构（属 `kernel`）。

## 2 验收标准

1. `derive` 对同一输入总是返回同一对值，且不读时钟、不读随机数。
2. 一千次派生得到一千个不同的 `DropAddr`（`secret.rs` 的 `a_thousand_addresses_are_a_thousand_addresses`）。
3. 同一通道内两个作者在每个高度都不碰撞（`the_two_lanes_of_one_channel_never_collide`，覆盖 0..64）。
4. 封装后的字节不含明文（`the_sealed_form_does_not_contain_the_plain_form`）。
4a. 任何长度的明文封出的密文都恰好 `DROP = 4 096` 字节（`every_drop_is_the_same_size_whatever_it_carries`）；长度不对的字节一律不开（`bytes_that_are_not_one_drop_long_never_open`）。
4b. 填充区任意一个非零字节使 `unpad` 返回 `Rejected`（`a_pad_that_carries_anything_is_refused`）。
5. 密文任意一位翻转后 `open` 返回 `OpenFailed::Rejected`（`every_flipped_byte_is_refused`，逐字节遍历）。
6. `Secret`、`Stream`、`Key` 的 `Debug` 不打印字节。
7. 端到端：`crates/kusanagi/tests/unlinkable.rs` 从宿主视角断言一百段之间无可关联特征。

## 3 假设与歧义

| 歧义 | 假设 | 何时失效 |
|---|---|---|
| 秘密怎么来 | v0.0.1 由邀请方随机生成并整份交给受邀方 | 阶段 4 上 Noise 握手后改为协商产生，本 crate 的接口不变 |
| nonce 怎么定 | 与 key 一起从同一次派生里取，因此 `(key, nonce)` 对每个 drop 唯一 | 若将来一个 key 需覆盖多条消息，此处必须改为显式计数器 |
| 是否需要 AAD | 不需要 | 地址已经通过密钥分离与密文绑定：把密文搬到另一个地址，那里派生出的是另一把钥匙 |
| `derive` 的签名 | 比 `ARCHITECTURE` 早期草案多一层 `Stream` | 见 §7 的理由；少了这一层，共享同一 secret 的两方在每个高度都会抢同一个地址 |

## 4 现状分析

阶段 0 的 `kernel::address::public_v0` 由 `(author, index)` 公开派生，**故意可链接**，用于让骨架跑通。本 crate 落地时该函数连同其全部调用方一并删除；全仓 grep 不到 `public_v0`。这是替换，不是并存——两条派生路径同时存在，就等于隐私主张有一个随时可以被绕过的后门。

## 5 权威信源

| 事实 | 来源 |
|---|---|
| `derive_key(context, material)` 是 BLAKE3 自带的 KDF 模式，context 应为硬编码、全局唯一、含日期的字符串 | blake3 crate 文档 |
| ChaCha20-Poly1305 的 nonce 在同一密钥下重复使用会导致灾难性失败 | RFC 8439 §3 |
| 一个密钥只加密一条消息时，固定 nonce 是安全的 | 同上；本设计据此把 nonce 也做成派生量 |

## 6 命名统一

`Secret`、`Stream`、`Key` 与 `ARCHITECTURE.md` §4 的词表一致。`Stream` 是本 crate 引入的新词，含义唯一：**一个作者在一条通道内的 drop 序列**。

## 7 模块边界

```
lib.rs        模块索引
secret.rs     Secret / Stream / derive —— 隐私主张的全部代码在这里
envelope.rs   Key / seal / open —— 标准件的装配
```

依赖：`kernel`（`DropAddr`、`Handle`）、`blake3`、`chacha20poly1305`。不依赖 `waypoint`、`chain`、`grant`。

**为什么多一层 `Stream`。** `ARCHITECTURE` 早期草案写的是 `derive(secret, index)`。两方共享同一个 secret，若地址只由 `(secret, index)` 决定，则双方在每个高度都指向同一个地址，而 drop 是一次性写入的——先到者占位，后到者永远写不进去。把作者的 `Handle` 掺进派生，双方各得一条互不相交的车道，且因为 `Handle` 是公开的，任何一方都能算出对方的车道，无需协商。

## 8 接口先行

```rust
pub const DROP: usize = 4_096;            // 每个密封 drop 在线上的字节数
pub struct Secret([u8; 32]);              // 私有字段；Debug 为 "Secret(redacted)"；ZeroizeOnDrop；不可比较
pub struct Stream([u8; 32]);              // 同上
pub struct Key { bytes: [u8; 32], nonce: [u8; 12] }   // 无 Clone，无公开构造器，ZeroizeOnDrop

impl Secret {
    pub const fn from_bytes(bytes: [u8; 32]) -> Self;
    pub const fn as_bytes(&self) -> &[u8; 32];
    pub fn stream(&self, author: &Handle) -> Stream;
}

pub fn derive(stream: &Stream, index: u64) -> (DropAddr, Key);
pub fn seal(key: &Key, plain: &[u8]) -> Result<Vec<u8>, OpenFailed>;
pub fn open(key: &Key, sealed: &[u8]) -> Result<Vec<u8>, OpenFailed>;
```

**用类型消灭的非法状态**：`Key` 没有公开构造器，因此一把钥匙只能来自一次 `derive`，「同一把钥匙加密两条消息」在调用方写不出来；`Key` 不实现 `Clone`，因此它也不能被复制到第二处使用。**`Secret` 与 `Stream` 删掉了 `PartialEq`**：全仓没有任何一处比较两个秘密，而 derive 出来的比较耗时取决于前缀匹配了多少字节——把 trait 拿掉是让这个错误写不出来，而不是写下来不要犯。

## 9 工作流程

```
发送：secret.stream(&me) → derive(stream, height) → (addr, key)
      → seal(key, segment.to_canonical_bytes()) → waypoint.put_if_absent(addr, sealed)
接收：secret.stream(&peer) → derive(stream, height) → (addr, key)
      → waypoint.get(addr) → open(key, sealed) → Segment::from_canonical_bytes
```

## 10 实现逻辑

**步骤 1：三个 context 字符串各管一件事。**

```
stream  = derive_key("kusanagi 2026-01-01 stream: one author's lane in a channel", secret ‖ author)
address = derive_key("kusanagi 2026-01-01 drop address", stream ‖ index_be)
key‖nonce = derive_key("kusanagi 2026-01-01 drop key and nonce", stream ‖ index_be)
```

地址与密钥用**两次独立派生**而不是同一次 XOF 输出的两段。后者在密码学上同样成立，但地址要交给不受信任的宿主而密钥绝不能：分成两个 context，这件事就变成一个关于域分隔的论证，而不是一个关于哈希内部结构的论证。

**步骤 2：地址取 XOF 的前 20 字节，而不是截断 32 字节哈希。** `finalize_xof().fill(&mut [0u8; 20])` 是「向 BLAKE3 要恰好这么宽的输出」，这正是可扩展输出的用途；顺带避免了一次切片。

**步骤 3：nonce 也是派生量。** 每个 drop 一把新钥匙，所以 nonce 取什么都安全；派生它只多两行，却让「零 nonce」这个需要读者停下来验算的写法从代码里消失。

**步骤 5：一个尺寸，没有梯子。** 密文长度是宕主不需要任何密码分析就能拿到的一个事实，而一份记录的长度剖面在加密之后原封不动地存活。`veil::pad` 把「4 字节长度前缀 + 段的规范字节 + 零填充」凑成固定的 `DROP - 16` 字节。

**为什么不用分档梯子。** 梯子仍然告诉宕主落在哪一档；更重要的是，梯子上每一个边界都是一个**有人选的参数**，而两个选得不一样的构建会把各自的用户分成两个可区分的人群——可配置性就是匿名集分割。PADMÉ（Nikitin et al., PETS 2019）是尺寸跨越数量级时的已发表答案，在这里是错的：段本来就被封顶在一个 drop 内，整个取值范围装得下一个桶。

**步骤 6：填充必须校验为零。** 不校验的填充是一条完美的隐蔽信道：它在认证信封**内部**（因此 tag 与签名都看不见它），它恰好在消息短时最长，而下游永远不会去看它一眼。一个被改过的构建可以每条消息带几 KB 地把身份种子运出去，而全仓测试照绿。校验一次比较，关掉它。

**步骤 7：尺寸的权威在 `kernel`，且由编译期断言钉住。** `veil.rs` 里一句 `const _: () = assert!(ROOM == MAX_SEGMENT, …)`：信封尺寸与 `kernel` 允许的最大段一旦错开，整个 workspace 编译不过——而不是等到运行时 `seal` 拒绝一条已经签好名的段。

**步骤 4：封装整个段，而不是段的 payload。** 段的规范字节里明文携带作者 `Handle`。若只封 payload，宿主可以按作者把 drop 分组，上面所有的地址不可链接性一文不值。这条是 `ARCHITECTURE.md` §8 记录在案的决定。

## 11 边界枚举

| 输入 | 期望 |
|---|---|
| 空明文 | 正常往返，密文仍为 `DROP` 字节 |
| 任何长度不等于 `DROP` 的密文 | `OpenFailed::Rejected`，在解密之前 |
| 恰好 `DROP` 字节的随机字节 | `OpenFailed::Rejected` |
| 填充区非零 | `OpenFailed::Rejected` |
| 长度前缀超过信封本身 | `OpenFailed::Rejected` |
| 明文大于 `MAX_SEGMENT` | `OpenFailed::Oversize`；由编译期断言保证对一个段不可达 |
| 用另一个 index 的 key 打开 | `OpenFailed::Rejected` |
| 用另一个 secret 派生的 key 打开 | `OpenFailed::Rejected` |
| `index = u64::MAX` | 正常派生；本 crate 不定义链高上限 |

无并发面：全部为纯函数与值类型。

## 12 错误处理

单一错误枚举 `OpenFailed`，两个变体：

| 变体 | 何时 | 稳定码 |
|---|---|---|
| `Rejected` | 字节不是在这把钥匙下封装的——无论是钥匙错、位翻转、长度不对、填充带了东西，还是从别的地址搬过来的 | `seal.rejected` |
| `Unusable` | 密码套件拒绝这把钥匙（在本 crate 的构造下不可达，但不 panic） | `seal.unusable` |
| `Oversize` | 明文装不进一个 drop。与 `Rejected` 分开是安全的：它不是伪造，而是本端调用方越了本端 `kernel` 已经在执行的上限，对端永远触不到 | `seal.oversize` |

**四种失败合并成一个答案是刻意的**：告诉攻击者伪造在哪一步失败，等于送他一个判定预言机。

## 13 依赖选型

| 依赖 | 理由 | 替代方案与代价 |
|---|---|---|
| `chacha20poly1305` 0.10 | RustCrypto 的 AEAD，纯 Rust、无 C 工具链、软件实现常数时间 | `aes-gcm` 在无 AES-NI 的设备上更慢且更难做到常数时间 |
| `zeroize` 1 | 三个秘密类型出作用域即擦除。**它本来就在依赖树里**（`ed25519-dalek` 用它擦签名私钥），直接依赖它不增加任何需要审计的代码 | 手写擦除会被优化器删掉，除非用 volatile 写入或内存屏障，而那正是这个 crate 在做的事 |
| `blake3`（已在全仓） | 自带 KDF 模式，全仓一个哈希原语 | 引入 `hkdf` 会多一套需要审计的构造，且多一个依赖 |

## 14 硬编码声明

**`DROP = 4 096`。** 它是线路格式的一部分，不是可调参数：改它的构建与没改的构建不能互相读写，而两个尺寸不同的人群是宕主分得开的两个人群。选 4 KiB 的理由是文件系统、对象存储与 TLS 记录都已经在用这个量级，且它把 `MAX_PAYLOAD` 留在 3 935 字节——对代理之间的一条消息绰绰有余。代价是一条 11 字节的消息也占 4 KiB，这是为属性 4a 付的带宽，已计入。

三个 context 字符串（见 §10 步骤 1）。它们**是线路格式的一部分**：改动其中任何一个，之后派生出的全部地址与密钥都会改变，两个 context 不一致的端点连对方的信箱在哪都算不出来。日期部分按 BLAKE3 的约定保留，用于将来需要换代时给出一个新的、明确不同的 context。

## 15 影响面

`waypoint::conformance`、`waypoint::probe`、`kusanagi::walk`、`kusanagi::assembly` 全部依赖 `derive`。改动 §10 的任何一步即改变全部地址，等同于换一个网络。

## 16 测试与约束

`secret.rs` 6 个、`envelope.rs` 7 个，共 13 个单元测试，覆盖 §2 全部条目。约束：非测试代码零 panic 构造；三个秘密类型的 `Debug` 必须被测试钉住（`a_secret_never_prints_itself`）。

端到端的不可关联断言不在本 crate，而在 `crates/kusanagi/tests/unlinkable.rs`——因为要断言的是「宿主看到的全部记录」，那需要一个真实的宿主目录。

## 17 文档同步

1. 本文。
2. `ARCHITECTURE.md` §3 的隐私表、§4 的词表（`Stream`）、§5 的行数表。
3. `README.md` 的「How it works」段。
