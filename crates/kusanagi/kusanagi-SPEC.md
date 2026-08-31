# kusanagi-SPEC

> `kusanagi` —— 九个动词、一份本地状态、一个装配点。这是唯一知道具体东西存在的 crate。
>
> 权威顺序：用户裁决 → `ARCHITECTURE.md` → 本文 → 代码与测试。本文先于代码改动。

## 1 需求拆解

| 单元 | 交付物 | 独立验收 |
|---|---|---|
| U1 库/二进制分离 | `lib.rs` 暴露 `run(&Site, &Request)`；`main.rs` 只做 clap ↔ `Request` 的翻译 | `crates/kusanagi/tests/` 不经命令行驱动全部动词 |
| U2 本地状态 | `Site`：identity / channels / revoked | 身份写一次不被覆盖；能逃出目录的名字被拒 |
| U3 通道记录 | `Channel` / `Standing` / `Peer` | 有无 peer 两种形态各自往返；尾随字节与陌生版本被拒 |
| U4 邀请 | `Invite` 的一行文本形式 | 往返恒等；缺前缀、改套件字节各得错误 |
| U5 读流 | `walk` / `peek` | 密封→解封→解码→查作者→验链，任一步失败即停 |
| U6 装配 | `assembly::run` | 九个动词；时钟每条命令采样一次 |
| U7 输出 | `Outcome` / `Complaint` | 同一个值渲染成散文与 JSON，两者不可能不一致 |

## 2 验收标准

`crates/kusanagi/tests/` 的 13 个验收测试即验收标准本身：

1. 两个端点经一个都不运行的宿主交换消息（`endpoint.rs`）。
2. 宿主翻转一位即被检出，错误码 `seal.rejected`。
3. 撤销 peer 后，其此前与此后写的一切都不再被接受，错误码 `grant.revoked`。
4. 一份邀请只接纳一个端点，第二次得 `kusanagi.invite_spent`。
5. 只有 `read` 的端点不能 `send`，得 `grant.forbidden`。
6. 过期的邀请被拒，得 `grant.expired`。
7. 从零重建的端点继续同一条链而不是分叉——**无常驻状态法则的机械形式**。
8. 一百段之后宿主无法关联任何两条记录（`unlinkable.rs`）。
9. 同一对端点的第二条通道与第一条毫无共同点。
10. 两个端点经真实 TCP 相遇（`across_tcp.rs`）。
11. `doctor` 对运行中的盒子给出四项 held、tier 为 `write-once`。
12. `doctor` 对普通目录如实报告两项 `not offered`，而不是判它失败。
13. 通道列表在有人加入前后各自正确。

## 3 假设与歧义

| 歧义 | 假设 | 何时失效 |
|---|---|---|
| 一条通道几方 | 两方 | cohort 落地后由名册决定；`Channel` 需增加成员列表 |
| 邀请方如何知道受邀方是谁 | 受邀方在**介绍流**的 0 号高度写一段，内容是自己的 grant | 永不失效；这是零往返引荐的最小构造 |
| 通道名 | 本地私有，从不上线 | 永不失效 |
| 身份文件权限 | 不设 Unix 模式位 | 跨平台一致优先；见 §14 |
| 读操作可否写盘 | 可以，且仅限一处：把已验证的 peer 记下来 | 见 §10 步骤 4 |

## 4 现状分析

阶段 0 的欠账「`assembly::run` 接收 clap 的 `Command` 类型，因此无法从测试驱动」在本版偿还：crate 变为 lib + bin，动词集合由 `Request` 这个纯枚举定义，clap 只在 `main.rs` 出现。代价是多一个枚举与一次翻译；收益是九个动词的端到端行为由 `cargo test` 判定，而不是由一个没人跑的 shell 脚本判定。

## 5 权威信源

| 事实 | 来源 |
|---|---|
| 时钟只在 `kusanagi::assembly` 采样 | `AGENTS.md`；由 `clippy.toml` 的 `disallowed-methods` 机械执行 |
| 邀请须一行带齐宿主地址、套件、一次性 Grant，零配置文件 | `ARCHITECTURE.md` §8 |
| 撤销立即且传递 | `ARCHITECTURE.md` §8 |

## 6 命名统一

`Channel`、`Standing`、`Peer` 已进入 `ARCHITECTURE.md` §4 的词表。`Site` 指本端点的磁盘状态；`Request`/`Outcome`/`Complaint` 分别是入、正常出、异常出，一名一义。

## 7 模块边界

```
lib.rs        模块索引
request.rs    Request —— 动词集合的唯一权威
site.rs       Site —— identity / channels / revoked
channel.rs    Channel / Standing / Peer 与其磁盘格式
invite.rs     Invite 与 kusanagi1: 文本形式
walk.rs       peek / walk —— 读一条流并逐段检查
report.rs     Outcome —— 一个值，两种渲染
complaint.rs  Complaint —— 失败 + 稳定码 + 恢复命令
world.rs      时钟与熵的唯一采样点
assembly.rs   九个动词的组装
main.rs       clap ↔ Request
```

依赖全部五个内部 crate，加 `clap`、`getrandom`、`serde`、`serde_json`、`thiserror`。

## 8 接口先行

```rust
pub fn run(site: &Site, request: &Request) -> Result<Outcome, Complaint>;

pub enum Request { Identity, Channels, Invite{..}, Join{..}, Send{..}, Read{..},
                   Revoke{..}, Doctor{..}, Host{..} }

pub enum Standing { Root, Granted(Grant) }
impl Standing {
    pub fn permits(&self, root: &Handle, who: &Handle, ability: Ability,
                   now: Instant, revoked: &Revocations) -> Result<(), GrantError>;
}

pub struct Site { /* 私有 */ }   // identity / adopt / channel / holds / keep / names / revocations / revoke
```

**为什么 `Standing` 是枚举而不是 `Option<Grant>`**：根权威没有 grant，不是因为它的 grant 缺失，而是因为它上面没有人可以签发。写成 `Option` 就要求每个调用点都记得 `None` 当时是什么意思。

## 9 工作流程

```
invite：生成 secret 与一次性种子 → 根对一次性 handle 签发 grant
        → 存下 Channel{ standing: Root, peer: None } → 输出一行邀请
join  ：解析 → 验证邀请的 grant → 一次性钥匙转授给自己 → 在介绍流 0 号写一段（内含自己的 grant）
        → 存下 Channel{ standing: Granted, peer: Some(邀请方 / Root) }
send  ：查 standing 是否允许 Send → walk 自己的流取链头 → 签段 → 封装 → put_if_absent
read  ：查 standing 是否允许 Read → 若 peer 未知则读介绍流并落盘 → 查 peer 是否被允许 Send
        → walk 对方的流
revoke：取 peer grant 的最末一节 id → 写进 revoked
```

## 10 实现逻辑

**步骤 1：动词集合是一个枚举，不是 clap 的形状。** 这样第二个门面（socket、MCP）到来时是加法，而不是把动词再教给第二个解析器。

**步骤 2：邀请携带一次性密钥。** 写邀请的人不可能知道谁会接受，所以 grant 签发给一把随邀请同行的钥匙；接受者立刻把它转授给自己的 handle，那把钥匙此后再不使用。撤销这一节，被切断的正好是用过它的那一个人。

**步骤 3：一次性由宿主保证，不由簿记保证。** 介绍段落在一个一次性写入的地址上，因此第二次接受被**宿主**拒绝。程序里没有任何东西记录一份邀请是否用过。

**步骤 4：读操作允许写一次盘。** `greet` 把已验证的 peer 记进通道文件。它是「三件事同时成立」之后的结论——grant 源自本通道的根、签发给了写下问候的那个 handle、且允许它写——每条命令重算一次只会付一次必然得到相同答案的请求钱。

**步骤 5：两端都检查权限。** `send` 检查自己的 standing，`read` 检查 peer 的 standing 是否允许 Send。第二项才是真正的执行点：撤销之后，对方写的东西在**这一侧**被拒，而不需要对方或宿主的配合。

**步骤 6：`host` 的进度写 stderr。** 一个永不返回的动词不能用「返回值即结果」的形状；stdout 只承载结果，绑定地址写在 stderr。

**步骤 7：通道名当作路径分量来校验，不做转义。** 只放行 `a-z0-9-`、长度 1..=32。转义容易写错的方式全都始于「允许一点有趣的东西」。

## 11 边界枚举

| 情形 | 期望 |
|---|---|
| 尚无身份就执行任意动词 | 首次使用时自动生成；不需要 setup 步骤 |
| 重复 `adopt` | 保留原身份，绝不覆盖（覆盖等于静默放弃全部通道） |
| 通道重名 | `kusanagi.channel_exists` |
| 名字含 `../`、`/`、大写、空格 | `kusanagi.malformed` |
| peer 尚未加入就 `read` 或 `revoke` | `kusanagi.no_peer_yet` |
| peer 就是根权威而试图 `revoke` | `kusanagi.cannot_revoke_root` |
| 对方流上出现别人签名的段 | `kusanagi.not_the_peer` |
| 下一个地址已被占 | `kusanagi.drop_taken`，并给出重读后重发的命令 |
| 通道文件版本不认识 | 拒绝而不是猜 |

## 12 错误处理

`Complaint` 十四个变体，每个带稳定码与**恢复命令**。三条与众不同：

- `seal.rejected` / `chain.*` / `segment.*` / `not_the_peer` 的恢复建议是「留着这些字节并报告」——它们不是瞬时故障，而是损坏或干预。
- `grant.*` 的建议是「去要一份新的邀请」，因为本端无法自行修复权限。
- `waypoint.*` 的建议是 `kusanagi doctor <waypoint>`，把诊断交给会实测的那个动词。

## 13 依赖选型

| 依赖 | 理由 |
|---|---|
| `clap` 4 | 只在 `main.rs`；派生宏换来的帮助文本与错误信息值这一个依赖 |
| `getrandom` 0.3 | 直接问操作系统要熵，中间不放生成器，就没有需要正确播种、重播种、fork 后重置的东西 |
| `serde` + `serde_json` | **只用于 `--json` 输出**。任何被哈希或签名的东西一律手写编码 |

## 14 硬编码声明

| 硬编码 | 意图 | 后续影响 |
|---|---|---|
| 默认 `--root .kusanagi` | 无参数即可用 | —— |
| 默认 `--for 604800`（一周） | 邀请有效期 | —— |
| 通道名 `a-z0-9-`、≤32 | 路径、shell、URL 三处都安全 | 放宽须同时想清三处 |
| 介绍流的高度 `0` | 引荐的约定位置 | 属线路格式 |
| `kusanagi1:` 前缀、版本 1、套件 0 | 邀请串的识别与拒绝未来格式 | 换套件即换版本字节 |
| **不设身份文件的 Unix 模式位** | Windows 上无对应语义，行为跨平台一致优先 | 已知短板：多用户机器上应把 site 放在仅本人可读的目录里，这一点写在 `docs/joining.md` |

## 15 影响面

本 crate 是叶子，没有下游。但它是**唯一**读时钟、读环境变量、发随机数的地方——三者中任何一处出现第二个采样点，都是对 `ARCHITECTURE.md` §7 第 3 条的违反。

## 16 测试与约束

17 个单元测试（site 5、channel 5、invite 5、world 2）加 13 个验收测试。

**关于生产侧的唯一抑制**：`world.rs` 里 `sample()` 带 `#[expect(clippy::disallowed_methods)]`。根 `Cargo.toml` 的生产允许清单为此加了一行并写明理由——`AGENTS.md` 已裁定「唯一采样点是 `kusanagi::assembly`」，`clippy.toml` 让这条裁定由机器执行，而程序仍必须读一次时钟，该抑制标记的正是裁定指定的那个地址。**第二处出现即评审失败。** 这一行需要用户的 `Verdict:` 确认保留。

## 17 文档同步

1. 本文。
2. `README.md` 的动词表与「五分钟」段。
3. `docs/joining.md`——任何动词或错误码的变化。
4. `ARCHITECTURE.md` §5 行数表、§7 法则。
5. 根 `Cargo.toml` 的允许清单，若抑制被移除或增加。
