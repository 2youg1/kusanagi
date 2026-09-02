# site-SPEC

> `kusanagi-site` —— 一个端点自己磁盘上的全部东西：一份身份、每条通道一个文件、一张撤销表，以及打开通道用的那行邀请。
>
> 权威顺序：用户裁决 → `ARCHITECTURE.md` → 本文 → 代码与测试。本文先于代码改动。

## 1 需求拆解

| 单元 | 交付物 | 独立验收 |
|---|---|---|
| U1 身份 | `Site::identity` / `Site::adopt` | 写一次不被覆盖；第二次 adopt 返回第一次的 handle |
| U2 通道记录 | `Channel` / `Standing` / `Peer` 与其磁盘格式 | 有无 peer 两种形态各自往返；尾随字节与陌生版本被拒 |
| U3 名字校验 | `Site::channel_path` 内的 `check_name` | 能逃出目录的名字被拒，而不是被转义 |
| U4 撤销表 | `Site::revocations` / `Site::revoke` | 重复撤销不重复计数；跨进程可见 |
| U5 邀请 | `Invite` 的 `kusanagi1:` 一行文本形式 | 往返恒等；缺前缀、改套件字节各得错误 |
| U6 失败形状 | `SiteError` 三个变体 | 每个变体在门那一层拿到稳定码；门加不出第四个码就编译不过 |

## 2 验收标准

15 个单元测试（site 5、channel 5、invite 5）加门那侧的 `complaint.rs::tests`：
后者断言四种 `SiteError` 到达门口时各自得到哪个稳定码，因为**产生失败的那一层不知道码，
而知道码的那一层不产生失败**——两者相遇只有一处，能检查的也只有那一处。

## 3 假设与歧义

| 歧义 | 假设 | 何时失效 |
|---|---|---|
| 「本机 IO 失败」这个形状归谁 | **归碰了磁盘的那一层**；稳定码与恢复命令归门 | 永不失效；见下 |
| 身份文件权限 | 不设 Unix 模式位 | 跨平台一致优先；短板写在 `docs/joining.md` |
| 通道名字符集 | `a-z0-9-`、1..=32 | 放宽须同时想清路径、shell、URL 三处 |
| 一条通道几方 | 两方 | cohort 落地后由名册决定 |

**第一行是这个 crate 存在的理由，值得写全。** `SiteError::Local { action, source }`
说的是「读身份文件时操作系统拒绝了」，这是碰磁盘的那一层唯一知道的事；
`kusanagi.local` 这个码、以及「检查 `--root` 指向一个可写目录」这句恢复命令，
是**门**才知道的事——因为恢复是用动词说的，而动词只有前端有。
两者合成一个类型，就等于把 `kusanagi channels` 这句话写进一个没有动词的 crate。

## 4 现状分析

本 crate 由 `kusanagi` 拆出。拆分的触发条件是 `ARCHITECTURE.md` §5 的行数预算：
`kusanagi/src` 到过 2,424 / 2,500，而 `kusanagi-SPEC.md` §7 早已记下「下一次实质改动的第一步是拆分」。
拆走的 937 行正好是**读一条流时不需要在场的磁盘格式细节**，拆完主 crate 降到 1,494。

拆分本身没有改变任何行为：三个文件整体搬迁，`Complaint` 换成 `SiteError`，
门上新增一处 `From<SiteError> for Complaint` 的映射。

## 5 权威信源

| 事实 | 来源 |
|---|---|
| 无常驻状态：高度来自 waypoint，不来自本地文件 | `ARCHITECTURE.md` §7 法则 1 |
| 邀请须一行带齐宿主地址、套件、一次性 Grant | `ARCHITECTURE.md` §8 |
| 每个失败都要带稳定码与恢复命令 | `ARCHITECTURE.md` §7 法则 5 |

## 6 命名统一

`Site` 已进入 `ARCHITECTURE.md` §4 词表。`Channel`、`Standing`、`Peer`、`Invite` 沿用原义，
一名一义；`SiteError` 是本 crate 唯一的失败类型。

## 7 模块边界

```
lib.rs      模块索引
site.rs     Site —— identity / channels / revoked 三份磁盘状态
channel.rs  Channel / Standing / Peer 与其磁盘格式
invite.rs   Invite 与 kusanagi1: 文本形式
error.rs    SiteError —— 本机失败的三种形状
```

依赖 `kernel`、`grant`、`seal` 与 `thiserror`。**不依赖 `waypoint`**：本 crate 不做网络，
也不知道 locator 指向什么，它只把那串文本原样存下与取出。

## 8 接口先行

```rust
pub struct Site { /* 私有 */ }
impl Site {
    pub fn at(root: impl Into<PathBuf>) -> Self;
    pub fn root(&self) -> &Path;
    pub fn identity(&self) -> Result<Option<Signer>, SiteError>;
    pub fn adopt(&self, seed: &[u8; 32]) -> Result<Signer, SiteError>;
    pub fn channel(&self, name: &str) -> Result<Channel, SiteError>;
    pub fn holds(&self, name: &str) -> Result<bool, SiteError>;
    pub fn keep(&self, name: &str, channel: &Channel) -> Result<(), SiteError>;
    pub fn names(&self) -> Result<Vec<String>, SiteError>;
    pub fn revocations(&self) -> Result<Revocations, SiteError>;
    pub fn revoke(&self, step: StepId) -> Result<(), SiteError>;
}

pub enum SiteError { Local{..}, BadName{..}, BadInvitation{..}, BadRecord{..},
                     UnknownChannel{..}, Grant(GrantError) }
```

**`SiteError` 特意不是 `#[non_exhaustive]`。** 门逐个变体地给码与恢复命令；
多出一种没人定过价的失败，正应当让构建停下来，直到有人给它定价。

## 9 工作流程

```
identity：读 <root>/identity；不存在则 None，不是错误
adopt   ：已有身份则原样返回，绝不覆盖（覆盖等于静默放弃全部通道）
channel ：校验名字 → 读 <root>/channels/<name> → 解码，版本不认识就拒
keep    ：建目录 → 整文件写入
revoke  ：读全表 → 并入 → 整文件写回
Invite  ：一行文本 ⇄ 定长字节，套件字节不匹配即拒
```

## 10 实现逻辑

**步骤 1：名字当作路径分量来校验，不做转义。** 只放行 `a-z0-9-`、长度 1..=32。
转义写错的方式全都始于「允许一点有趣的东西」。

**步骤 2：身份写入用 `create_new` + `sync_all`。** 覆盖一份身份等于放弃它持有的每条通道。

**步骤 3：邀请携带一次性密钥而不是受邀者的名字。** 写邀请的人不可能知道谁会接受。

**步骤 4：解码一律拒绝尾随字节与陌生版本。** 猜一个不认识的版本，等于让两种格式共用一个地址空间。

## 11 边界枚举

| 情形 | 期望 |
|---|---|
| 尚无身份就读 | `Ok(None)`，不是错误 |
| 重复 `adopt` | 保留原身份 |
| 名字含 `../`、`/`、大写、空格 | `SiteError::BadName` |
| 通道文件版本不认识 | `SiteError::BadRecord`，拒绝而不是猜 |
| 通道记录尾随字节 | 同上 |
| 撤销表某行不是 StepId | `SiteError::BadRecord`（经 `DigestParseError`） |
| 邀请缺 `kusanagi1:` 前缀 | `SiteError::BadInvitation` |
| 邀请套件字节非 0 | `SiteError::BadInvitation`，指出本 build 说哪个版本 |

## 12 错误处理

五个变体加一个透传：`Local`（操作系统拒绝）、`BadName`（你打的名字不能用）、
`BadInvitation`（你贴的那行不是邀请）、`BadRecord`（盘上的字节不是它声称的结构）、
`UnknownChannel`（要的东西不在这里）、`Grant`（记录里的 grant 不解码）。

**「格式不对」拆成三个而不是一个，是因为出路有三条**：名字重打一次，邀请重拷一次，
盘上的记录既不能重打也不能重拷——它要被留证。三者在门那侧共用稳定码 `kusanagi.malformed`。
恢复命令一概不在这里；见 §3。

## 13 依赖选型

| 依赖 | 理由 |
|---|---|
| `kernel` | Handle / Signer / Reader / Hex，全仓一套编解码 |
| `grant` | Standing 里放的就是 Grant；撤销表放的是 StepId |
| `seal` | Channel 持有 Secret |
| `thiserror` | 与其他 crate 同一套错误派生 |

`serde` **不在此列**：被签名或被哈希的东西一律手写编码，磁盘格式也是手写的。

## 14 硬编码声明

| 硬编码 | 意图 | 后续影响 |
|---|---|---|
| 通道名 `a-z0-9-`、≤32 | 路径、shell、URL 三处都安全 | 放宽须同时想清三处 |
| 通道记录版本 1 | 拒绝未来格式而不是猜 | 换格式即换版本字节 |
| `kusanagi1:` 前缀、版本 1、套件 0 | 邀请串的识别 | 换套件即换版本字节 |
| 不设身份文件的 Unix 模式位 | 跨平台一致优先 | 多用户机器上须把 site 放进仅本人可读的目录 |

## 15 影响面

上游只有 `kusanagi` 一个。磁盘格式的任何改动都是**别人机器上已存在的文件**的改动，
所以版本字节先动，解码器拒绝的分支先写。

## 16 测试与约束

15 个单元测试就在三个文件里，跨过边界的那一条断言在 `kusanagi/src/complaint.rs` 的测试模块。
本 crate 没有集成测试目录：它的端到端行为是 `kusanagi/tests/` 的九个动词。

## 17 文档同步

1. 本文。
2. `ARCHITECTURE.md` §4 词表（`Site`）、§5 crate 图与行数表。
3. `crates/kusanagi/kusanagi-SPEC.md` §7 模块边界。
