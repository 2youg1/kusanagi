# adversary-SPEC

> `adversary/` —— 仓外的对抗性性质预言机。它不是 crate，不进 workspace，不进发布物，不进 `just check`。
>
> 权威顺序：用户裁决 → `ARCHITECTURE.md` §8「The adversary is out of the workspace」→ 本文 → 代码与测试。本文先于代码改动。

## 1 需求拆解

`ARCHITECTURE.md` §8 允许一个仓外的 Haskell 预言机存在，条件是它永远不会长成第二权威。拆成五个可独立验收的最小单元：

| 单元 | 交付物 | 独立验收 |
|---|---|---|
| U1 门 | `Door`：以子进程驱动已发布的二进制，把两个流解成 `Answer` | 一条 invite→join→send→read 的轨迹被解析成结构化值；未知的 `command` 标签当场报错而不是被忽略 |
| U2 场地 | `Ground`：一次性根目录、一台宿主目录、若干 site，以及宿主的敌意动作 | 场地退出后不留文件；`stored` 看到的正是宿主看到的 |
| U3 模型 | `StateModel` / `RunModel`：动作集合与后置条件 | 随机轨迹全通过；把 revoke 变成 no-op 则立即判红 |
| U4 定向对抗 | dynamic logic：任意前缀 ＋ 一次撤销 ＋ 任意后缀 | 该性质在随机轨迹上成立，且撤销后的每一次读都被拒 |
| U5 回归 | 反例最小化后渲染成 Rust `#[test]` | 渲染结果与仓内那个 Rust 文件逐字节相同，而该文件由 `cargo test` 编译运行 |
| U6 键盘与引导 | `Keyboard`：按键级的错字、粘贴损伤、管道载荷 | 四条关系性质，见 §2 第 6–9 条 |
| U7 命令行不认人 | `Overheard`：同机器另一个账户从 argv 里读得到什么 | 见 §2 第 10 条 |
| U8 观测中继 | `Relay`：站在真 `kusanagi host` 前面的 TCP 中继，每一次请求记一条 `(单调时刻, 方法, 路径)` | 见 §2 第 11–13 条 |
| U9 时序特征 | `Tempo`：把那些时刻变成三个可下阀值的数，并在两组世界上求声明清单的等式 | 同上 |

**不负责**：任何规则的再实现（地址派生、签名、格上的交）；任何 Rust 侧的构建闸门；任何随产品交付的东西。三者中任何一条被违反，本目录应当被删除而不是被修补。

## 2 验收标准

1. `cabal test` 全绿。
2. 没有 GHC 的机器上 `just check` 的行为与本目录不存在时**逐字节相同**；`just adversary` 打印 `skipped: GHC is not installed` 并返回 0。
3. 模型不预测任何地址、摘要、签名或密文。凡断言只谈**两条轨迹之间的关系**。
4. U5 渲染出的 Rust 源码与 `crates/kusanagi/tests/from_adversary.rs` 逐字节相同。
5. **咬得动的证据**：`Model.hs` 的 `revocationIsFinal` 在把 `Cut` 从模型里摘掉后必须失败。一个永远为真的性质与没有性质等价。

### U8 与 U9 的三条（数字接上文）

宿主什么都不应该记。**记的是站在宿主前面的人**，而本目录正是那个人——
`ARCHITECTURE.md` §3 的路径观察者看不到字节，只看得到**什么时候有包**。

11. **中继真的看得见。** 一个说了话的世界里，中继至少记下两条观测，每条的路径都形如 `/d/<十六进制>`。
    **这条先跑**：一个什么都没量到的特征集在下两条上一定是绿的，而那种绿比红更坏。
12. **说了多少字，在时间上不分离。** 同样条数的消息，一条一个字节对一条三千字节：
    三个时序特征**一个都不得分离**。定长封装把尺寸抹平了，它不应当从时间里漏回来。
13. **有没有人说话，只按写下来的那份清单分离。** 沉默的世界对忙碌的世界，分离集合与
    `Tempo.declared` **相等**。它今天不是空集；I3 的公开时隙落地后它必须变成空集，
    而那一天这条性质会因为“声明了却没发生”而判红——**等式两边都咬人**。

### U6 的四条（数字接 §2 上文）

人不是通过 `Verb` 调用这个程序的，人是通过键盘。以下四条**全部是关系**，
没有一条预测输出：

6. **两扇门说同一件事。** 任意命令行（包括打错的），若被拒，则加上 `--json` 后
   stderr **必须能解成** `{error, code, recover}`。一个只对人说话的失败，对代理而言等于没有发生。
7. **引导是可执行的。** `recover` 里出现的每一条 `kusanagi …` 命令，拿去真的跑，
   **不得以 `kusanagi.argument` 失败**。叫不出来的命令不是引导，是安慰。
8. **引导不谈你没给的东西。** 若 `recover` 提到 `kusanagi2:` 那行邀请，则本次输入里
   **必须真的有一个邀请参数**。把名字写错的人被告知去拷邀请码，是把他送进另一个错误。
9. **代理的字节原样进出。** 任意字节串经 stdin 进、经 `payload` 十六进制出，
   逐字节相等；包括换行、引号、反斜杠、非 UTF-8 与零字节。

### U7 的一条（数字接上）

10. **命令行不出现任何认得出人的东西。** 对任意合法通道名与任意正文，
    每个动词的 argv 里既不出现那个名字，也不出现那段正文，而名字仍然经 stdin 送达。
    `ARCHITECTURE.md` §8 为邀请裁过这件事；通道名比邀请更重——邀请泄漏一次入网机会，
    `send --to bob` 每发一条消息就泄漏一次「谁在跟谁说话」。
    **这条性质护住的是整套测试走的那扇门**：`Door` 现在用 `-` 驱动每一个动词，
    所以它一旦回退，失败的是十八个测试而不是一个。

另外，每一条回答都只允许两种形状：**退出码 0 且 stdout 是一个 `Outcome`**，或
**退出码 1 且 stderr 是一个带稳定码与恢复命令的 `Complaint`**。其余退出码（包括 clap 自己的 2）
都是门上的一个缺口。**形状可以断言，内容不可以**：这里不枚举错误码表，枚举一份就是在仓外养第二份契约。

## 3 假设与歧义

| 歧义 | 假设 | 何时失效 |
|---|---|---|
| 门的形状 | CLI 的 `--json`：成功走 stdout 的 `Outcome`，失败走 stderr 的 `{error, code, recover}`，由退出码选择 | `port` 落地后门变成它的 schema，改 `Door.hs` 一处 |
| 宿主是什么 | 两种世界。大多数性质里它是一个本地目录，分片规则为地址前两字符；U8/U9 里它是一个真 `kusanagi host` 进程，经 `Relay` 拿到 `http://` locator。**两种世界共用同一个目录**，所以 `stored` 不分彼此 | 没有时序问题的性质不花进程钱；敌意动作仍走目录 |
| 时钟 | 只用两档 `--for`：`0`（当场过期）与 `3600`（本轮不过期）。模型不预测时钟 | 若将来要测过期的时刻边界，须由产品提供可注入的时间，而不是由本目录去猜 |
| 身份的数量 | 三个 site：`alice`、`bob`、`mallory`。一条通道恰好两方 | `cohort` 落地后名册进入模型 |

## 4 现状分析

Rust 侧的验收测试全部是**具体轨迹**：它们证明「这一条路走得通」，不证明「任何一条路都走不出去」。本目录的全部增量在后者，以及一件 Rust 侧写不出的事——**为一个撒谎的宿主建模，本质是在一个策略空间上做不确定性选择**。

### 第一个发现（首次完整运行）

随机轨迹在第 13 个样本上失败，收缩 8 次后得到四步：

```
var1 <- Invite Alice "one" Forever {send,read}
        Join   Alice var1 "two"        -- 自己接受自己的邀请
        Send   Alice "one" "beta"
        Read   Alice "one"             -- heard ["beta"] where [] was said
```

**诊断**：流由 `(secret, author)` 派生。自接之后，一个端点拿到同一条流的两个本地名字，`greet` 从介绍流里认出的 peer 就是它自己，于是 `read` 把自己刚写的段当作对方说的递回来。对一个自主代理而言，把自己的输出当输入读是反馈环，不是对话。

**处置**：在 Rust 侧 `join` 拒绝 `inviter == 自己`，新错误码 `kusanagi.own_invitation`；模型学会这一条拒绝；最小化后的轨迹渲染成 `crates/kusanagi/tests/from_adversary.rs`。知识已经迁到 Rust，Haskell 这边只留下那条性质。

### U6 首次运行的两处发现

1. **一次漏按就绕过了整扇门。** `-root`（少一个横杠）走的是 clap 自己的出口：退出码 2、
   只有散文、`--json` 无效。对代理而言，那等于这次失败没有发生过。
   处置：`main.rs` 改用 `try_parse`，把解析失败译成 `Complaint::Argument`；
   `--help`/`--version`/无动词照旧走 stdout 且退出码 0。回归测试落在
   `crates/kusanagi/tests/door.rs::a_mistyped_flag_is_a_complaint_like_any_other`。
2. **引导指向了用户没有的东西。** 把通道名打错（`--name BAD`）会被告知
   「copy the whole invitation, including the `kusanagi2:` prefix」——他手里从来没有邀请码。
   处置：`SiteError` 把「格式不对」拆成 `BadName` / `BadInvitation` / `BadRecord`，
   稳定码不变，恢复命令各说各的。

两处都符合 §4 的规矩：**Haskell 找，Rust 记**。

## 5 权威信源

| 事实 | 来源 |
|---|---|
| `quickcheck-dynamic` 4.0.1（2025-07-14），在 LTS 24.x / GHC 9.10.3 与 Nightly / GHC 9.12.4 中 | Hackage、Stackage 实测 |
| 它由 IOG 与 QuviQ 从 Plutus 测试里抽出，`StateModel` 与 `RunModel` 分属两个类型类 | 上游 README |
| `hedgehog-lockstep` 已是 Hackage 上的真包，提供集成式收缩 | Hackage |

**一处对提案的更正**：v3 提案设的复查点是「收缩结果不可读就换 Hedgehog」。当时 `hedgehog-lockstep` 尚不存在，如今它存在，但它给的是 lockstep 加集成收缩，**没有 dynamic logic**。于是这个赌注的代价现在是明确的：换过去会丢掉「任意前缀 ＋ 这个具体攻击 ＋ 任意后缀」的表达力，而那正是当初选 `quickcheck-dynamic` 的全部理由。复查点保留，但门槛提高为「收缩不可读**且**定向场景已不再需要」。

## 6 命名统一

`Segment`、`Drop`、`Channel`、`Grant`、`Standing` 一律沿用 `ARCHITECTURE.md` §4 的词表，Haskell 侧不得另起名字。本目录只新增三个词，各自只指一件事：

| 词 | 它是什么 |
|---|---|
| **Door** | 已发布二进制的 `--json` 门面，唯一知道有个可执行文件存在的地方 |
| **Ground** | 一次性的场地：一台宿主、若干 site，以及宿主可以施加的敌意 |
| **Trace** | 一串动作及其观察结果，是本目录唯一的断言对象 |

## 7 模块边界

```
adversary.cabal            工程定义；不被任何 Rust 构建读到
src/Kusanagi/Answer.hs     门的 schema 的代数镜像。只解析，不判断
src/Kusanagi/Door.hs       唯一知道二进制存在的地方
src/Kusanagi/Ground.hs     一次性场地，以及宿主的敌意
src/Kusanagi/Keyboard.hs   人怎么敲，代理怎么管道；以及引导能不能被照做
src/Kusanagi/Overheard.hs  同机器另一个账户从命令行读得到什么。纯性质，不起进程
src/Kusanagi/Model.hs      状态模型、后置条件、定向对抗场景
src/Kusanagi/Regression.hs 反例 → Rust #[test]
test/Main.hs               tasty 入口
```

`Keyboard` 与 `Model` 互不相识：一个敲字符，一个走动词。两者只共用 `Door` 与 `Ground`。

依赖单向：`Model` → `Door` → `Answer`，`Model` → `Ground`，`Regression` 只依赖动作类型。`Answer` 不 import 任何本工程模块。

## 8 接口先行

```haskell
-- Answer.hs —— 门说了什么
data Answer   = Accepted Outcome | Refused Complaint
data Complaint = Complaint { code :: Code, message :: Text, recover :: Text }
data Outcome  = Identity Handle | Listing [Summary] | Invited ChannelName Invitation Word64
              | Joined ChannelName Handle Handle | Sent ChannelName Word64 Address
              | Heard ChannelName (Maybe Word64) [Entry] | Revoked ChannelName Text
              | Forgotten ChannelName Text | Examined Text Text | Hosted

-- Door.hs —— 怎么问
newtype Door = Door FilePath
discover :: IO (Maybe Door)                       -- KUSANAGI_BIN，或 target/{debug,release}
ask      :: Door -> FilePath -> Verb -> IO Answer -- site 根目录 → 动词 → 回答
type  :: Door -> [String] -> Maybe ByteString -> IO Typed  -- 原始 argv 与 stdin

-- Keyboard.hs —— 人怎么敲
data Slip = Slip { slipName :: String, slipHit :: String -> String }
fumbled  :: [String] -> Gen [String]              -- 把一条命令行敲坏
advice   :: Text -> [[String]]                    -- 从 recover 里抽出可执行命令

-- Ground.hs —— 在哪里问，以及宿主怎么撒谎
withGround :: (Ground -> IO a) -> IO a
siteOf     :: Ground -> Site -> FilePath
waypoint   :: Ground -> FilePath
stored     :: Ground -> IO [(Address, ByteString)]
corrupt    :: Ground -> Address -> IO ()            -- 翻一位

-- 丢弃与重放属于「撒谎的宿主」那一单元，**届时才写**：
-- 一个没有调用者的敌意动作，与一句没有被证伪过的断言等价。

-- Model.hs —— 断言什么
instance StateModel World
instance RunModel World (ReaderT Ground IO)
revocationIsFinal :: DL World ()

-- Regression.hs —— 交付什么
sequenced :: [Any (Action World)] -> Actions World   -- 极性由模型判定，不由手写声明
coherent  :: Actions World -> Bool                   -- 每一步都是模型此刻允许的
render    :: Text -> Actions World -> Text           -- 轨迹 → 一个 Rust #[test]
```

`ask` 返回 `Answer` 而不是抛异常：被拒绝是产品的正常输出，而**解析失败**才是异常——门的形状变了，测试应当当场停下，而不是把新形状当成一次拒绝。

## 9 工作流程

1. `just adversary` 先 `cargo build`，把二进制路径经 `KUSANAGI_BIN` 传给 cabal。**唯一一处知道二进制在哪的地方是 justfile**。
2. `cabal test` 跑 tasty：先跑 U5 的渲染对拍（毫秒级，先失败先止损），再跑 U3 的随机轨迹，最后跑 U4 的定向场景。
3. 反例出现时，`Regression.render` 把最小化后的轨迹写成 Rust 源码打到 stderr，并给出它该被放在哪个路径。
4. 人把那个文件提交到 `crates/kusanagi/tests/`。**知识就此迁移到 Rust，Haskell 不保留它。**

## 10 实现逻辑

**门**：`readProcessWithExitCode`。退出码 0 解 stdout 为 `Outcome`，非 0 解 stderr 为 `Complaint`。两者都失败即抛出——见 §8 的理由。

**模型**：`World` 只记一个用户记得住的东西——哪些通道开着、谁是对端、每一方在每条通道上说过哪些话、谁被撤了、邀请是否用过。它**不记**地址、高度以外的任何链上事实；高度也只作为 `[Text]` 的长度间接出现。

**后置条件**，逐条都是关系而非期望值：

| 动作 | 断言 |
|---|---|
| `Hear` 且对端未被撤 | 听到的正是对端说过的那串话，顺序相同 —— 说与听之间的等价 |
| `Hear` 且对端已被撤 | 一定被拒，且 `code == "grant.revoked"` |
| `Say` 而未获 send | 一定被拒，且 `code == "grant.forbidden"` |
| `Accept` 一份已用过的邀请 | 一定被拒，且 `code == "kusanagi.invite_spent"` |
| `Accept` 一份 `--for 0` 的邀请 | 一定被拒，且 `code == "grant.expired"` |
| 每一步之后 | 宿主持有的地址两两不同，且任意两个地址的公共前缀不超过 §14 的阈值 |

**定向对抗**（U4）：先用三个具体动作把世界推到「alice 已认识 bob」，再 `anyActions_` 生成任意前缀，`action (Revoke …)` 插入那一次撤销，`anyActions_` 生成任意后缀，最后以 `failingAction (Read …)` 收口——那一步必须失败，且必须以 `grant.revoked` 失败。这是本目录存在的核心理由——**只会均匀随机生成的东西是 fuzzer，不是对手**。

**「拒绝」不是「期望输出」**。模型断言的是**哪一种失败**，从不断言任何被计算出来的值：错误码由 `AGENTS.md` 定为门的契约的一部分（每个失败都带稳定码），钉住它钉的是产品对调用方的承诺，不是对某条规则的重算。

## 11 边界枚举

| 边界 | 处理 |
|---|---|
| 二进制不存在 | `discover` 返回 `Nothing`，测试树整体标记为 skipped，退出码 0 |
| 宿主目录为空 | `stored` 返回 `[]`，公共前缀断言在少于两个地址时平凡成立 |
| 同一 site 重复 `Open` 同名通道 | 期待被拒且 `code == "kusanagi.channel_exists"` |
| 未 join 就 `Hear` | 期待被拒且 `code == "kusanagi.no_peer_yet"` |
| 撤销根权威 | 期待被拒且 `code == "kusanagi.cannot_revoke_root"` |
| 轨迹里出现 Windows 路径分隔符 | 全程走 `FilePath`，不手拼字符串 |

## 12 错误处理

本目录没有「恢复」这个概念：断言不成立就是发现，发现就该停下并交付一个 Rust 回归测试。唯一被当作错误处理的是**门的形状变了**（JSON 解析失败），它抛异常，因为继续跑只会把新形状当成拒绝，从而把红的测成绿的。

## 13 依赖选型

| 依赖 | 为什么 |
|---|---|
| `quickcheck-dynamic` | `StateModel` 与 `RunModel` 的类型分界**就是黑盒边界**，由类型强制而不靠自律；且它能写有方向的对抗场景 |
| `QuickCheck` | 上游要求 |
| `aeson` | 门讲 JSON。手写 JSON 解析器等于在这里养第二个 bug 源 |
| `process`、`directory`、`filepath`、`temporary` | 起进程、造场地 |
| `tasty` + `tasty-quickcheck` | 把三组性质编成一棵可选择运行的树 |
| `bytestring`、`text`、`containers` | 基础件 |
| `network` | U8 需要一个真实的套接字，而 `base` 没有。**中继必须在产品外面**：宿主自己记下请求时刻就是一份日志，而 §3 第 0 行正是说宿主什么都不学到。adversary 不进发布物、不进 `cargo deny`，它的供应链不是产品的供应链 |

**不引入**：任何 FFI、任何绑定 Rust 类型的东西、任何需要改 Rust 代码才能工作的东西。

## 14 硬编码声明

| 硬编码 | 意图 | 后续影响 |
|---|---|---|
| 分片规则「地址前两字符为目录名」 | 目录 waypoint 的布局，敌意动作要按它找到文件 | 该布局改变时本目录判红，属预期 |
| 三个 site 名 `alice` / `bob` / `mallory` | 固定的演员表，让反例可读 | 加人时同步改 `Regression.render` 的模板 |
| 公共前缀阈值 8 个十六进制字符 | 少量地址下随机碰撞的概率可忽略，超过即可疑 | 轨迹长度大幅增加时须重算 |
| `Veil.apart` 的重合阈值：**五倍标准差**，不是常数 | 两条独立密钥流在每个位置以 1/256 重合，所以对长 `n` 的一对 drop，期望重合是 `n/256`、标准差是 `sqrt(n·255/256²)`。阈值取 `n/256 + 5σ`：单对误报率约 3e-7，一次运行只比十几对 | **它必须随 drop 尺寸走。** 原先是常数 64，按 `DROP = 4 096`（期望 16）标定；ML-DSA-87 把 `DROP` 推到 131 072 后期望变成 512，于是三条性质全部假阳（实测 490–536，恰在 512 ± 1.1σ 内）。阈值写成 `n` 的函数后，下一次换签名方案不需要再改它 |
| `Discriminator.constantPositions` 减去**同一形状的机会底**，而不是报原始计数 | k 条独立密钥流在某个偏移上同时相同的概率是 `256^(1-k)`：两个 drop 的世界期望 512 次重合，五个 drop 的世界期望零次。**原始计数因此是「对象数」换了一个单位**，任何两个对象数不同的世界都会在它上面分开——而那正是 `drops` 已经报过的事实 | C2 让 `invite` 往宿主写一个 offer drop，沉默世界的对象数由 1 变成 2，这条特征当场开始分离，`presenceSaysOnlyHowMany` 判红。修法是减去机会底而不是把它写进 `declared`：写进去等于声称一个没有任何设计改动能关掉的泄漏 |
| `Tempo` 的突发阈值 **100 ms**，且特征只有两个 | 一次命令里的几次请求相隔毫秒级，两次命令之间隔着一整个进程启动（实测 40–55 ms，充分余量）。阈值把「一次命令」归成一个簇，于是 `gap.burst` 就是命令数 | 进程启动大幅变快（G1）时要重新标定；它需要的不是精确，而是两个量级之间的一道线 |
| **拒绝把「最长静默」当特征** | 它实测三次里分离一次（沉默 41–44 ms，忙碌 48–55 ms），而原因是算术而不是产品：**十二个样本的最大值大于三个样本的最大值**，即使两边同分布；而样本数就是请求数，正是 `gap.burst` 已经报过的事实 | 与 `constantPositions` 同一类错误，且修法同一个形状：**修特征，不修 `declared`**。写进 `declared` 等于声称一个没有任何设计改动能关掉的泄漏。三个样本的极值没有诚实的去偏办法，所以删掉它。**凡随样本量增长的统计量（最大值、极差、总和）都是样本数换了一个单位** |
| `Tempo.declared` = `{gap.burst}` | 实测：沉默世界 3、忙碌世界 12，四个世界零方差。`gap.median` 一次也没分离——两边每条命令付同一份进程启动 | I3 公开时隙落地后此表必须为空；到时不改这行，性质会以「声明了却没发生」判红 |
| `--for` 只取 `0` 与 `3600` | 见 §3 | —— |
| 环境变量 `KUSANAGI_BIN` | 二进制位置的唯一入口 | 由 justfile 提供 |

**这条阈值能抓到什么、抓不到什么，写清楚以免被当成更强的声明。** 它抓的是散布在整个 drop 上、量级超过 5σ 的重合：重用的密钥流、恒定 nonce、未加密的尾巴。它抓不到一个很短的固定字段——比如每个 drop 相同位置上的四字节 tag，`DROP = 131 072` 时它只把重合抬高 4，远在 113 的噪声底之下。首部头由 `prefixTolerance` 单独卡住；**中间的短固定字段由 `noPositionIsFixed` 卡住**——它比的不是一对 drop 重合多少，而是哪些**位置**在多个 drop 上同时取同一个值：碰巧是散的，结构是对齐的。

**同一条算术的第三种用法在 `Discriminator` 里。** `noPositionIsFixed` 问「有没有位置在所有 drop 上取同一个值」并把阈值定在零，因为八个 drop 的机会底是 `2e-12`；而 `constantPositions` 作为**特征**要在对象数不同的两个世界之间比较，机会底随对象数变化，所以它报的是超出机会底的部分。两处问的是同一个问题，用的是同一个二项式，只是一处的对象数固定、另一处不固定。

**它的阈值是零,不是一个余量。** 八个 drop 在某个偏移上恰好都相同的概率是 `256^-7`,乘以整个 drop 的长度,期望大约 `2e-12`——一条五千亿次运行才误报一次的性质,红了就是真的坏了。这也是为什么它必须问「哪些位置」而不是「重合多少」:同一个四字节固定字段，在成对计数里只是噪声的 4%,在按位置比较里是确定性。

**U8 的一个实现约束，写在这里因为它曾伪造过一个产品缺陷。** 中继必须做完整的半关闭：
`sendAll` 返回只说明操作系统收下了字节，不说明对端读走了。第一版在任一方向结束时就拆连接，
丢掉一段已经接下的应答，端点就报 `waypoint.io: Peer disconnected`——**一个量具的故障穿上了
宿主故障的衣服**，而这正是对抗测试最容易浪费人时间的那种红。现在先转完应答、再向客户端
`ShutdownSend`、然后等客户端自己关（上限五秒）。

## 15 影响面

对 Rust 侧的影响**必须**恰好为零：不改 `Cargo.toml` 的 members，不进 `cargo deny` 的依赖图，不参与 `just budget` 的行数。唯一的交汇点是 `crates/kusanagi/tests/from_adversary.rs`——它由本目录渲染、由 `cargo test` 编译，两侧任何一方漂移都会让某一侧变红。

## 16 测试与约束

六组，按「坏得越早越省时间」排序：渲染对拍（U5，毫秒级）、**键盘与引导（U6，秒级）**、
随机轨迹加宿主视角的不可关联断言（U3）、定向撤销（U4）、宿主最简单的那句谎话——改掉一个字节，读必须被拒——
以及两个分类器实验（U8/U9）。约束是 §2 第 5 条——**咬得动**必须被演示过，而不是被相信。
U6 首次运行就咬到了两处，记在 §4；U9 首次运行咬掉的是它自己的一个特征，记在 §14。

**实测**：22 项全绿，54.65s。引入中继之后总时长没有上升（上一次 19 项 58.49s）：
分类器实验本来就把世界建了一遍，现在同一个世界同时交出宿主视角与载运者视角。

## 17 文档同步

1. 本文。
2. `ARCHITECTURE.md` §5 的「Outside the workspace」段与 §8 的对应裁决。
3. `AGENTS.md` 的命令表（`just adversary` 一行）。
4. `.github/workflows/adversary.yml`——定时任务，**永远不进 `check`**。

---

*本文档采用 MPL-2.0。*


---

## 附：两个新思路，以及性质自身的一个洞

**`Kusanagi.Cairn`——读取方开始有记忆之后，仍然必须被告知全部。** 端点现在会记下验证到哪，好让轮询只向主机点名一个地址。这个形状的修法出错的方式是**读得比报得少**：续读起点偏高会交回一份短列表和一个正确的高度，而任何「某条消息到达了」的断言都照样通过。两条性质都是同一次运行中两条轨迹之间的关系：水位恰好隐藏它所指的那些条目、不多不少；第二次读取不用第一次写下的记忆去抵扣自己交回的内容。它不碰站点目录——记忆是不是文件、放在哪、叫什么，这个模块一旦知道就不再测门而开始测实现。

**`Kusanagi.Lying`——主机不伪造任何东西时还能说的两种谎。** `Model` 已经翻过一个字节，那是主机因损坏而说的谎。这两种是它在所持每个字节都真实、都由端点亲手签名的前提下说的，因此任何签名检查都看不见：

- **移植**：把一个地址上的对象拿到另一个地址上服务。位置是唯一的谎。本网络用密钥而非检查来回答它——地址推导出其内容所用的密钥——所以这条性质真正问的是那个推导是否承重，它会在有人把密钥改成依赖段本身的那天亮红灯。答案落在 `seal.rejected`，比验证低一层。
- **消失**：停止服务某个对象。谁也拦不住。不许发生的是读者相信**比它已经验证过的更少**，因为「她从没发过那条撤回」是一次消失就能说的谎。

**消失这条第一次运行就红了**：主机删掉一个对象，把已验证到高度 1 的读者拉回高度 0。修法是 `kusanagi::walk::confirm`，交付物是 `crates/kusanagi/tests/lying.rs`。

**洞也可能在性质里。** `adviceIsAboutWhatWasGiven` 用「参数里有没有 `kusanagi2:` 字面量」判断调用者是否给过邀请。对抗测试把一份完好邀请的首字母 `k` 打掉，于是这条规则断定没有邀请被给出，从而把**正确的**建议判为缺陷。判据改为调用者所站的位置：`join` 是唯一以邀请为位置参数的动词。**一份被打残的邀请仍然是一份被给出的邀请**，而这条记录留在这里，是因为下一个把性质写严的人会犯同一个错。
