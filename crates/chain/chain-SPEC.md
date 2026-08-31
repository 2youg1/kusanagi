# chain-SPEC

> `kusanagi-chain` —— 链的规则层。判断一串段是否构成一条合法的链，以及两个段是否构成一次分叉。

## 1 需求拆解

| 单元 | 交付物 | 独立验收 |
|---|---|---|
| U1 流式验证 | `Verifier`：逐段接受，常量内存 | 一万段验证过程中驻留状态不随段数增长 |
| U2 一次性验证 | `verify(iter)` | 合法链返回 head；六类非法各返回具名错误 |
| U3 分叉检出 | `fork(&a, &b) -> Option<Fork>` | 同作者同链高不同 id 判为分叉；其余情形一律不是 |

`chain` **不**负责：存取（`waypoint`）、构造段（`kernel`）、决定谁有权写（`grant`，阶段 2）。

## 2 验收标准

1. `Verifier` 的驻留状态是 `Option<(Handle, ChainHead)>`——**结构上**与段数无关，不是「测出来内存没涨」。
2. 六种非法输入各得一个具名错误：非创世开头、中途再现创世、链高跳跃、前驱不匹配、作者变更、链高耗尽。
3. `fork` 对「同作者同链高同 id」返回 `None`——**重复投递不是分叉**，这是最容易写错的一格。
4. clippy 零输出；非测试代码零 panic 构造。

## 3 假设与歧义

| 歧义 | 假设 | 何时失效 |
|---|---|---|
| 一条链是否只属于一个作者 | 是。作者中途变更即错误 | 阶段 2 起作者由签名证明，此规则不变但由密钥执行 |
| 段是否必须按序抵达 | `Verifier` 要求按序。乱序重排是调用方的事 | 阶段 3 的 `post` 引入乱序缓冲；`chain` 仍只接受有序输入 |
| 分叉如何被发现 | 阶段 0 由调用方拿两个段来问 | 阶段 5 由 `cohort` 在 gossip 中自动比对链头 |

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

`kusanagi` CLI 的 `verify` 子命令依赖本 crate。阶段 3 的 `post` 将在乱序缓冲之后调用它。`Verifier` 的公开接口变更需同步这两处。

## 16 测试与约束

按 §11 表逐行一个测试，外加：`verify` 对一条十段链返回正确链头；`Verifier` 在错误发生后**状态不被污染**（错误段不会成为新链头）。

约束：`accept` 出错时 `self` 必须保持出错前的状态——否则调用方无法在丢弃坏段后继续。

## 17 文档同步

1. 本文。
2. `ARCHITECTURE.md` §4.2 的行数表。
3. `kusanagi-SPEC.md` 的 `verify` 子命令一节。
