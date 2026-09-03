# chain-SPEC

> `kusanagi-chain` —— 链的规则层。判断一串段是否构成一条合法的链，以及两个段是否构成一次分叉。

## 1 需求拆解

| 单元 | 交付物 | 独立验收 |
|---|---|---|
| U1 流式验证 | `Verifier`：逐段接受，常量内存 | 一万段验证过程中驻留状态不随段数增长 |
| U2 一次性验证 | `verify(iter)` | 合法链返回 head；六类非法各返回具名错误 |
| U3 分叉检出 | `fork(&a, &b) -> Option<Fork>` | 同作者同链高不同 id 判为分叉；其余情形一律不是 |

`chain` **不**负责：存取（`waypoint`）、构造段（`kernel`）、决定谁有权写（`grant`）。

## 2 验收标准

1. `Verifier` 的驻留状态是 `Option<(Handle, ChainHead)>`——**结构上**与段数无关，不是「测出来内存没涨」。
2. 六种非法输入各得一个具名错误：非创世开头、中途再现创世、链高跳跃、前驱不匹配、作者变更、链高耗尽。
3. `fork` 对「同作者同链高同 id」返回 `None`——**重复投递不是分叉**，这是最容易写错的一格。
4. clippy 零输出；非测试代码零 panic 构造。

## 3 假设与歧义

| 歧义 | 假设 | 何时失效 |
|---|---|---|
| 一条链是否只属于一个作者 | 是。作者中途变更即错误 | 永不失效。作者今天已由签名证明（`kernel::Segment`），规则不变，执行它的是密钥 |
| 段是否必须按序抵达 | `Verifier` 要求按序。乱序重排是调用方的事 | 永不失效。调用方是 `kusanagi::walk`；即使它某天并发乱序地**取**，**验证仍串行**，`chain` 只接受有序输入 |
| 分叉如何被发现 | 由调用方拿两个段来问 | `cohort` 落地后由名册持有者自动比对链头 |

## 4 现状分析

`kernel` 已提供 `Segment::head()` 与 `ChainHead`（无公开构造器）。因此本 crate 无需自己维护「链高与前驱一致」——那已经是类型事实，本 crate 只验证**相邻两段之间**的关系。

## 5 权威信源

`ARCHITECTURE.md` §3.4「序号即链高」：寻址序号与哈希链段高是同一个 n，因此去重、定序、重放、分叉检测四件事由同一结构给出。本 crate 是那句话的实现。

## 6 命名统一

`Verifier` / `ChainError` / `Fork`。不引入 "validator"、"checker" 等同义词。`ChainHead` 沿用 `kernel` 的定义，不重新声明。

## 7 模块边界

依赖 `kernel` 与 `thiserror`，无其他。

```
lib.rs      模块索引，零逻辑
verify.rs   Verifier、ChainError、verify()
fork.rs     Fork、fork()
```

## 8 接口先行

```rust
pub struct Verifier { /* Option<(Handle, ChainHead)> */ }
impl Verifier {
    pub const fn new() -> Self;
    pub fn accept(&mut self, segment: &Segment) -> Result<(), ChainError>;
    pub fn head(&self) -> Option<ChainHead>;
    pub fn author(&self) -> Option<Handle>;
}
pub fn verify<'a>(segments: impl IntoIterator<Item = &'a Segment>) -> Result<Verifier, ChainError>;

pub struct Fork { /* author, index, left, right —— 私有字段加访问器 */ }
pub fn fork(left: &Segment, right: &Segment) -> Option<Fork>;
```

`fork` 返回 `Option<Fork>` 而非 `bool`：**分叉的证据就是分叉本身**，调用方需要的是那四个字段，不是一个是或否。

## 9 工作流程

`Verifier::new()` → 对每个段调用 `accept` → 首段必须是创世且此后记住 `(author, head)` → 每个后续段依次比对作者、链高、前驱 → 全部通过后 `head()` 给出链头。

## 10 实现逻辑

**步骤 1：状态就是一个 `Option`。** 「还没见过任何段」与「见过，链头在此」是仅有的两种状态，用 `Option<Seen>` 表达。这不是省事——它是「常量内存」这条设计律在类型上的形状。

**步骤 2：用 `(已见状态, 段的前驱)` 的四元组合分派。** 四种组合恰好覆盖：`(无, 无)` 合法起始；`(无, 有)` 非创世开头；`(有, 无)` 中途再现创世；`(有, 有)` 进入三项比对。**穷尽匹配，没有 `_ =>` 兜底**，因为兜底会把将来新增的状态悄悄吞掉。

**步骤 3：比对顺序是作者、链高、前驱。** 先作者，因为作者错了则后两项的比较毫无意义，报错也会误导；先链高后前驱，因为链高错误的诊断信息（期望 n 实得 m）对调用方比哈希不匹配更可用。**错误的用处取决于它先报哪一个。**

**步骤 4：`fork` 是纯函数。** 它不需要 `Verifier`，因为分叉的定义只涉及两个段：同作者、同链高、不同 id。写成自由函数而非 `Verifier` 的方法，是因为它的两个输入通常来自两条不同的链。

**为何优于替代**：把验证做成「收集成 `Vec` 再检查」少写约 20 行，但驻留内存变成 O(n)，直接违反 `ARCHITECTURE.md` §3 的内存指标；把分叉检测塞进 `Verifier` 会迫使它保存历史，同样破坏 O(1)。

## 11 边界枚举

| 输入 | 期望 |
|---|---|
| 空迭代器 | `Ok`，`head()` 为 `None` |
| 只有一个创世段 | `Ok`，head 为该段 |
| 首段是 `Follows` | `ExpectedGenesis` |
| 第二段是 `Genesis` | `UnexpectedGenesis` |
| 链高 0,1,3 | `IndexGap { expected: 2, found: 3 }` |
| 前驱指向别的段 | `PreviousMismatch` |
| 第二段换了作者 | `AuthorChanged` |
| 链头已在 `u64::MAX` | `Exhausted` |
| `fork(同一个段, 它自己)` | `None` |
| `fork(同作者同高不同 payload)` | `Some` |
| `fork(不同作者)` | `None` |

无并发面：`Verifier` 是值，`accept` 取 `&mut self`，编译器已保证独占。

## 12 错误处理

`ChainError` 单一枚举，`#[non_exhaustive]`，每个变体带 `code()`：`chain.expected_genesis`、`chain.unexpected_genesis`、`chain.index_gap`、`chain.previous_mismatch`、`chain.author_changed`、`chain.exhausted`。

不做任何恢复：一条链要么合法要么不合法，没有「部分合法」这种中间状态可供降级。上抛给 CLI，由那一层附加恢复命令。

## 13 依赖选型

只有 `kusanagi-kernel` 与 `thiserror`。不引入迭代器辅助库——`IntoIterator` 已足够。

## 14 硬编码声明

无硬编码常量。所有阈值来自 `kernel`（`u64::MAX` 是类型上界，不是本 crate 的选择）。

## 15 影响面

唯一的调用方是 `kusanagi::walk`：它取回一条流并把段按序喂进 `Verifier`。`Verifier` 的公开接口变更需同步那一处。

## 16 测试与约束

按 §11 表逐行一个测试，外加：`verify` 对一条十段链返回正确链头；`Verifier` 在错误发生后**状态不被污染**（错误段不会成为新链头）。

约束：`accept` 出错时 `self` 必须保持出错前的状态——否则调用方无法在丢弃坏段后继续。

## 17 文档同步

1. 本文。
2. `ARCHITECTURE.md` §4.2 的行数表。
3. `kusanagi-SPEC.md` 的 `verify` 子命令一节。

---

## 附：v0.0.1 的两处变化

**一、0 号以上的段不再带签名，认证由 Trail 承担。** `Verifier::accept` 多一道检查：段出示的 `reveal` 哈希后必须等于前一段发布的 `commit`，不等则 `ProofRefused`。这是 0 号以上的段唯一一处变成「可信」的地方——解码器只解析它们。`Cairn` 因此多带 32 字节承诺，`WIDTH` 73 → 105：从 cairn 续读的人手里已经没有下面那一段了，承诺是它接受下一段所需的全部。可否认性的验收在 `tests/deniable.rs`，那里真的伪造一份通得过的抄本。

**二、作者身份现在是可证的。** `kernel::Handle` 由「名字的哈希」变成 Ed25519 公钥，段带签名，`Segment::from_canonical_bytes` 解码即验签。对本 crate 的影响只有一处，而且是收紧：`Verifier::accept` 判定的 `AuthorChanged`，此前意为「有人在这条链上声称了另一个名字」，现在意为「有人在这条链上出示了另一把钥匙签的段」——**声称变成了证明**。测试改为用 `Signer::from_seed` 构造作者，逻辑一行未动。

**二、`walk` 没有放进本 crate。** 从 waypoint 上取回一条流并逐段验证，需要同时用到 `waypoint`（取）、`seal`(解封) 与 `chain`(验序)，它是三者的组合而不是任何一个的内部逻辑，因此留在 `kusanagi::walk`。把它搬进来会迫使本 crate 依赖 `seal` 与 `waypoint`，并复制一份几乎与 `Complaint` 相同的错误枚举——为省下一百行而多写五十行，且给「读一条流出了什么错」制造第二个权威。

验收未变：12 个单元测试全过，`Verifier` 的驻留状态仍是一个 `Option`。


---

## 附：`Cairn`——把折叠的状态写下来

**动机不在本 crate。** 读取方每次从高度零开始走，就会把一条流的全部地址按升序、连续地报给主机；`seal` 精心推导出的互不关联，在访问日志面前当场作废。修法是让下一次读取从上一次停下的地方继续，而「上一次停在哪」正是 `Verifier` 唯一的驻留状态。

**因此不新造类型，而是把已有的私有 `Seen` 升为一等概念。** `Cairn { author, head }` 就是原来的 `Seen`，`Verifier.seen: Option<Cairn>`。内存里的状态与磁盘上的记录是同一个东西，「这条流验证到哪」因此只有一个定义。

```rust
pub struct Cairn { /* author: Handle, head: ChainHead */ }
impl Cairn {
    pub const WIDTH: usize;                    // 73 = 1 + 32 + 32 + 8
    pub const fn author(&self) -> Handle;
    pub const fn head(&self) -> ChainHead;
    pub const fn next_index(&self) -> Option<u64>;
    pub fn to_bytes(&self) -> Vec<u8>;
    pub fn from_bytes(&[u8]) -> Result<Self, CairnError>;
}
impl Verifier {
    pub const fn resume(cairn: Cairn) -> Self;
    pub const fn cairn(&self) -> Option<Cairn>;
}
```

**三处决策。**

1. **`Cairn::new` 是 `pub(crate)`。** 从外部得到 cairn 的唯一途径是 `Verifier::cairn`，而它只有在其下的段都验过之后才给得出。这保住了「cairn 是一个关于过去的断言，且那个断言为真」。
2. **`next_index` 返回 `Option` 而不是 `Result`。** 链高已达 `u64::MAX` 时其上没有段，这是关于链的事实而不是失败；调用方应当被回答而不是被打断。这也省掉一个跨 crate 的错误映射。
3. **续读不重验其下的段，这不是放宽。** 那正是 `Tier::AckFirstSeen` 承诺的缓解措施：允许覆写的主机无法修改读者已经走过的历史，因为读者不再回头看。

**为何优于替代**：让 `Verifier` 多一个「从记录恢复」的内部状态，会得到一个把 `(index, id)` 拆开保存的分支——那是 `ChainHead` 的第二份实现，也就是一条规则两个权威。

验收：18 个单元测试，含版本、宽度、截断、补零四类畸形记录各自被具名拒绝。测试助手 `cairn_at` 取 `u8` 而非 `u64`，因为它真的会把链走一遍——类型是阻止「要求一个走几小时的高度」的东西，这个错误本文件已经犯过一次。
