# kusanagi-SPEC

> `kusanagi` —— 十个动词、一个装配点。这是唯一知道具体东西存在的 crate。
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
| U14 输出里没有重复 | `Carried` 枚举：`text` 与 `payload` 二选一；段里不再有 `id` 与 `address` | 一条普通消息的 JSON 里，同一句话只出现一次；非文本载荷只出现十六进制那一次 |
| U8 代理可用的门 | 载荷进得去也出得来，增量读得到，参数错误也带码 | 任意字节经 stdin 进、经 `payload` 出且逐字节相等；`--after H` 只报 H 之后的段；每一条失败都有稳定码与恢复命令 |
| U12 邀请不进 argv | `join` 只从 stdin 读邀请，没有位置参数 | 四种粘贴形式（裸行、`\n`、`\r\n`、前后空白）都能加入；空管道与洪水管道各得一个带码的拒绝，不挂起、不无界缓冲 |
| U13 通道名与正文不进 argv | 每个收名字的旗标接受 `-`：名字来自 stdin 第一行，其余仍归该动词 | 一整条通道（invite → join → send → read）跑完，argv 里不出现通道名，也不出现正文；`--to -` 同时给出文本参数被拒 |
| U9 退出一条通道 | `forget` | 忘掉后 `channels` 不再列出它；同名可以重新 join；撤销表不受影响 |
| U10 看自己写过什么 | `read --mine` | 崩溃后不写入即可问出自己的链头；报告的 `author` 是自己 |
| U11 通道的现时权限 | `channels` 报 `can` 与 `expires_at` | 过期或被撤销的通道在列表里就能看出来，不必先失败一次 |

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
14. 一段不是合法 UTF-8 的字节经 `send` 进去，经 `read --json` 的 `payload` 出来，**逐字节相等**（`payload.rs`），而**同一条记录里没有 `text`**。从前两者并存，`text` 带着替换字符，而下游分不出它与真货——**一个看起来无损的有损渲染，比没有渲染更坏**。现在报哪一个是关于字节的事实：全是文本报 `text`，不是文本报 `payload`。
15. 三段之后 `read --after 0` 只报两段，而 `height` 仍是已验证的链头——增量报告不得影响验证。
16. 经**真正的二进制**管道写入一段，再用 `--json` 读回（`door.rs`）——前端那几十行胶水只能这么测，而那正是代理真实走的那扇门。
17. 自己发出的邀请被拒，得 `kusanagi.own_invitation`（`from_adversary.rs`）。**这条验收不是人想出来的**：`adversary/` 在首次完整运行时把它最小化成四步轨迹，那个文件就是它的渲染结果。
18. `forget` 之后通道不再被列出，同一个名字可以重新 join，而撤销表里的条目仍在（`leaving.rs`）。
19. 一个端点连发三段后**不写任何东西**就能读回自己的链头，`author` 是自己的 handle（`leaving.rs`）。
20. 被撤销的一端在 `channels` 里就看得出来：`can` 为空，`refused` 是 `grant.revoked`（`leaving.rs`）。

## 3 假设与歧义

| 歧义 | 假设 | 何时失效 |
|---|---|---|
| 一条通道几方 | 两方 | cohort 落地后由名册决定；`Channel` 需增加成员列表 |
| 邀请方如何知道受邀方是谁 | 受邀方在**介绍流**的 0 号高度写一段，内容是自己的**公钥加 grant**，而这一段由邀请中的一次性 bearer 密钥签名 | 永不失效；这是零往返引荐的最小构造 |
| 为何问候不由受邀方自己签 | 邀请方正是要从这条消息里**学到**受邀方的公钥，只有那把钥匙能验的消息它读不了；bearer 密钥是两端已经都持有的那一个，也正是介绍流地址的派生根据 | 永不失效；参见 `kernel-SPEC.md` §10 步骤 6 |
| 通道名 | 本地私有，从不上线 | 永不失效 |
| 身份文件权限 | 不设 Unix 模式位 | 跨平台一致优先；见 §14 |
| 读操作可否写盘 | 可以，且仅限一处：把已验证的 peer 记下来 | 见 §10 步骤 4 |
| 读自己的流要不要权限 | **不要**。见 §10 步骤 11 | 除非某天自己的流不再由自己的密钥派生 |
| `forget` 要不要顺带撤销 | 不要。忘记是本机动作，撤销是对世界的声明 | 永不失效；两件事失败方式不同 |

## 4 现状分析

骨架期的欠账「`assembly::run` 接收 clap 的 `Command` 类型，因此无法从测试驱动」已偿还：crate 变为 lib + bin，动词集合由 `Request` 这个纯枚举定义，clap 只在 `main.rs` 出现。代价是多一个枚举与一次翻译；收益是九个动词的端到端行为由 `cargo test` 判定，而不是由一个没人跑的 shell 脚本判定。

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
walk.rs       peek / walk —— 读一条流并逐段检查
report.rs     Outcome —— 一个值，两种渲染
complaint.rs  Complaint —— 失败 + 稳定码 + 恢复命令
world.rs      时钟与熵的唯一采样点
assembly.rs   十个动词的组装
main.rs       clap ↔ Request
intake.rs     动词从 stdin 收下的一切（属二进制，不属 lib）
```

磁盘那一半已拆出为 `kusanagi-site`（`Site` / `Channel` / `Invite`），见
`crates/site/site-SPEC.md`。本 crate 依赖全部六个内部 crate，加 `clap`、`getrandom`、
`serde`、`serde_json`、`thiserror`。

### 行数预算：拆分已完成

上一版记下的欠账是「`src/` 到了 2,405 / 2,500，下一次实质改动的第一步是拆分」，
以及拆分前必须先回答的那个问题——`Local` / `Malformed` 这个「本机 IO 失败」的形状归谁。

**答案：形状归碰了磁盘的那一层，名字归门。** `SiteError` 说「读身份文件时操作系统拒绝了」，
`Complaint` 说这叫 `kusanagi.local`、以及「检查 `--root` 指向一个可写目录」。
恢复是用动词说的，而动词只有前端有；合成一个类型就等于把 `kusanagi channels` 这句话
写进一个没有动词的 crate。拆分照此执行，`src/` 由 2,424 降到 1,494。

剩下的一条缝仍不建议现在切：把 `main.rs`（约 240 行）拆成前端 crate 买到的是
**最不占脑子的那 240 行**，预算存在是为了「一个想法能装进脑子」，搬走 clap 胶水
只改善数字——那叫挪门柱。等 `ARCHITECTURE.md` §9 的 `port` 真的到来再说。

## 8 接口先行

```rust
pub fn run(site: &Site, request: &Request) -> Result<Outcome, Complaint>;

pub enum Request { Identity, Channels, Invite{..}, Join{..}, Send{..},
                   Read{ name, after, whose: Whose }, Revoke{..}, Forget{..},
                   Doctor{..}, Host{..} }

/// 读哪一条流。布尔旗标会在调用点丢掉这个名字，枚举不会。
pub enum Whose { Peer, Mine }

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
forget：删掉本机那一个通道文件。撤销表不动，宿主上的字节不动
```

## 10 实现逻辑

**步骤 1：动词集合是一个枚举，不是 clap 的形状。** 这样第二个门面（socket、MCP）到来时是加法，而不是把动词再教给第二个解析器。

**步骤 2：邀请携带一次性密钥。** 写邀请的人不可能知道谁会接受，所以 grant 签发给一把随邀请同行的钥匙；接受者立刻把它转授给自己的 handle，那把钥匙此后再不使用。撤销这一节，被切断的正好是用过它的那一个人。

**步骤 3：一次性由宿主保证，不由簿记保证。** 介绍段落在一个一次性写入的地址上，因此第二次接受被**宿主**拒绝。程序里没有任何东西记录一份邀请是否用过。

**步骤 4：读操作允许写一次盘。** `greet` 把已验证的 peer 记进通道文件，存的是**公钥**，因为之后每一次读都要拿它验签。它是「三件事同时成立」之后的结论——问候本身在解码时已经对着一次性 bearer 公钥验过、grant 源自本通道的根、且该 grant 签发给的正是问候里宣告的那把公钥的 handle——每条命令重算一次只会付一次必然得到相同答案的请求钱。三者不一致时的报告是 `kusanagi.bad_greeting`。

**步骤 5：两端都检查权限。** `send` 检查自己的 standing，`read` 检查 peer 的 standing 是否允许 Send。第二项才是真正的执行点：撤销之后，对方写的东西在**这一侧**被拒，而不需要对方或宿主的配合。

**步骤 6：`host` 的进度写 stderr。** 一个永不返回的动词不能用「返回值即结果」的形状；stdout 只承载结果，绑定地址写在 stderr。

**步骤 7：通道名当作路径分量来校验，不做转义。** 只放行 `a-z0-9-`、长度 1..=32。转义容易写错的方式全都始于「允许一点有趣的东西」。

**步骤 8：载荷是字节，不是字符串。** `Request::Send` 收 `Vec<u8>`；命令行给了文本就用它的字节，没给就从 stdin 读到 EOF。理由不是便利：代理要发的东西里有引号、换行与非 UTF-8 字节，而 argv 既有长度上限又要经过一层 shell 引用规则。**入口有界**：最多读 `MAX_PAYLOAD + 1` 字节，多出的那一个字节让 kernel 给出 `segment.payload_too_large`，而不是让本进程去吃一个无界的管道。

**步骤 9：`--after` 只剪报告，不剪验证。** 链仍从创世段逐段验证（`ARCHITECTURE.md` 的读取契约），`--after` 只决定哪几段进入 `segments`。它省的是输出与调用方的比对，**不省请求钱**——把它写成省钱会诱使人以为验证变短了。

**步骤 11：读自己的流不查权限。** `--mine` 走的是同一条 `walk`，但不问 standing。
理由不是宽松而是诚实：那些段由自己的密钥派生的地址装着、由自己的签名签着，
检查过不过都拿得到同样的字节，**执行不了的检查是表演**。真正的执行点在别处——
`send` 查自己能不能写、`read` 查对方当时能不能写，两处都能真的拦住事情发生。
过期或被撤销之后仍然读得到自己写过什么，这正是崩溃后要恢复的那个代理需要的。

**步骤 12：`channels` 用本地时钟验一次 standing。** 列表里的 `can` 是**验证过的**能力，
不是记录里抄出来的声明；`refused` 出现时带的是 `grant.*` 那个稳定码。
一次网络请求都不发，因为过期与撤销都是本地事实。
代价是同一条通道在 `channels` 与 `send` 之间可能跨过失效时刻——那不是不一致，
那是时间。

**步骤 13：`forget` 只删本机那一个文件。** 它不撤销、不通知对方、不动宿主上的字节，
也不清撤销表——撤销必须活得比通道记录长，否则重新 join 同一个名字就能让一份
已撤销的 grant 复活。它删掉的是通道密钥，所以那条通道**再也回不去**，这句话必须
出现在散文输出里。

**步骤 14：名字与正文都能不进 argv，机制只有一条。** 命令行是公开的——同机器任何账户在进程运行期间读得到别人的 argv，shell 事后还留一份。`ARCHITECTURE.md` §8 为邀请裁过这件事，本步只是把同一条裁决推广到它的同类，而**通道名泄漏的东西比邀请更重**：邀请泄漏一次入网机会，`send --to bob` 每发一条消息就泄漏一次「谁在跟谁说话」——正是 §3 属性 2a/2b 用地址派生去藏的关系图。

规则一条：**名字旗标取值 `-`，即 stdin 第一行是名字，其余仍是该动词本来就要从 stdin 读的东西。** 于是 `read`/`revoke`/`forget`/`invite` 的管道里只有一行名字，`send` 的管道里是名字加正文，`join` 的管道里是名字加邀请，四种形状共用一个 `split_name`。

**`--to -` 同时给出文本参数是拒绝，不是容忍。** 藏住名字而让正文留在命令行是半个修复，而一个读起来像整修的半修比不修更坏。

**步骤 10：参数错误也走 `Complaint`。** 以前 clap 到 `Request` 的翻译失败只向 stderr 打一句散文，于是门上有一类失败没有稳定码。现在它们是 `Complaint::Argument`，与其他十四种失败同形。这是**删掉一个特例**，不是加一个。

## 11 边界枚举

| 情形 | 期望 |
|---|---|
| 尚无身份就执行任意动词 | 首次使用时自动生成；不需要 setup 步骤 |
| 重复 `adopt` | 保留原身份，绝不覆盖（覆盖等于静默放弃全部通道） |
| 通道重名 | `kusanagi.channel_exists` |
| 名字含 `../`、`/`、大写、空格 | `kusanagi.malformed` |
| peer 尚未加入就 `read` 或 `revoke` | `kusanagi.no_peer_yet` |
| peer 就是根权威而试图 `revoke` | `kusanagi.cannot_revoke_root` |
| **接受自己发出的邀请** | `kusanagi.own_invitation`。流由 `(secret, author)` 派生，所以自接会让一个端点拿到**同一条流的两个本地名字**，认出的 peer 是自己，于是 `read` 把刚写的东西当作对方说的递回来。**这一条是 `adversary/` 找出来的**，见 §2 第 17 条 |
| 对方流上出现别人签名的段 | `kusanagi.not_the_peer` |
| 下一个地址已被占 | `kusanagi.drop_taken`，并给出重读后重发的命令 |
| 通道文件版本不认识 | 拒绝而不是猜 |
| `send` 未给文本且 stdin 是终端 | `kusanagi.argument`，告诉他两种给法。**不得阻塞等一个人打字** |
| `join` 且 stdin 是终端 | 同上：`kusanagi.argument`，告诉他怎么管进去 |
| 名字旗标是 `-` 而 stdin 第一行为空或不是文本 | `kusanagi.malformed`（`BadName`），不把空名字送进下一层 |
| 名字旗标是 `-` 而管道里只有名字没有正文 | 照发。空载荷是合法的段，与「stdin 给了零字节」同一条规则 |
| `send --to -` 又给了文本参数 | `kusanagi.argument`。见 §10 步骤 14 |
| `join` 的 stdin 为空或不是邀请 | `kusanagi.malformed`，且恢复命令**必须提到管道**——只说「拷贝整条邀请」会把人送去找一个不存在的参数 |
| `join` 的 stdin 超过 16 KiB | 读到上限就停、拒绝并退出。父进程因此会拿到 EPIPE，**那正是上限生效的证据** |
| stdin 给了超过 `MAX_PAYLOAD` | `segment.payload_too_large`，由 kernel 判定，本层不重复那条规则 |
| stdin 给了零字节 | 照发。空载荷是合法的段，拒绝它需要一条没人写过的规则 |
| `--after H` 中 H ≥ 链头 | `segments` 为空而 `height` 照报——这正是轮询者要的那一条回答 |
| `--can` 里出现不认识的词 | `kusanagi.argument`，而不是静默地少授予一项 |
| **命令行 clap 都解析不了**（`-root`、`rea`、缺子命令） | `kusanagi.argument`，退出码 1，`--json` 时仍是 JSON。**这条是 `adversary/` 的键盘性质找出来的**：原先它走 clap 自己的出口，退出码 2 且只有散文 |
| `--help` / `--version` | 不是失败：照打到 stdout，退出码 0 |
| 不带任何动词 | 打印帮助，退出码 0。人是在提问，不是在犯错 |
| waypoint 写成 `ftp://…` 这类不认识的 scheme | `locator.unknown_scheme`，而不是当成一个相对目录去实测 |
| `forget` 一个不存在的通道 | `kusanagi.unknown_channel` |
| `forget` 之后再用同名 `join` | 允许。名字是本地的，忘掉即空出 |
| `read --mine` 而自己一段都没写 | `height` 为 `null`，`segments` 为空，不是错误 |
| `read --mine` 在被撤销之后 | 照读。见 §10 步骤 11 |
| `channels` 里有一条 grant 已过期 | 列出来，`can` 为空，`refused` 为 `grant.expired` |

## 12 错误处理

`Complaint` 十八个变体，每个带稳定码与**恢复命令**。「格式不对」是三个变体而不是一个——
`BadName`（你打的名字）、`BadInvitation`（你贴的那行）、`BadRecord`（你盘上的文件）——
三者**共用已公开的稳定码 `kusanagi.malformed`**，因为码是脚本匹配的东西，
而恢复命令必须各说各的：把名字打错的人被告知去拷贝邀请码，是把他送进另一个错误。
（同样由键盘性质找出。）四条与众不同：

- `kusanagi.argument` 是唯一把恢复文字**随变体带进来**的（`instead` 字段）。其他变体的恢复由失败种类推出，而一个参数错在哪釬只有写下那个旗标的地方知道——这与 `complaint.rs` 开头的理由是同一条，只是又往上一层。

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
| `kusanagi1:` 前缀、版本 1、套件 1 | 邀请串的识别与拒绝不认识的格式 | 套件 0 是 Ed25519 时代的那一个，已被按号拒绝；换密码学即换套件字节 |
| 站点文件的权限已不在本层 | 现在由 `kusanagi_site::permissions` 一处决定：Unix 上文件 `0600`、目录 `0700` | 见 `site-SPEC.md` §14；Windows 仍是缺口 |
| `join` 的 stdin 上限 256 KiB | 由最宽的那一份倒推：公钥 2 592、密钥与种子各 32、八跳 ML-DSA-87 grant 58 345，十六进制后约 12 万字符 | 邀请里的 grant 链大幅变深时要重算 |
| 名字旗标的哨兵值 `-` | Unix 已有的「这一项从 stdin 来」约定，不新造记法 | 这个哨兵要求名字不得以 `-` 开头，而那条规则只在 `kusanagi_site::check_name` 一处（见 `site-SPEC.md` §10 步骤 1） |
| 名字行最多 64 字节 | 只是缓冲上限，不是名字规则；名字合法性归 `kusanagi_site` 一处判 | 放宽名字长度时这个数要跟着走 |
| `--after H` 是**严格大于** | 调用方手里持有 H，要的是 H 之后的 | 改成含 H 会让每次轮询重复一段 |
| stdin 最多读 `MAX_PAYLOAD + 1` 字节 | 越限由 kernel 判，本层只负责不无界 | 跟随 `kernel::MAX_PAYLOAD`，不另写常量 |
| `payload` 用小写十六进制 | 全仓只有一套十六进制编解码（`kernel::wire`），不为一个字段引入 base64 | 体积翻倍；但它现在**只在载荷不是文本时出现**，所以翻倍只落在真需要它的那一类载荷上 |
| 一个段只报 `text` 与 `payload` 之一 | 从前两个字段说同一句话，其中一个还不可读；合法 UTF-8 装进 JSON 字符串本就无损，所以并存的那一份纯属重复。**用枚举承载，两者因此不可能同时出现或同时缺席** | 读的人要处理两种键；作为交换，普通消息的输出体积减半 |
| 段里不再有 `id` 与 `address` | 两者都是可重算的派生值，而且没有任何调用方读它们；`sent` 仍然报 `address`，因为写入的人确实需要一个引用 | 需要它们的人要自己算，或者提一个带旗标的条目 |
| `read` 的输出字段是 `author` 而不是 `peer` | 加了 `--mine` 之后那条流可能是自己的，`peer` 会变成一句假话 | 门上的**破坏性改名**，pre-alpha 期内按 README 的约定直接改 |

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


---

## 附：读取路径的起点是隐私决策

**问题。** `walk` 从高度零走到第一个空地址，`--after` 在走完之后才过滤，`send` 也要走完自己整条流才知道下一个 index。于是一次轮询把整条流的地址按升序连续报给主机——`seal` 存在的目的在读取路径上被抵消。红灯证据：`crates/kusanagi/tests/unwatched.rs`，12 条消息的一次轮询点名 13 个地址。

**`Reach`——编码调用方的需求，而不是机制。**

```rust
pub enum Reach { Whole, Above(u64), Head }
pub fn track(site, name, waypoint, stream, author, reach) -> Result<Walked, Complaint>;
pub fn walk(waypoint, stream, author, name, from: Option<Cairn>) -> Result<Walked, Complaint>;
impl Walked { pub fn cairn(&self) -> Option<Cairn>; pub fn extended(&self, &Segment) -> Result<Option<Cairn>, Complaint>; }
```

「往回取多远」由需求与磁盘上的 cairn 共同推出，而这个推导只应存在于一处。**`Above(floor)` 在 cairn 高于水位时必须退回整链行走**：中间那些段是调用方要求看的，续读永远不会取回它们。这是本次改动最容易安静丢消息的地方，由 `a_read_that_shows_segments_shows_every_one_it_was_asked_for` 守住。

**`send` 在写入成功后推进位置。** 主机在一个空地址上接受了写入，所以本端点无需读回即知段已在那里；`Walked::extended` 把验证器再前进一格。否则记录会永远落后一格，每次发送都要多问一个地址去重新发现自己刚写的东西。

**`confirm`——只对整链行走做。** 续读不可能与它续的记录矛盾，因为它就从那里开始；整链行走可以，而那是主机唯一能靠「少给」说谎的形状：交回一条更短但验证完美的链，没有记忆的读者会相信。两种矛盾各自具名：流比记录短，或已读高度上的段换了一个。

**`Complaint::HistoryChanged`，码 `kusanagi.history_changed`。** 恢复命令指向 `kusanagi doctor <waypoint>`：只有 write-once 的主机能承诺这件事不发生，而这一台刚刚做了。

由 `adversary/src/Kusanagi/Lying.hs` 找到，`crates/kusanagi/tests/lying.rs` 记住。
