# HANDOFF — 从阶段 0 推进到 v0.0.1 pre-alpha

> **读者是下一个 Agent。** 本文是恢复入口，不是权威：它只给基线、边界、执行图、以及已经付过的账。所有事实的权威在 `ARCHITECTURE.md` 与各 `crates/<crate>/<crate>-SPEC.md`，按定位符去读，不要在这里找细节。
>
> 交接人：上一会话（Claude）。基线提交 `card-S0.01`。

---

## 0 先说一件不受欢迎的事

用户的原话是「把事情一次性全部做完，直接推进到完整 V0.0.1 pre-alpha 发布」。

`ARCHITECTURE.md` §8 有八个阶段。**一个会话做不完八个阶段，试图做完只会产出桩代码**——而项目宪法禁止用计划、桩或看似合理的输出替代被要求的结果。所以我在 §2 给 v0.0.1 划了边界：**阶段 1、2、3 加上接入与发布**。理由逐条写在那里。

如果用户推翻这个边界，以用户为准；但请在动手前把推翻记下来，不要默默扩大范围。**把边界之外的东西做成桩，比不做更糟**：桩会让人以为那部分能用。

---

## 1 你接手的是什么

阶段 0 已收口。以下是实测数字，不是声称：

| 事实 | 值 | 怎么复核 |
|---|---|---|
| 测试 | 41 通过 | `cargo test --all-features` |
| clippy | 零输出 | `cargo clippy --all-targets --all-features -- -D warnings` |
| 行数 | 2,334 / 25,000 | `just budget` |
| crate | `kernel` 996、`chain` 438、`waypoint` 497、`kusanagi` 403 | 同上 |
| 端到端 | 四个独立进程写读一条可验证的链，篡改一字节即被检出 | `just demo` |
| 工具链 | rustc 1.97.1，`just` 可用，**GHC 未安装** | `rustc -V; just --version; ghc -V` |

**能跑起来的**：`kusanagi send --as alice "文本"` / `kusanagi read --from alice`，落在一个本地目录上。地址由 `(author, index)` **公开派生**，故意可链接——那是阶段 0 的脚手架，阶段 1 删掉它。

## 2 v0.0.1 pre-alpha 的边界

**判据**：v0.0.1 值得发布，当且仅当这句话为真——**两台机器上的两个身份，经由一台不被信任的宿主，交换权限受限且互不可链接的消息。** 少任何一半，发布的就不是这个项目的论点。

### 进入 v0.0.1

| # | 阶段 | 为什么它不能等 |
|---|---|---|
| 1 | `seal` + 不可链接派生 | 没有它，kusanagi 只是一个带哈希链的文件格式。`ARCHITECTURE.md` §2 第 2 项的全部主张在此 |
| 2 | `grant` | 没有它就没有多方，也没有「谁可以写给谁」的答案；需求 6 的隔离同时由它兑现 |
| 3 | 远程 Waypoint + `doctor` | 没有它，「跨机器」「外包工作空间」「宿主不被信任」三句话都未经证明 |
| — | `join` 一行接入 | 需求 4。它是前三项的自然出口，不是额外工作 |
| — | README + 发布物 | 没有发布物就没有发布 |

### 明确不进入 v0.0.1，以及理由

| 不做 | 理由 |
|---|---|
| `veil`（混淆） | 没有真实审查设备可打，做出来的东西**不可证伪**。等有人真的被 DPI 挡住再做 |
| `cohort`（成员与 epoch） | 需要多节点测试设施；v0.0.1 的两方场景用不到名册 |
| `depot`（工作空间分块） | 独立问题，且 64 KiB 以下的 payload 已够走通全链路 |
| `port` / MCP 门面 | 需要先还 §7 的第一笔欠账；它是剩下最大的一块，单独一版 |
| Bell | 优化。**在没有流量可测之前做优化，等于凭想象调参** |
| 混合 PQ | 加法项。经典套件先对，再加 PQ 是一次干净的追加 |
| `adversary/`（Haskell） | GHC 未装，且按 `ARCHITECTURE.md` §5.3 第三条，它**不得**阻塞 Rust 闸门 |

---

## 3 执行图

一次一个单元，顺序即依赖。**每个单元的收口条件是可执行的命令，不是「完成了」这三个字。**

| 序 | 单元 | 依赖 | 收口条件（可执行） | 状态 |
|---:|---|---|---|---|
| 1 | 还欠账：`assembly::run` 脱离 clap 类型 | — | `crates/kusanagi/tests/` 里有集成测试，能不经命令行驱动 send/read | 待办 |
| 2 | `seal`：`derive(secret, index) -> (DropAddr, Key)` + AEAD | 1 | 一百段往返；**宿主日志里任意两条记录不可关联**——写成断言 | 待办 |
| 3 | 删除 `kernel::address::public_v0` 及其全部调用方 | 2 | 全仓 grep 不到 `public_v0`；`v0` 从来就是标记「会被删」而非「会被升级」 | 待办 |
| 4 | `grant`：签发 / 衰减 / 验证 / 撤销 | 2 | 三级衰减链，撤第二级则第三级立即失效；`kani` 证明衰减不能扩权 | 待办 |
| 5 | HTTP 盒子 Waypoint + 那台 ~200 行的服务器 | 2 | **两个进程经 TCP** 收发；CI 里可跑，不需要外网 | 待办 |
| 6 | S3 兼容 Waypoint | 5 | 有凭据时对真实 R2 跑通；无凭据时该测试跳过而**不是**假通过 | 待办 |
| 7 | `kusanagi doctor <waypoint>` | 5, 6 | 实测四件事（条件写是否真拒覆写、条件读是否 304、ETag 是否稳定、TTL 是否生效），签发能力证书或**具名降级** | 待办 |
| 8 | `kusanagi join <invite>` | 4, 5 | 一行邀请串带齐宿主地址、suite id、一次性 Grant；零配置文件 | 待办 |
| 9 | README + 一页接入文档 | 8 | **找一个没读过本仓代码的读者照着做一遍**；做不通就是文档的错 | 待办 |
| 10 | 发布：`just dist`、校验和、tag `v0.0.1` | 全部 | 见 §8 | 待办 |

单元 1 排第一不是因为它最重要，而是因为**没有集成测试，后面九个单元都在裸奔**。

---

## 4 已经付过的账：不要重做这些研究

| 事实 | 用在哪 | 来源 |
|---|---|---|
| `blake3::derive_key(context, key_material)` 是 BLAKE3 自带的 KDF 模式 | 单元 2 的派生。**不要引入 `hkdf` crate**——全仓一个哈希原语，少一个依赖 | blake3 文档 |
| `clatter` 2.3.0（no_std Noise，带 PQ 扩展）、`snow` 0.10.0 | 阶段 4 的握手，v0.0.1 用不到 | crates.io 实测 |
| `x-wing` 0.1.0（X25519 + ML-KEM-768，对齐 draft-06） | 混合 PQ，v0.0.1 之后 | crates.io 实测 |
| `chacha20poly1305` 0.11.0、`redb` 4.2.0、`fastbloom` 0.17.0 | 依次是 AEAD、本机存储、Bell | crates.io 实测 |
| S3 `If-None-Match: *` 自 2024-08 起支持 | 单元 6 的一次性写入 | AWS S3 用户指南 *Conditional writes* |
| **Cloudflare R2 支持**，条件失败返 `412 PreconditionFailed` | 单元 6 的首选宿主 | R2 S3 扩展文档 |
| **MinIO 语义分歧**：只认精确 ETag，不认通配符 `*`（minio#20346）；对不存在的 key 用 `If-Match` 会忽略条件（minio#21526） | 单元 7 存在的全部理由。**两处分歧都朝失败开放——条件被静默忽略，写入照常成功** | 两个 issue |
| R2 免费额度：每月 1000 万次 Class B 读、100 万次 Class A 写，egress 免费 | 60 秒轮询下覆盖 231 个 agent，成本为零 | R2 定价页 |
| crates.io 的 `kusanagi` **可用**；GitHub 组织 `kusanagi` 已被占；npm 已被占 | 单元 10。仓库用 `2youg1/kusanagi` | 带 UA 的 API 实测 |

**crates.io 那条要特别说**：上一会话第一次用裸 curl 查得到 403，误当成「不确定」。403 是它拦 UA。**查名字必须带 User-Agent**。

## 5 已经踩过的坑

| 坑 | 表现 | 怎么绕 |
|---|---|---|
| `clippy::allow_attributes` 与测试模块 | 测试模块的 `#[allow(..., reason="test code")]` 被判红 | 已在 `Cargo.toml` 关掉并写明理由。`allow_attributes_without_reason` 仍是 deny，**别动它** |
| `create_new` 不足以保证一次性写入 | 写入中途崩溃留下半个段，而那个地址**按定义永远不能重写** | 暂存 + `fsync` + `hard_link`。见 `crates/waypoint/src/dir.rs` |
| `arithmetic_side_effects` 连位移一起禁 | 十六进制编解码写不出 `>> 4` | 渲染用 `format!("{b:02x}")`，解码用 `chunks_exact(2)` + `checked_*`。见 `kernel/src/digest.rs` |
| pedantic 的四个常客 | `needless_pass_by_value`、`format_push_string`、`format_collect`、`elidable_lifetime_names` | 传引用；用 `Vec<String>` + `join` 而不是 `push_str(&format!(…))` |
| 生产代码的 `#[expect]` 允许清单**是空的** | 任何生产侧抑制都过不了评审 | 改代码，不要改清单。加一行清单需要用户的 `Verdict:` |

## 6 用户已作的裁决——不要重新讨论

原话照录，避免转述走样：

- 命名：「我想把这个项目命名未 kusanagi」→ 项目名 `kusanagi`，二进制同名，不设简称。
- 重构授权：「只要你看到更优以及更少行数的优雅代码实现，就可以执行，我允许！」→ **这是常驻授权**。看见更短更优的写法直接做，但仍须先改 SPEC 再改代码。
- 设计定案：「我感觉没什么问题，现在开始实现吧」→ `ARCHITECTURE.md` 全文（含 §7 的 D1/D2/D3 三条裁决）已被接受。
- Haskell：「Haskell 是探索性的，可以在一些特别适合的模块使用」→ 位置已定死在 `ARCHITECTURE.md` §5，只能是仓外的 `adversary/`，且不得阻塞 Rust 闸门。

`ARCHITECTURE.md` §7 的三条已定，**不要再当作开放问题**：Bell 是 Waypoint 的能力而非协议必需；规模是分层的（1000 cohort × 1000）而非扁平；baseline suite 强制，插件只做增量。

## 7 已知欠账——必须还，不许假装不存在

1. **`assembly::run` 接收 clap 的 `Command` 类型**，因此无法从测试驱动；阶段 0 只有 `just demo` 做端到端。已记在 `crates/kusanagi/kusanagi-SPEC.md` §16。**这是执行图的单元 1。**
2. **`ARCHITECTURE.md` §4.2 的行数表仍是预估**，没填实测值。填一次很便宜。
3. **没有 README**，没有 CI 配置。
4. **`waypoint-SPEC.md` §3 关于 MinIO 的假设行**，要在单元 7 之后由 `doctor` 的实测结果替换。

## 8 发布机制（单元 10）

`v0.0.1` 是 pre-alpha，标签要诚实。发布物与判据：

- `just check` 全绿，且 `just budget` 未超限。
- 三个平台的二进制（Windows / macOS / Linux）加 SHA-256 校验和。
- README 第一屏必须写清**它还不能做什么**——照抄 sprawling 的 "What works / what doesn't" 那张表的诚实度。
- `docs/third-party.md`：依赖清单与许可，`cargo deny` 通过（**`deny.toml` 尚未创建**）。
- 每个 `.rs` 首行的 MPL-2.0 声明齐备。
- Git tag `v0.0.1`，release note 引用本文 §2 的边界表，让读者知道**没做什么以及为什么**。

## 9 什么时候停下来问人

- 要往 `Cargo.toml` 的生产抑制允许清单加任何一行。
- 要改 §2 的 v0.0.1 边界。
- 单元 2 的收口条件（不可关联性）**做不到**——那不是一个可以绕过的技术困难，那意味着隐私主张是假的，应当停下重划而不是继续往上垒。
- 要引入 `ARCHITECTURE.md` §4.4「特意没有采用」表里的任何东西。

## 10 下一个动作

**单元 1：把 `assembly::run` 从 clap 的 `Command` 上摘下来，给 `crates/kusanagi/tests/` 加第一个集成测试。**

先读 `crates/kusanagi/kusanagi-SPEC.md` §16 与 §15（那里已经写明这次改动从哪里开始），按规程改 SPEC 再改代码。收口条件：一个不经命令行的测试能完成三次 send 加一次 read，并断言篡改被检出。

---

*本文档采用 MPL-2.0。交接完成后，若单元 1 已开工，请在此处更新「下一个动作」，不要新开一份 Handoff。*
