# grant-SPEC

> `kusanagi-grant` —— 可离线验证、只能衰减、自带过期的授权。本网络的权限只以这一种形式存在。
>
> 权威顺序：用户裁决 → `ARCHITECTURE.md` → 本文 → 代码与测试。本文先于代码改动。

## 1 需求拆解

`ARCHITECTURE.md` §1 的判据里含「权限受限」，§8 记录了「衰减是格上的交」。拆成四个最小单元：

| 单元 | 交付物 | 独立验收 |
|---|---|---|
| U1 能力格 | `Ability` / `Abilities` / `Scope`，以及 `meet` | `meet` 是最大下界：对全域穷举验证交换律、幂等性与包含关系 |
| U2 签发与衰减 | `Grant::issue` / `attenuate` | 三级链可建；请求超出所持时得到所持而非报错 |
| U3 验证 | `Grant::verify` / `permits` | 换根、断链、越权、过期、伪签名各得一个具名错误并指出位置 |
| U4 撤销 | `Revocations` | 撤第二级则第三级立即失效，且第一级不受影响 |

**不负责**：撤销信息如何传播（那是传输问题，`Revocations` 作为参数传入）、谁是根（调用方决定）、权限之外的身份语义。

## 2 验收标准

1. 三级衰减链验证通过，`holder()` 为最末 subject（`a_three_step_chain_verifies_to_its_holder`）。
2. 撤销中间一级，则该级与其下全部失效，其上不受影响（`revoking_the_middle_kills_everything_below_it`、`revoking_a_leaf_leaves_its_ancestors_alone`）。
3. 请求比所持更宽时，结果等于所持（`asking_for_more_than_you_hold_yields_what_you_hold`）。
4. 过期时间只能提前，不能推后（`an_expiry_can_only_come_forward`）。
5. 非持有者不能继续转授（`only_the_holder_may_delegate_onward`）。
6. 线路格式往返恒等；尾随字节、空链、超长链各得具名错误。
7. 属性测试：任意请求序列下，任一跳的 scope 都在其上一跳与根之内（`crates/grant/tests/attenuation.rs`）。
8. 逐位翻转线路字节，验证必失败（同上）。

## 3 假设与歧义

| 歧义 | 假设 | 何时失效 |
|---|---|---|
| 时间从哪来 | 作为 `Instant` 参数传入，本 crate 不读时钟 | 永不失效；这是 `ARCHITECTURE.md` §7 的法则 |
| 撤销从哪来 | 作为 `Revocations` 参数传入 | 有了 cohort 名册后，撤销可以随名册发布，但本 crate 的签名不变 |
| 链能多深 | `MAX_STEPS = 8` | 需要更深的组织结构应当由根另发一份 grant，而不是加长链 |
| 未知能力位 | 拒绝（fail-closed） | 永不失效；忽略未知位等于批准一件自己无法评估的事 |

## 4 现状分析

骨架期没有权限模型。本 crate 是第一版。前提是身份可证：没有签名，grant 里的 subject 只是一个声称，`permits` 就只能约束自愿遵守的软件。

**一节里带的是签发者的公钥与接受者的名字，这个不对称是规则。** 一份 grant 要说服一个两边都不认识的人——这就是「离线可验证的凭证」的含义——所以每一跳必须自带验自己签名所需的东西。subject 在这一节里不证明任何事，因此只被命名；持有人出示 grant 时一并出示自己的公钥，`permits` 是让名字与钥匙对上的那一处。与之对照，一个 *segment* 不带公钥，因为它只需说服已经被引荐给作者的那一个人（`kernel-SPEC.md` §10 步骤 6）。该改动记在 `ARCHITECTURE.md` §8。

## 5 权威信源

| 事实 | 来源 |
|---|---|
| ML-DSA-87 的签名 4 627 字节、公钥 2 592，一节因此是 7 293 字节 | FIPS 204；宽度由 `kernel` 导出，测试不得写死 |
| 「衰减是格上的交」这一表述与 `attenuate(g,c) ⊑ g` 的定律 | `ARCHITECTURE.md` §8 与其前身提案 §5.5 |

## 6 命名统一

`Grant` 取自 `ARCHITECTURE.md` §4 词表。链的一节称 `Step`——不用 `Link`，因为 `kernel::Link` 已经指「段在链中的位置」，一名一义。

## 7 模块边界

```
lib.rs          模块索引
scope.rs        Ability / Abilities / Scope —— 格与它唯一的收窄操作
step.rs         Step / StepId —— 一跳的签名与定长编码
chain.rs        Grant —— 签发、衰减、验证、编解码，以及 kani 证明
revocation.rs   Revocations
error.rs        GrantError
```

依赖：`kernel`（`Handle`、`VerifyingKey`、`Signer`、`Signature`、`Instant`、`Reader`）、`blake3`、`thiserror`。不依赖 `waypoint`、`seal`、`chain`。

## 8 接口先行

```rust
pub enum Ability { Send, Read }                       // 封闭集合
pub struct Abilities(u8);                             // 集合；meet = 按位与
pub struct Scope { abilities: Abilities, expires_at: Instant }

impl Scope {
    pub fn meet(&self, other: &Self) -> Self;         // 最大下界
    pub fn is_within(&self, wider: &Self) -> bool;
    pub fn permits(&self, ability: Ability, now: Instant) -> bool;
}

impl Grant {
    pub fn issue(root: &Signer, subject: &Handle, scope: Scope) -> Self;
    pub fn attenuate(&self, holder: &Signer, subject: &Handle, request: Scope) -> Result<Self, GrantError>;
    pub fn verify(&self, root: &Handle, now: Instant, revoked: &Revocations) -> Result<Scope, GrantError>;
    pub fn permits(&self, root: &Handle, presenter: &Handle, ability: Ability,
                   now: Instant, revoked: &Revocations) -> Result<(), GrantError>;
    pub fn to_canonical_bytes(&self) -> Vec<u8>;
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, GrantError>;
}
```

**用类型消灭的非法状态**：`attenuate` 内部把新 scope 定义为 `held.meet(request)`，因此「衰减产生了更宽的授权」这条路径在代码里**不存在**，而不是被检查后拒绝。`Step` 的字段全私有且只能由 `sign` 或 `read` 产生。

## 9 工作流程

```
签发：root 用自己的 Signer 对 (root, subject, scope, parent=None) 签名 → 一节链
衰减：持有者用自己的 Signer 对 (holder, next, held.meet(request), parent=上一节 id) 签名 → 追加一节
验证：从根开始逐节走：根匹配 → 父链接匹配 → `issuer()` == 上一节 subject → scope ⊑ 上一节

`Step::issuer()` 产出的是 handle，即存储的公钥的 BLAKE3，所以这三道比较全部发生在名字上，与签名算法无关；公钥只在 `check_signature` 里被用一次。
      → 签名有效 → 未被撤销；走完后检查最末 scope 是否过期
```

检查顺序是「结构 → 密码学 → 策略」，这样报出来的是**最早真正出错的那一件事**，而不是一个正确但指错方向的结论。

## 10 实现逻辑

**步骤 1：能力用位集合，因为需要的是集合运算。** 项目原则是「穷尽 enum 优于布尔标志」，而这里要表达的本来就是一个集合，`meet` 就是按位与。`Ability` 仍是穷尽 enum，位由它的 `bit()` 决定，外部无法发明新能力。

**步骤 2：未知位拒绝而不是忽略。** `Abilities::from_bits` 对任何本版本未定义的位返回 `UnknownAbility`。忽略未知位的验证器会安静地批准一件它无法评估的授权。

**步骤 3：Step 定长 170 字节。** 定长意味着 `Grant` 的编码只需要一个计数字节，解码时不需要对攻击者提供的长度做算术。root 节的 parent 字段必须全零，否则同一含义会有多种字节表示，`StepId` 随之不唯一。

**步骤 4：撤销不做级联，因为验证本来就从根走。** 撤销一节之后，任何经过它的链在走到那一节时即失败；没有需要传播的状态，也没有需要重签的东西。这是「一条规则一个权威」在此处的形态。

**步骤 5：`permits` 是一次调用而不是四次。** 调用方真正的问题是「这个人现在能不能做这件事」。拆成 verify + holder + expiry + ability 四步交给调用方组合，就是把四个忘记其中一步的机会送出去。

## 11 边界枚举

| 输入 | 期望 |
|---|---|
| 零节链 | `Empty` |
| 九节链 | `TooLong { count: 9, limit: 8 }` |
| 第一节 issuer 不是给定的根 | `WrongRoot` |
| 第二节 parent 指向别处 | `Detached { at: 1 }` |
| 第二节 issuer 不是第一节的 subject | `IssuerMismatch { at: 1 }` |
| 手工拼出更宽的第二节 | `Widened { at: 1 }` |
| 任意一节签名被改 | `NotAuthentic { step }` |
| 链中任一节被撤销 | `Revoked { step }` |
| `now >= expires_at` | `Expired` |
| 出示者不是最末 subject | `NotTheHolder` |
| 完整 grant 之后多一字节 | `TrailingBytes` |
| root 节 parent 字段非零 | `UnknownParentTag` |

并发：全部为值类型，无内部可变性。

## 12 错误处理

`GrantError` 十六个变体，全部 `#[non_exhaustive]`，每个带稳定码 `grant.*`，每个指出**在第几跳**或**哪一节**出错。本 crate 不做恢复——它回答的是「这条链是否授权」，如何应对由调用方决定；`kusanagi::complaint` 在其上附加恢复命令。

## 13 依赖选型

| 依赖 | 理由 | 替代方案与代价 |
|---|---|---|
| `blake3` | `StepId` 的域分隔哈希，与全仓同源 | 无 |
| `kernel` 的 `Signer`/`Handle`/`VerifyingKey` | 签名能力已在 kernel 定型，此处不引入第二套身份 | 自带一套密钥类型即第二权威 |
| `proptest`（dev） | 对任意链采样衰减性质 | 见 §16 关于 `kani` 的说明 |

## 14 硬编码声明

| 硬编码 | 意图 | 后续影响 |
|---|---|---|
| `b"kusanagi.grant.v1"` | `StepId` 的域分隔前缀 | 编码若变更须同步升版，否则两种格式的 id 会相撞 |
| `b"kusanagi.grant.v1.sign"` | 签名域，与 id 域分开 | 防止「一个 grant 的标识符」被误当成「签发者同意过的东西」 |
| `MAX_STEPS = 8` | 验证工作量与邀请串长度的上界 | 放宽即放宽邀请串长度，属线路格式变更 |
| `STEP_BYTES = 170` | 定长编码的宽度 | 字段增删须同步 |

## 15 影响面

`kusanagi::channel::Standing`、`kusanagi::invite`、`kusanagi::assembly` 的 send/read/join/revoke 全部依赖 `permits`。`Grant` 的线路格式出现在邀请串与 channel 文件中，改动两者都要升版本字节。

## 16 测试与约束

26 个单元测试（`scope.rs` 6、`step.rs` 2、`chain.rs` 14、`revocation.rs` 3，含 §11 全部行）加 4 个属性测试。

**关于 `kani`**：`ARCHITECTURE.md` 要求用 `kani` 对真实 MIR 证明「衰减不能扩权」。本机未安装 `kani`（`cargo kani` 不存在），因此：

- 证明 harness 已提交在 `src/chain.rs` 的 `#[cfg(kani)]` 模块里，装有 `kani` 的机器执行 `cargo kani --harness attenuation_never_widens` 即可运行；
- 今天真正跑起来的是 `tests/attenuation.rs` 的采样版本；
- `unexpected_cfgs` 已在根 `Cargo.toml` 声明 `cfg(kani)`，所以 harness 参与常规编译检查，不会腐烂。

这是一次**降级而非省略**，记录在此以免被误读为已完成证明。

## 17 文档同步

1. 本文。
2. `ARCHITECTURE.md` §5 行数表、§8「衰减是格上的交」条目。
3. `docs/joining.md` 的错误码表（`grant.*` 各行）。
