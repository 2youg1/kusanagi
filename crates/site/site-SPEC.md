# site-SPEC

> `kusanagi-site` —— 一个端点自己磁盘上的全部东西：一份身份、每条通道一个文件、一张撤销表，以及打开通道用的那行邀请。
>
> 权威顺序：用户裁决 → `ARCHITECTURE.md` → 本文 → 代码与测试。本文先于代码改动。

## 1 需求拆解

| 单元 | 交付物 | 独立验收 |
|---|---|---|
| U1 身份 | `Site::identity` / `Site::adopt` | 写一次不被覆盖；第二次 adopt 返回第一次的 handle |
| U2 通道记录 | `Channel` / `Standing` / `Peer` 与其磁盘格式 | 有无 peer 两种形态各自往返；尾随字节与陌生版本被拒 |
| U8 对端的钥匙 | `Peer::key` / `Channel::introduction` 存 `VerifyingKey` | 一条流只能用记录里的公钥读；记录版本为 3，旧版本被拒而不是重新解释 |
| U3 名字校验 | `naming::check` | 能逃出目录的名字被拒，而不是被转义 |
| U9 文件名不是对端名 | `naming::filed`：以身份种子派生的密钥对名字做带密钥哈希 | 目录里每一项都是 64 位十六进制，既不含对端名也不能被别的站点复现；名字只在记录内部，且与文件名对不上即拒 |
| U4 撤销表 | `Site::revocations` / `Site::revoke` | 重复撤销不重复计数；跨进程可见 |
| U5 邀请 | `Invite` 的 `kusanagi2:` 一行文本形式 | 往返恒等；缺前缀、改套件字节各得错误 |
| U6 失败形状 | `SiteError` 三个变体 | 每个变体在门那一层拿到稳定码；门加不出第四个码就编译不过 |
| U7 只有主人能读 | `kusanagi-vault`：本 crate 写盘的唯一入口，平台差异是文件不是分支 | 站点里没有任何文件或目录对其他账号可读——Unix 看模式位，Windows 看受保护 DACL；被替换的记录是新 inode / 新句柄，生下来就是关的 |

## 2 验收标准

15 个单元测试（site 5、channel 5、invite 5）加门那侧的 `complaint.rs::tests`：
后者断言四种 `SiteError` 到达门口时各自得到哪个稳定码，因为**产生失败的那一层不知道码，
而知道码的那一层不产生失败**——两者相遇只有一处，能检查的也只有那一处。

## 3 假设与歧义

| 歧义 | 假设 | 何时失效 |
|---|---|---|
| 「本机 IO 失败」这个形状归谁 | **归碰了磁盘的那一层**；稳定码与恢复命令归门 | 永不失效；见下 |
| 文件 `0600`、目录 `0700`，**只在创建时确立，此后永不调整** | 一份站点里是身份种子与全部通道秘密，而 `fs::write` 默认留下的是 `0644`；要防的不是国家级对手，是共用构建机上的第二个账号、sidecar 容器、推到镜像仓库的一层。**后半句是安全属性本身**：`set_permissions` 作用于路径并跟随符号链接，一个会去 chmod 非自己创建的文件的构建，等于给能往站点目录里写东西的人一个把 chmod 指向别处的原语 |
| 替换旧记录用「旁边暂存 + 重命名盖过去」 | 不是为了原子性而已。`rename` 作用于**名字**，因此被替换的是链接本身而不是它指向的东西；新文件是新 inode，生下来就是 `0600`，因此不存在任何一处去 chmod 一个本构建未曾创建的路径。与 `waypoint::dir` 让一个 drop 整个出现用的是同一个形状 |
| 本构建未曾创建的**目录**保持原模式 | 里面每个文件仍是 `0600`，因此暴露的是通道名的集合而不是内容。关上它需要 chmod 一个本构建未曾创建的目录，那正是上一行禁止的操作 |
| Windows 走 `CreateDirectoryW` / `CreateFileW` + SDDL `D:P(A;OICI;FA;;;OW)(A;OICI;FA;;;SY)` | **缺口已关。** `D:P` 拒绝继承——落在别人打开过的目录下的站点不会跟着被打开；`OW` 是 OWNER RIGHTS，即创建者本人，因此代码不必去问「现在是谁在跑」；`SY` 是 SYSTEM，缺它则备份、索引与更新以没人能联系到本程序的方式失败。**不列 Administrators**：他们能取得任何对象的所有权，列了也防不住谁 |
| 不用 `SetNamedSecurityInfoW`，也不用 `OpenOptionsExt::security_attributes` | 前者按路径解析，站点将去的位置上被预埋一个 junction 就会把这次修改指向别人的目录；后者在 std 里仍未稳定。所以直接 `CreateFileW` 拿句柄再 `File::from_raw_handle` | 
| **`unsafe` 有且只有一个地址** | `vault::windows`，理由与代价见 `vault-SPEC.md` §5 |
| 通道名字符集 | `a-z0-9-`、1..=32、**首字符不是 `-`** | 放宽须同时想清路径、shell、URL 三处；首字符那条另有理由，见 §10 步骤 1 |
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

**一份记录里哪些字段是名字、哪些是钥匙，只有一条判据：本端点是否要拿它去验一个签名。** `root` 是名字，因为验 grant 的公钥就在 grant 自己里；`peer.key` 与 `introduction` 是钥匙，因为一个 segment 只报作者的名字、不带验它的公钥（`kernel-SPEC.md` §10 步骤 6）。`Peer::handle()` 是从钥匙算出来的，不单存一份——否则一条记录里就有两个可以不一致的真相。

## 7 模块边界

```
lib.rs      模块索引
site.rs     Site —— 站点的入口：identity、channels，以及向下面四个模块的分发
cairns.rs   <root>/cairns/<filed>/<filed_author> —— 写会报错、读永不报错的那一对
roster.rs   <root>/groups/<filed> —— 一个群组发给哪几条通道
archive.rs  export / import —— 整个站点封进一串字节，再放回来
naming.rs   名字能长什么样，以及它的文件叫什么（两条规则，一处）
revoked.rs  <root>/revoked —— 撤销表，活得比通道记录长
egress.rs   <root>/egress —— 一行 `proxy-required`：本站点是否允许无代理出网（K12）；缺席即允许，写成别的字即 `site.bad_record`
channel.rs  Channel / Peer 与其磁盘格式（版本 5：peer 旁加 ward）
standing.rs   Standing —— 谁凭什么在这条通道上（从 channel.rs 分出，400 行门）
identity.rs   Identity —— 种子 + ward（版本 1；32 裸字节的旧身份按名拒绝）
blocks.rs   长度前缀块：本盘上每一种记录共用的那一层框
cadence.rs  Cadence —— 这一端多久写一次（I3）
retention.rs Retention —— 对端读过之后那个 drop 还在不在（C4）
rhythm.rs   Site 上属于 outbox / slots / ratchets 的那几个方法
outbox.rs   <root>/outbox/<filed>/<票号> —— 说了但还没轮到时隙的正文
slots.rs    <root>/slots/<filed> —— 上一个已填的时隙号
ratchets.rs <root>/ratchets/<filed>/<filed_author> —— 这条道的钥匙烧到哪了

`<filed_author>` = `naming::filed_author(seed, filed, author)`：同一把归档密钥对 `filed ‖ author` 做带密钥哈希。
**不再是明文 handle**（adversary `surface-SPEC` S3–S5 查出）：同一对端在两条通道上、或在两个被扣的站点上，
不再留下同一个文件名；一份目录清单交出的仍是计数，不是关系图。cairn 内部仍带 author，所以归档与导入重算文件名。
invite.rs   Invite 与 kusanagi2: 文本形式
error.rs    SiteError —— 本机失败的几种形状
```

依赖 `kernel`、`grant`、`seal` 与 `thiserror`。**不依赖 `waypoint`**：本 crate 不做网络，
也不知道 locator 指向什么，它只把那串文本原样存下与取出。

**`permissions/` 与 `at_rest` 已拆为 `kusanagi-vault`（714 行）。** 触发它的是 W1 盲读：
身份记录与通道记录都要加 ward，而本 crate 已到 3 645 / 4 000。拆走的那一块不是「端点在自己
盘上存了什么」，而是「怎么请操作系统把一个文件锁在一个账户上」；全仓唯一的 `unsafe`、唯一的
`windows-sys` 依赖与唯一的平台矩阵随它一同过去，允许清单第三行因此从「一个模块」变成
「一个 crate」。形状与理由在 `crates/vault/vault-SPEC.md`；本 crate 只留下一处逆向映射：
`error.rs` 的 `From<VaultError> for SiteError`，逐臂，因为两边都不是 `#[non_exhaustive]`。

**为什么 `rhythm.rs` 单独一个文件**：它装的三份记录共有一条别处没有的性质——**丢掉任何一份，
都没有任何宿主或对端能还回来**。cairn 重走一遍流就能重建；一条排队中的正文只对调用方承诺过、
别处不存在；而棘轮状态**按定义**不可重算，能重算就等于它什么都没做。这条性质决定了它们的失败
策略，也决定了 `export` 必须带上它们。

**为什么把 cairn 从 `site.rs` 搬出去**：不是为了行数。cairn 是这块盘上**唯一可重算**的东西，
因而它的失败策略与其他每一份记录相反（读不到就当没有，写不了就报错）。一个自己一套失败规则的
东西值得一个自己的文件；理由原文在本文末尾那一节，不在这里重写。

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
channel ：校验名字 → 派生文件名 → 读 <root>/channels/<filed> → 解码 → 记录自称的名字须与查它用的名字一致
keep    ：从记录自带的名字派生文件名 → 建目录 → 整文件写入
names   ：列目录 → 逐个读出记录里的名字（文件名已不是名字）
revoke  ：读全表 → 并入 → 整文件写回
Invite  ：一行文本 ⇄ 定长字节，套件字节不匹配即拒
```

## 10 实现逻辑

**步骤 1：名字当作路径分量来校验，不做转义。** 只放行 `a-z0-9-`、长度 1..=32，且首字符不是 `-`。
首字符那条是命令行的账不是文件系统的账：任何命令行都把开头的连字符读成旗标，而这个程序把
单独一个 `-` 读成「名字从 stdin 来」（`kusanagi-SPEC.md` §10 步骤 14）。**一个打不出来的名字
不如不许起。**
转义写错的方式全都始于「允许一点有趣的东西」。

**步骤 1b：文件名是名字的带密钥哈希，名字搬进记录里。** 从前 `<root>/channels/bob` 列一次目录就交出关系图——
后者不是计数，是这张网络所有派生地址存在的理由。密钥取自身份种子（不付 ML-DSA 展开代价），
**同一名字在两端归档成两个字符串**，谁也做不出一张到处能查的表（不加密钥的哈希，常见人名枚举即破）。
代价两处：`names()` 从列目录变成逐个读记录；**记录版本 2→3**，旧记录不再接受（pre-alpha 允许的破坏性变更）。
挡的是列目录、备份清单、同步索引、崩溃报告——**文件名泄漏面最广**；能读文件内容的人照样见名字（D-04：防第二账户与顺手翻看，不防取证）。

**步骤 2：身份写入用 `create_new` + `sync_all`。** 覆盖一份身份等于放弃它持有的每条通道。

**步骤 3：邀请携带一次性密钥而不是受邀者的名字。** 写邀请的人不可能知道谁会接受。

**步骤 4：解码一律拒绝尾随字节与陌生版本。** 猜一个不认识的版本，等于让两种格式共用一个地址空间。

## 11 边界枚举

| 情形 | 期望 |
|---|---|
| 尚无身份就读 | `Ok(None)`，不是错误 |
| 重复 `adopt` | 保留原身份 |
| 名字含 `../`、`/`、大写、空格 | `SiteError::BadName` |
| 名字是 `-` 或以 `-` 开头 | `SiteError::BadName`。见 §10 步骤 1 |
| 尚无身份就 `keep` | `SiteError::NoIdentity`。文件名派生自身份种子，没有种子就没有可归档之处 |
| 尚无身份就 `channel` / `holds` / `names` | 分别是 `UnknownChannel` / `false` / 空表。**没有身份的站点可证明没有通道** |
| 记录归档在 `filed(A)` 却自称叫 `B` | `SiteError::BadRecord`。两者互相派生，不一致即文件被搬动或被别的东西写过 |
| `channels/` 里有一个本 build 读不懂的记录 | `names()` 整体报错，而不是跳过它。**悄悄不再列出的通道，在主人眼里就是丢了的通道** |
| 通道文件版本不认识 | `SiteError::BadRecord`，拒绝而不是猜 |
| 通道记录尾随字节 | 同上 |
| 撤销表某行不是 StepId | `SiteError::BadRecord`（经 `DigestParseError`） |
| 邀请缺 `kusanagi2:` 前缀 | `SiteError::BadInvitation` |
| 邀请套件字节非 1 | `SiteError::BadInvitation`，指出本 build 说哪个版本。**套件 0 是 Ed25519 时代那一个，按号拒绝**：让它走到解析 2 592 字节公钥那一步，得到的报告会是「邀请损坏」而不是「套件不认识」 |

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
| `zeroize` | 身份种子进归档的路上要能被擦掉；仓库已有 |
| `kusanagi-vault` | 本 crate 写盘与读盘的唯一入口；平台矩阵与全仓唯一的 `unsafe` 住在那里 |

`serde` **不在此列**：被签名或被哈希的东西一律手写编码，磁盘格式也是手写的。

## 14 硬编码声明

| 硬编码 | 意图 | 后续影响 |
|---|---|---|
| 通道名 `a-z0-9-`、≤32、首字符非 `-` | 路径、shell、URL 三处都安全，且名字打得出来 | 放宽须同时想清三处，并回看 `intake` 的 `-` 哨兵 |
| 通道记录版本 1 | 拒绝未来格式而不是猜 | 换格式即换版本字节 |
| `kusanagi2:` 前缀、版本 2、套件 1 | 邀请串的识别与拒绝不认识的格式 | 换密码学即换套件字节；套件 0 已作废，版本 1 **按名字拒绝**——它不是坏掉的粘贴，是另一种格式 |
| 邀请只装 secret 32 + seed 32 + locator | 版本 1 把邀请者的 2 592 字节公钥与 grant 链塞进那行字，于是一条普通邀请是 20 028 个字符：念不出来、打不出来，在 Windows 上只能过剪贴板。**这些体积没有一个是秘密的**——公钥与 grant 按构造就是公开的——所以是秘密被身旁的公开数据绑架了。现在约 180 字符 | 换 locator 长度就换总长；grant 深度不再影响它 |
| 校验码 = `BLAKE3(secret)` 前两字节的十六进制 | 两端从同一批字节算出同一个四位数，**在传输中被改过的那一行在另一端算出不同的四位**。四位念得出口，又让在途改写的人只有 1/65 536 的运气——而且只有一次机会，因为错的那个会被念出来 | 加长要同时改 `invite.rs::check` 与两处散文 |
| 归档密钥的语境串 `kusanagi 2026 channel file name v1` | BLAKE3 `derive_key` 的惯例：一个语境串全局只指一个用途，同一颗种子派生出的两把密钥因此永不相撞 | 改这个字符串会让已有站点的每一条通道都「找不到」；要改就得连迁移一起做 |
| Windows 上不加 `\?\` 长路径前缀 | 直接用 Win32 入口点，超过 260 字符的路径由操作系统拒绝，带着真实错误码浮上来成为 `site.local`，**不静默截断** | 出路是更短的 `--root` |
| 已存在的父目录保留它自己的权限 | 与 Unix 的「非本构建创建的目录保持原模式」同一条规矩；要事后收紧就得走按路径解析的 API，那正是本模块存在的理由 | 里面的文件仍然逐个是关的 |

### 归档格式（S1）

```
"KSNB" | version u8 | nonce[12] | sealed(kind u8 | len u32 | bytes)*
```

四种 kind：身份种子、通道记录、cairn（前缀一个 `u16` 名字长度）、被撤销的 step。

- **归档里放的是明文形态的记录**，即本 build 写到盘上的形状，在任何平台存储介入之前。
  于是 Windows 上做的归档能在 Linux 上打开——**它就是跨平台迁移路径**，所以这里没有任何一行
  知道平台是什么。
- **nonce 随密文走**，因为归档没有地址可派生：它是本仓库唯一一个被密封、却不是 drop 的东西。
  密钥是 `blake3::derive_key("kusanagi 2026-01-01 backup archive", recovery)`。
- **恢复密钥是生成的，不是选的。** 人想出来的口令是人猜得到的口令，而一个文件上没有速率限制。
  三十二字节由操作系统给出，十六进制**只印一次**。
- **`import` 拒绝落在已有身份的 root 上**。合并两个站点会让一个端点拥有两份一切，而没有规则说
  哪一份是对的。
- 密封用 `Fit::Exact`（不填充到 DROP）：归档不去任何宿主那里，把它填充成 drop 的整数倍，
  只会让「三条通道的站点」与「六条通道的站点」在**已经同时拥有两者的那个人**眼里变得一样。

### 邀请劈开与 offer drop（C2）

邀请里剩下 64 字节秘密加一个 locator；**其余的搬进宿主上的一个 drop**，地址与密钥由
`kusanagi_seal::offer(secret)` 从通道秘密单独派生——其他每一个地址都要经过某一方的 handle，
而这里两端还谁都不认识谁，那正是这个 drop 存在的理由。

```
version 1 byte = 1 | inviter 2592 bytes | grant 其余
```

- **写宿主在写记录之前。** 两种失败都因此是无害的那一种：宿主不收，本机什么都不用清理；
  磁盘不收，宿主上多一个没人握有密钥的 drop，`--for` 到期即被清扫。
- 用 `put_with_ttl(lifetime)`，所以过期的是宿主替我们做的——这就是旧 C3 剩下的那半个残余。
  桶不支持逐对象寿命时返回 `TtlOutcome::NotOffered`，offer 照样写进去，`doctor` 会在有人
  信任那台宿主之前把这件事报出来。
- `join` 的顺序变了：解析 → 读 offer drop → `envelope::open` → `Offer::from_bytes` →
  校验 grant（根必须是 offer 里那把公钥的 handle，否则 `Grant::verify` 给 `grant.wrong_root`）
  → 其余不变。drop 不在 → `kusanagi.no_invitation`。
- **宿主多看到一个 drop。** 它与其他每一个 drop 同尺寸、同地址形状；`unlinkable.rs` 的计数从
  `2n+1` 变成 `2n+2`，而这正是它唯一变的东西。

### 静态加密（I6）

站点里每个文件的**第一个字节是标签**：`0x00` 明文、`0x01` DPAPI、`0x02` 起留给下一个平台的存储。

- 读到本平台打不开的标签 → `SiteError::ForeignRecord`，码 `site.foreign_record`，恢复是
  「在做出它的平台上 `export`，把归档 pipe 进这里的 `import`」——`archive.rs` 写的是明文形态的
  记录，正是为了让这条恢复路径不需要为每一对平台各写一份迁移代码。
- **封与开只有一处**：`vault::write` / `write_new` 出去时封，`vault::read`
  进来时开。site 里所有 `fs::read` 都改成走它，于是「哪些文件要加密」不再是一个需要记住的清单。
- Windows 用 `CryptProtectData` / `CryptUnprotectData`，`CRYPTPROTECT_UI_FORBIDDEN`（一次性
  动词绝不能停下来等对话框），附加熵常量 `b"kusanagi/site/1"`——它不是密钥也不假装是，买到的是
  「从站点里抠出来的 blob 不会在别人的 `CryptUnprotectData` 调用下打开」。
- **失败是拒绝，不是回退。** 加密失败就明文写下去，等于悄悄撤回这个属性本身。
- 边界照旧写清楚：DPAPI 挡不住这个账户自己、挡不住一台开着且已解锁的机器、挡不住取证级手段。
  全盘加密仍是前提（§8 的裁决不变）。

### 通道记录版本 5 与 offer v3（W1 前半）

`Peer` 加 `ward: Ward`（非 `Option`：知道的对端就是能写的对端，不知道 ward 的对端没有地方可写）。
`Offer` 升到版本 3，多两字节 `inviter_ward`——受邀者从 offer 得到邀请者的 ward，
邀请者从问候段得到受邀者的 ward。问候段格式为 `verifying key ‖ ward ‖ grant`。

此前无人加入的通道上 `send` 即是 `NoPeerYet`：写者不能把段落放进一个没有读者的 bin；
`send` 在写之前做与 `read` 同样的懒问候（同一请求），所以通常流程（邀请→加入→互发）不受影响。
归档版本升到 2，`Kind::Identity` 条目为种子 32 字节 + ward 2 字节。

### 通道记录版本 7、offer v4 与 `alias` 记录（L1 · D-10）

`Peer` 加 `alias: Option<Alias>`：对端在介绍时签名声明的名字，到货时已按其 key 验过，记录里只存词。
**定宽 33 字节**（长度 + 32 字节补零，`peer.rs::ALIAS_BLOCK`），无对端时写全零且**不读**——`robust.rs`
守着「记录大小不随是否有人加入而变」，变长块会把这个性质连同 alias 长度一起交出去。
`Offer` 升到版本 4：`retention` 之后多一个 `u16` 长度块，装邀请者的 `kernel::Declaration`（0 = 没有）。
本端自己的名字是站点记录 `alias`（`alias.rs`，与 `egress`/`sweep` 同一形状），不进身份记录：种子是「我是谁」，
永不变；名字是「我想被怎么叫」，可变。签名形式在 `invite`/`join` 时由身份密钥现算，盘上不存签名。
归档升到 **3**，新条目 `Kind::Alias = 8`。`Peer` 与 alias 编解码拆到 `peer.rs`，`channel.rs` 回到 400 行内。

### 群组（E1）

**一个群组不是密码学对象，而是一份名单。** 每一对关系仍然有自己的密钥与自己的流；
发给五个人就是写五个 drop。于是「谁删」不再是共识，不需要群密钥协商，
而踢人就是不再给他写——这也是为什么名单只有**整份替换**一种写法，没有 add / remove：
一条规则一处权威，而且“现在名单就是这些人”比“把某人去掉”更难写错。

磁盘形式是纯文本，与 `revoked` 同一个形状：**首行是群组在本地的名字，其余每行一个成员通道名**。
不需要长度前缀也不需要转义，因为 `naming::check` 已经把名字限在 `a-z0-9-` 上，
里面不可能出现换行。文件名仍是 `naming::filed`，所以目录里看不出群组叫什么；
首行与归档名不符就是 `BadRecord`，与通道记录同一条规则。

**空名单是合法的，它就是删除。** 一个没有成员的群组扇出到零条通道，
这比再加一个 `ungroup` 动词少一条规则。

### 房间（F8 · D-17）

**一个房间是一份共享秘密加一份签名名册，不是 E1 的广播名单。** E1 名单的成员互不可见；房间成员共享同一 ward、同一秘密派生的流，读一次 sweep 取回全体——代价是成员互知 handle（D-17，明写）。`room.rs`（`Room`：名、32 字节秘密、ward、founder 签名的 `kernel::Roster`、`roster_at: Option<u64>`（名册取自 founder 流的高度，读端据此决定从哪走 founder；杀进程不改结果）、`ushers`（未消费的一次性邀请钥匙）、locator、opened），**记录版本 3**；`RoomOffer` v2。名册与 usher 列表**不走 `put_block`**：两者自带计数字节自定界，而 32 把钥匙 87 KiB 会让 u16 前缀饱和截断——`put_block` 的饱和对名字、locator、grant 仍不可达，对名册不成立。`Room::founder()` 是「founder = 名册第一人」的唯一定义。`holds` 与 `forget` 同时认通道与房间：cairn 与 sweep 记录以名字归档，两个命名空间会互相继承高度。**sweep 记录改键 `(name, ward)`**（`SWEEP_FILING` v2）：一次列举服务该 ward 下所有 lane，房间一份、通道两份。判据：crate 内四条（往返含 `roster_at`；尾随字节与陌生版本被拒；32 人记录与 offer 往返且记录超过 u16；offer 往返）。

### 节奏与释放（I3 · C4 · I5）

通道记录升到**版本 4**，一次升级容纳两项，因为它们都是「这一端在网络上做什么」而不是
「这一端知道什么」：

```
… standing 之后 …
cadence     1|5 bytes   0 = 按需；1 = 时隙，后接 u32 秒数
retention     1 byte    0 = 保留；1 = 对端确认后释放
```

**`cadence` 变长而非定宽**：按需通道不写那个它没有的数字。补四个零会在记录里留下
四个谁也不读的字节，那就是同一条记录的第二种拼法，`tests/robust.rs` 判红——
一个解码器不读的字段，是一个别人改了没人发现的字段。（这条是测试查出来的，不是设计出来的。）

**两端不互相通知。** 两者都是本端的局部决定，两端可以不一致：一端填时隙、另一端按需应答，
协议一字不改。一个端点被观察到什么，只由它自己决定——这是这个选择唯一值钱的形状。

**相位在 `seal::phase(secret, handle)`**，每端每通道各一个。两端同相位就会同时换时隙，
宿主不用解密就把两条流配上了对——那正是派生地址存在的理由。

**时隙号先落盘再写 drop**（`slots.rs`）。中途被杀因此**跳过**一个时隙而不是在一个时隙里写两次。
两者都不对，但不一样不对：跳过是一个缺口，而离线本来就会产生缺口，`cadence.rs` 已经把它写成
残余泄漏；一个时隙里两个 drop 是一次突发，而突发正是时隙要消掉的形状。

**棘轮搭在 `Retention::ReleaseOnAck` 上，不另设开关。** D-01 已裁：释放解决「宿主老实时不留历史」，
棘轮解决「宿主偷留副本时也解不开」，是同一件事的两步。两个开关会让人只选到那个单独不起作用的一半。

**归档新增两种条目**（`Kind::Ratchet = 6`、`Kind::Outbox = 7`），各带通道名前缀。
少带棘轮的备份会还原出一个**打不开自己通道**的站点。

## 15 影响面

上游只有 `kusanagi` 一个。磁盘格式的任何改动都是**别人机器上已存在的文件**的改动，
所以版本字节先动，解码器拒绝的分支先写。

## 16 测试与约束

**解码器健壮性**（`tests/robust.rs`，H4）：任意字节与任意文本只产生答案；`Channel` 记录里
长度前缀在分配之前被比较；解码器读到的每个字节都有意义。另有一条此前没写下来的性质——
**一条通道记录的长度不说明邀请是否被接受**：`peer: None` 时仍写满一个定宽公钥块，那块字节不被读取。

15 个单元测试就在三个文件里，跨过边界的那一条断言在 `kusanagi/src/complaint.rs` 的测试模块。
本 crate 没有集成测试目录：它的端到端行为是 `kusanagi/tests/` 的动词，归档由
`kusanagi/tests/backup.rs` 七条判定——往返后 `channels --json` 逐字节相同、cairn 回来了
（`--after` 之后读到零段）、错密钥得 `kusanagi.bad_recovery_key`、归档字节里 grep 不到通道名
与身份、落在已有身份上被拒、空站点导出得 `kusanagi.no_identity`。

## 17 文档同步

1. 本文。
2. `ARCHITECTURE.md` §4 词表（`Site`）、§5 crate 图与行数表。
3. `crates/kusanagi/kusanagi-SPEC.md` §7 模块边界。
4. `AGENTS.md`「机器持有的规则」的 `unsafe` 一行与允许清单条数；根 `Cargo.toml` 的允许清单；`crates/vault/vault-SPEC.md`。
5. `crates/door/door-SPEC.md` §12——`site.permissions` 这个码。


---

## 附：cairn 落盘，以及读写两侧刻意的不对称

`<root>/cairns/<name>/<author>` 是站点的第四种文件。文件名取自 cairn 内部的 author，因此一条记录不可能被归档到它并不描述的那条流下面。`forget` 连同删除；留着会让同名的新 channel 继承陌生人的高度。

```rust
pub fn cairn(&self, name: &str, author: &Handle) -> Result<Option<Cairn>, SiteError>;
pub fn mark(&self, name: &str, cairn: &Cairn) -> Result<(), SiteError>;
```

**读取侧：任何读不出来都报告为「没有」。** 这是一条规则，不是被吞掉的错误。cairn 是站点上**唯一可重算的东西**——把流从高度零走一遍就能精确重建——所以退化永远正确，而任何别的答案都会让一次撕裂的写入、一个旧版本的记录或一次权限故障，把整条 channel 读不出来。

代价是丢掉一个信号：cairn 正在被人删除的端点每次都从创世走起，并且不抱怨。之所以接受，是因为拒绝换不回来——**能篡改 cairn 的人就能删除 cairn，而删除与「从未读过」不可区分**。

**写入侧：失败要报出来。** 读取的 miss 是代价；拒绝写入的磁盘是关于这个端点的事实，运营者总得从某处知道，否则此后每次读取都付一次完整行走而没有任何东西说明原因。

**新依赖 `kusanagi-chain`。** 与已有的 `grant`、`seal` 同为同层边，图仍然无环。site 存的是 `Cairn` 的字节而不是自己定义一份记录格式——编解码的权威留在 `chain`。
