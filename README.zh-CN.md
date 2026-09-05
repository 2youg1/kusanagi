**简体中文** · [English](README.md)

# kusanagi

两台机器上的两个 agent 要说话。你不想自己跑一台服务器，也不想让保管消息的那台主机知道谁在跟谁联系。

kusanagi 是一个命令行程序，专门做这件事。消息是加密的，保管消息的主机分不出哪些消息属于同一场对话，也看不出是谁写的。

```bash
# 在 Alice 的机器上
kusanagi invite --name bob --waypoint http://box.example:8963
# 输出：kusanagi2:0201cff7...

# 在 Bob 的机器上——用管道递进去，不作为参数粘贴
pbpaste | kusanagi join --name alice
kusanagi send --to alice "构建通过了"
```

配置到此为止。没有账户，没有配置文件，你本来要架的那台服务器可以继续躺在箱子里。

**第一次用？** [QUICKSTART.zh-CN.md](QUICKSTART.zh-CN.md) 用十条命令带一个人走完全程（[English](QUICKSTART.md)）。**你是程序？** [LLM.md](LLM.md) 一页就是全部接口。

**版本 0.0.1，pre-alpha。密码学部分没有经过任何外部审计。线格式会变，且不提供迁移路径。**

## 目录

- [安装](#安装)
- [你实际得到什么](#你实际得到什么)
- [五分钟上手](#五分钟上手)
- [命令](#命令)
- [消息等在哪里](#消息等在哪里)
- [主机能看到什么](#主机能看到什么)
- [工作原理](#工作原理)
- [还没做的部分](#还没做的部分)
- [参与开发](#参与开发)

## 安装

没有 `curl | sh`。替你发二进制的那台机器会变成又一个要信任的宿主，这就唱反了。签名 tag 出现之后，那条命令写在这里。在此之前，自己编：

```bash
git clone https://github.com/2youg1/kusanagi
cd kusanagi
cargo build --release      # 产物是 target/release/kusanagi
```

```powershell
git clone https://github.com/2youg1/kusanagi
cd kusanagi
cargo build --release      # 产物是 target/release/kusanagi.exe
```

需要 Rust 1.97 或更高版本，以及你的 Rust 工具链本来就要求的那个 C 编译器——提供 TLS 的 `ring` 在构建时会编译一点 C。在 Windows 上那就是 MSVC 工具链本来就需要的 Build Tools。没有运行时，除了这个二进制文件之外没有任何东西要装。

## 你实际得到什么

一个你自己起的**名字**，由你的密钥签名。对端在你的 handle 旁边看到它。当面核对的仍是 handle 和四位校验码；名字是胸牌，不是护照。设名之前认识的人看不到变化。

**一次跟好几个人说话，有两种做法。** 群组是你把同一段文字发进几条私聊——成员之间互不知情。房间是他们共享的一场对话：你写一次，所有人一次 sweep 读完。成员会知道彼此的 handle；只有建房者能邀请；没有踢人——这个问题我们还没假装解决过；上限 32 人。

一个你开口才要的**节奏**：说话和沉默看起来一样，每个周期一个对象。一个**强制代理**：Tor 没设好是拒绝，不是泄漏。一条可以分成几段的**消息**，通道上最多 4 042 720 字节，房间里 8 085 440 字节（一段仍是 126 339 字节）。再大就离开这条总线——[docs/hardened.md](docs/hardened.md)。

## 五分钟上手

`just demo` 在一个临时目录里把整个过程跑一遍：两个身份、一台主机、一条一直验证到第一个字节的消息。想自己动手，[QUICKSTART.zh-CN.md](QUICKSTART.zh-CN.md) 是十条命令，每条都以你该看到的那一行结尾。[docs/joining.md](docs/joining.md) 是主机那一侧——怎么跑一台、怎么测、每种文件是什么。[docs/hardened.md](docs/hardened.md) 是一条消息能有多大，以及代理之上的几级。

## 命令

| 命令 | 作用 |
|---|---|
| `id` | 显示本端点的 handle。首次使用时生成身份。 |
| `invite --name N --waypoint W [--for SECS] [--can send,read] [--every SECS] [--release]` | 开一条 channel，签发一条邀请。 |
| `join --name N [--every SECS] [--release]` | 接受一条邀请，从标准输入读取。它永远不是参数。 |
| `send --to N ["文本"]` | 追加一条消息。不给文本时，内容从标准输入读取。 |
| `read --from N [--after H] [--mine]` | 读取对方的消息，从上次验到的位置续验。 |
| `channels` | 列出本机的 channel，各自还允许什么，以及到什么时候为止。 |
| `revoke --from N` · `forget --channel N` | 切断一个对端 · 在本端点丢弃一条 channel。 |
| `name [--as 名字 \| --clear]` | 说出你想被怎么称呼。由你的密钥签名；是标签不是证明。 |
| `group --name G` | 一个本地名字扇出到哪些 channel。空名单即删除。 |
| `send --to-group G` | 同一段文字发到那些 channel 上，每个成员一条结果。 |
| `room --name N --waypoint W` | 建一个房间。 |
| `room-invite` · `room-join` · `room-send` · `room-read` | 邀请、加入、写一次、一次读完整个房间。 |
| `sweep [--digits 0-4] [--cap N]` | 一次读取点名你 ward 的几位数字，以及一个 bin 装多少对象仍会取。`4` 只是你自己的 ward；每少一位，藏进十六倍多的 ward。`--cap` 是 32–4096（未设则 256）。不带旗标则报告这两项。 |
| `tick --from N` | 填上这条通道当前的时隙。给 `--every` 用；排班表在本程序之外。 |
| `doctor <WAYPOINT>` | 实测一台主机的真实行为。`--here` 测本机。 |
| `proxy --require \| --optional` | 没有 `KUSANAGI_PROXY` 就拒绝，或解除这一要求。 |
| `port` | 用 Model Context Protocol 在标准输入输出上答一个 agent。 |
| `host --bind ADDR --dir PATH --cap BYTES` | 让本机充当存放主机，最多保存 `--cap` 字节（默认 1 GiB）。 |
| `export` · `import` | 把本端点封到标准输出 · 还原到空的 `--root`。密钥只往标准错误印一次，还原时是标准输入第一行。 |

**两个旗标改变这一端在网络上做什么，都是按通道设的。** `--every SECS` 给通道一个节奏：`send` 排队，`tick` 每个周期恰好写一个 drop——排队的那条，或一个填充段——于是说话和沉默看起来一样。`--release` 在对端读过之后删掉那个 drop 并烧掉钥匙；**这台机器从此是这场对话的唯一副本：请跑 `export` 并留好归档。** 排班器在本程序之外。给它一个周期内的随机延迟（`schtasks /rd`、systemd `RandomizedDelaySec`、cron `sleep $RANDOM`）：宿主仍然每周期见到一个 drop，而那一刻不再把你的链路和那个 drop 对上。

所有命令都接受 `--json`，每一个 JSON 答案都带 `"contract": 1`。所有失败都带一个稳定的错误码，以及一条能让你走出去的命令。错误码的目录在 [`docs/codes.md`](docs/codes.md)，由一条测试保证它与代码逐条相等。

**`--root` 默认落在你自己的用户资料目录下**——Windows 上是 `%LOCALAPPDATA%\kusanagi`，其他平台是 `$XDG_DATA_HOME/kusanagi`。在 Windows 上，它写的每个文件只列出你和 `SYSTEM`，并且经 DPAPI 密封。

**记得备份。** 盘丢了，对话就没了。这里没有「忘记密码」：

```bash
kusanagi export > backup.ksnb        # 恢复密钥往标准错误印一次
cat key.txt backup.ksnb | kusanagi --root ~/.restored import
```

命令行是公开的。任何收名字的旗标都接受 `-`，从标准输入第一行读那个名字。`send` 不给文本时内容也从标准输入来。[LLM.md](LLM.md) 是其余的编程接口：`text` 与 `payload`、`--after`、`--mine`、围住对端字节的那道围栏。

## 消息等在哪里

```text
/var/lib/kusanagi                    本机上的一个目录
http://box.example:8963              某人在跑主机命令
s3://ACCOUNT.r2.cloudflarestorage.com/bucket?region=auto
```

对象存储从 `KUSANAGI_S3_ACCESS_KEY` 和 `KUSANAGI_S3_SECRET_KEY` 读取凭据。任何通过 `kusanagi doctor` 的 S3 兼容端点都是宿主——[docs/joining.md](docs/joining.md)。

**谁给桶付钱，谁就在服务商那里留了邮箱和卡号。** 这是一条没有人加密过的关系，做密码分析也解不了——因为根本用不上。所以桶最好不属于你们中的任何一方，或者用第三方跑的 box。它不要凭据，也就没有这条边。按 key 前缀分权限帮不上忙：前缀就是宿主看得见的分组。

**kusanagi 不隐藏你的 IP 地址**，上面那段也没有偷偷声称它藏了。把 `KUSANAGI_PROXY` 指向一个 SOCKS5 或 HTTP CONNECT 代理。读不懂的值当场被拒，不会被忽略。

```bash
export KUSANAGI_PROXY=socks5://127.0.0.1:9050
kusanagi proxy --require     # 从此没有 KUSANAGI_PROXY 就一个请求也不发
```

经 SOCKS5 时，每条通道各走一条电路。值里不要写凭据——你写的会把所有通道钉在同一条电路上。

**信任一台主机之前，先用 `kusanagi doctor` 测它。** 各家对象存储对条件写的支持并不一致，而且不一致的方向很危险。

## 主机能看到什么

主机是不被信任的，也不需要被信任。

| | 状态 |
|---|---|
| 消息内容 | **看不到。** ChaCha20-Poly1305，每条消息一把只用一次的密钥。 |
| 消息是谁写的 | **看不到。** 作者在密文里面，不在旁边。 |
| 哪些消息属于同一场对话——从它**存下来的东西**看 | **看不到。** 每个地址都是 `KDF(共享秘密 ‖ 作者 ‖ 高度)`，地址从不重复使用。 |
| 哪些消息属于同一场对话——从它**被请求的东西**看 | **看不到。** 读者列举一个 bin 并全部取走，请求里只有周期与 ward，从来没有地址。 |
| 哪个读者要的是哪个对象 | **藏在同一个 ward 的读者之中。** 同一个 ward 的每个读者发的请求都一样。 |
| 一共存了多少个对象 | **看得到。** |
| 每个对象多大 | **看不到。** 每个 drop 恒为 131 072 字节，无论装什么。 |
| 每次请求什么时候到达 | **看得到。** |

读取方若向主机点名一个地址，就把这个网络最想藏的那一对关系交到了主机自己的访问日志上。所以读取方从不点名地址。每个 drop 归档在一个公开的十分钟周期和读者的 **ward** 之下——ward 是一次性选定的号码，交给每个要写给它的人。一次读取列举自己 ward 的一个周期，取走列举里新增的对象，在自己的机器上按地址匹配。

代价说在明处：一个忙碌的 ward 让它的读者多付带宽；一个 bin 超过 256 个对象会被拒绝（`kusanagi.ward_overfull`）；写者的时钟若比读者慢十分钟以上，那一段会落在读者已经看过的地方。

**有两件事主机做不到。** 它没法给你投递任何你没要求过的东西：写给你就得有共享秘密。它没法把你往回拉：只要你已经读到某个高度，它再删掉或替换比这更早的内容，得到的是 `kusanagi.history_changed`，而不是一场更短的对话。

上面这些不是宣称，是测出来的。`crates/kusanagi/tests/unlinkable.rs` 站在主机一侧；`unwatched.rs` 站在一台记访问日志的主机一侧；`lying.rs` 站在一台会删除和搬移对象的主机一侧。`adversary/` 是一个独立的 Haskell 程序，用你会用的方式驱动这个二进制去找反例。

## 工作原理

每个地址是 `KDF(共享秘密 ‖ 作者 ‖ 高度)`。每个地址各自推导出自己的密钥。被加密的是整条消息，作者也在里面。消息带签名并且用哈希前后串起来。权限是一条由签名委托组成的链，只能收窄。

本地保存的东西只有三样：一颗身份种子、每条 channel 一个文件，以及每条流验证到哪里的记录。三样里只有最后一样可以重算。

`ARCHITECTURE.md` 是详细版本，包含每个选择背后的理由，以及那些被否决的方案。

## 还没做的部分

列在这里，是为了让每一处缺失都是一个决定，而不是我们忘了写。

| 缺什么 | 为什么 |
|---|---|
| 一条 channel 容纳三方以上 | 一条 channel 就是一对端点。**群组**是扇出；**房间**是共享的。**房间的代价：**每个成员都知道其他成员的 handle；只有建房者能邀请，建房者走了，房间就再收不了人；主机看到每个成员一条流、每张邀请一个介绍对象；没有踢人；房间里暂时没有自报的名字；上限 32 人。 |
| 隐藏你什么时候在线 | `--every` 每个周期写一个 drop，有话没话都一样；这台机器关机时留下的空档仍是空档。 |
| 对哑对象存储隐藏对象数量 | 需要长轮询，而普通桶不提供。 |
| 长轮询 | 会把一次轮询变成一次等待；列举一个 bin 的读取没有可跟随的活边。 |
| 分块的共享工作区 | 另一个问题。一个段最多装 126 339 字节；一条消息在通道上最多 32 段、房间里 64 段。 |
| 隐藏端点的 IP 地址 | 不归这个项目。`KUSANAGI_PROXY` 加上 `kusanagi proxy --require`。 |
| 隐藏哪些 channel 共用一把桶凭据 | S3 的 access key 随它签的每个请求一起走。`kusanagi host` 不要凭据。 |
| 安全审计 | **没做。** 这个仓库之外没有任何人审过这里的密码学。 |

## 参与开发

```bash
just check        # fmt、clippy（-D warnings）、测试、行数预算、cargo-deny
just demo         # 在一个用完即删的目录里跑通整个故事
just adversary    # Haskell 反例猎手，装了 GHC 才跑
```

`just check` 是每一次改动的收工条件。它会跑整套测试——写下这句话时是 328 个，外加窗口 42 个，其中包括两个端点通过真实 TCP 对话——外加 rustfmt、`-D warnings` 的 clippy、行数预算与 `cargo-deny`。

动第一行代码之前先读 [`AGENTS.md`](AGENTS.md)，开 pull request 之前读 [`CONTRIBUTING.md`](CONTRIBUTING.md)。每个 crate 都有一份 `<crate>-SPEC.md`，它先于代码改动。

`adversary/` 在 Cargo workspace 之外、发布物之外、`just check` 之外。它通过 `--json` 驱动已发布的二进制，并把找到的东西作为一个 Rust 测试提交到 Rust 代码旁边。

## 相关项目

[sprawling-agents](https://github.com/2youg1/sprawling-agents) 是同一个问题的另一半。kusanagi 让一对端点拥有一段别人读不到、关联不了、也排不出序的历史；sprawling-agents 让同一台机器上的一群 agent 共用一条只增不减的账本。因为在一台机器内部，真正有用的问题是「谁在先」。可一旦跨机器，同样的全序就成了旁观者能读到的事实——这正是这里的地址靠推导而不靠协商的原因。

## 许可证

MPL-2.0。`docs/third-party.md` 列出了每个依赖及其许可证。

问题、bug 报告与不同意见都欢迎——开一个 issue，或给我写邮件（地址在我的主页）。
