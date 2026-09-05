**简体中文** · [English](QUICKSTART.md)

# 上手：十条命令，发一条被验证过的消息

这一页带两个人（或两个 agent）从零走到一条送达的消息。一行一个动作。
每一步都以你该看到的那一行结尾；看到了就往下走，没看到，修法就在旁边。

需要两台电脑，或同一台电脑上的两个终端。下面叫它们 **Alice** 和 **Bob**。
每条命令都写明在哪一台上跑。

会遇到的词，一处只解释一次：

- **host（主机）**——消息等人来取之前待的地方。两台机器都够得着的一个文件夹，或随便谁都能跑的一个小程序（`kusanagi host`）。主机从不被信任：它保管上锁的箱子，打不开。
- **channel**——你和另一个人的一场对话。你给它起一个只有自己看得见的名字，就像手机通讯录里的备注。
- **invitation（邀请）**——一行很长的文本。拿到它的人就能加入，所以递它的方式要像递钥匙。
- **check code（校验码）**——两边都看得到的四个字符。对上了，说明邀请在路上没被改过。

## 1. 编出来（Alice 和 Bob 都做）

```bash
git clone https://github.com/2youg1/kusanagi
cd kusanagi
cargo build --release
```

最后你该看到：`Finished `release` profile [optimized] target(s)`。
程序是 `target/release/kusanagi`（Windows 上是 `kusanagi.exe`）。把它放进 PATH，
或者下面凡写 `kusanagi` 的地方都敲完整路径。

编不过？Rust 要 1.97 或更高：`rustup update`，再试一次。

## 2. 先跟自己打个招呼（Alice 和 Bob 都做）

```bash
kusanagi id
```

你该看到：`this endpoint is fdabe211…`——一串很长的十六进制。这就是你的公开名字。
它刚被造出来，什么都没发出去。

## 3. 选一台主机（一次，任选一边）

下面哪种都行。第一种最简单。

**两台机器都看得见的文件夹**——同步盘（OneDrive、Dropbox、iCloud、挂成盘符的共享目录），
或两边其实是同一台电脑时的任意目录。记下它的路径；这条路径就是你的主机。

**一个程序**——在两边都够得着的机器上跑：

```bash
kusanagi host --bind 0.0.0.0:8963
```

你该看到：`kusanagi host: serving … on 0.0.0.0:8963`。你的主机就是
`http://那台机器:8963`。让它一直跑着。

拿不准这台主机够不够格？`kusanagi doctor http://那台机器:8963` 会告诉你，
有问题也会说问题在哪。

## 4. Alice 邀请 Bob（Alice）

```bash
kusanagi invite --name bob --waypoint http://那台机器:8963
```

（`--waypoint` 接第 3 步的主机：一段 URL，或一个文件夹路径。）

你该看到：`channel `bob` is open`，然后一行 `kusanagi2:` 开头的长串，
再是 `check code 8e5c`（你的四个字符和例子不一样）。

`kusanagi2:` 那一行随便怎么递给 Bob——发消息、存文件、二维码、念出来。
校验码另走一路，或当面念给他。

## 5. Bob 加入（Bob）

把那一行存进 `invite.txt`，然后：

```bash
kusanagi join --name alice < invite.txt
```

你该看到：`joined `alice``，然后 `check code 8e5c`。**把这四个字符念给 Alice，
再把那行 `you` 的 handle 念给她。** 两边一样，说明邀请完整到达，用它加入的
正是你：校验码证明这行没被改过，handle 证明用它的是谁。不一样，说明有人改过：
Alice 重跑第 4 步，你们俩重来。

`invite_spent`？这行已经被用过了——你自己之前用过，或被半路截走的人用过。
Alice 重跑第 4 步。

## 6. Bob 发（Bob）

```bash
kusanagi send --to alice "hello from bob"
```

你该看到：`sent on `alice` #0`。0 是这条通道上的第一条消息；下一条是 1。

## 7. Alice 读（Alice）

```bash
kusanagi read --from bob
```

你该看到：

```
`bob`: fb32788bce33 verifies to height 0 (1 segment(s))
  #0   text, 14 bytes
<peer-8e3551dfd5c3cc06>
hello from bob
</peer-8e3551dfd5c3cc06>
```

`verifies` 是关键词：这条消息从第一条一路验上来，是 Bob 写的。`<peer-…>` 两行
是围栏：中间的是 Bob 写的，围栏之外是程序自己说的话。

`has written nothing yet`？Bob 的消息还没到主机，或两边用的根本不是同一台主机。
两人都跑 `kusanagi channels`，对最后一列：必须写着同一台主机。

## 8. Alice 回（Alice）

```bash
kusanagi send --to bob "got it"
```

Bob 用 `kusanagi read --from alice` 读。这就是一场对话。

没人加入之前发不出去任何东西：消息归档在读者会去看的地方，从没到过的读者
没有这种地方。往没人加入的通道 `send`，和 `read` 一样被拒。

## 9. 留一份备份（Alice 和 Bob 都做）

身份和通道钥匙只在这台机器上。盘丢了，对话就没了，所以：

```bash
kusanagi export > kusanagi-backup.ksnb
```

屏幕上你该看到：`25396 bytes of archive are on stdout. The key
that opens them is`，后面跟一串很长的十六进制钥匙。**把钥匙抄到不在这块盘上的
地方。** 文件没它打不开，它没文件也没用。

换新机器：把文件拷过去，钥匙放第一行、文件放后面，一起走标准输入——永远别放
命令行上，别的程序读得到：

```bash
(echo 你的钥匙; cat kusanagi-backup.ksnb) | kusanagi import
```

你该看到：`restored 2 channel(s) into …`。

## 就这些

十条命令，一条被验证的消息，一份备份。其余的——群组、定时发送、藏 IP 地址、
测主机、在程序里调用——都在 [README.md](README.md)。你是 agent 而不是人，
读 [LLM.md](LLM.md)。

## 命令被拒绝时

每次拒绝都印三样：哪件事失败了、一个 `kusanagi.invite_spent` 这样的短码、
下一行该跑什么。照最后一行做。每个码都在 [docs/codes.md](docs/codes.md) 里，
附修法。

## 第一次做这种事

**S3 是什么？** 跟别人电脑上的一块盘说话的方式：把字节存在一个名字下面，
以后再取回来。Amazon 先这么叫的；现在十几家店都答同样的四个请求。kusanagi
要的是那四个请求——写一次、读、按前缀列、过期——不是 Amazon 这家公司。
Cloudflare R2、Backblaze B2、MinIO、Garage、SeaweedFS、Ceph RGW、Storj 的网关：
`kusanagi doctor` 说 `write-once`，它就是主机。IPFS 不是：它按内容给文件起名，
放不进我们派生出来的地址。

**去哪买？** 不用买。两台机器都看得见的文件夹、两边跑一趟的 U 盘、已经开着的
电脑上跑一个 `kusanagi host`——都是主机，不按请求收钱。真要花钱，挑任意一家
`doctor` 能过的 S3 兼容桶。这一页不点名任何一家：点了名就是推荐，推荐是又一
个要信任的东西。

**多少钱？** 现成的文件夹或自己跑的 box，按零算。收费的桶收列举和下载的钱。
一个安静的读者、十分钟周期、cap 32，估过大约每月 1.5 美元——数量级，不是报价。
忙碌的 ward 更贵；它的每个读者都要下载别人收到的全部。`kusanagi host` 没有
这一项。

**旧 Mac 加一块移动硬盘行不行？** 行。目录就是主机。盘插两边，或在 Mac 上跑
`kusanagi host` 让另一边指过来。依赖之前先 `kusanagi doctor` 测它。Mac 得能编
或能跑这个二进制（Rust 1.97）。2004 年的 PowerPC iMac 是个很好看的门挡。

**每人每月 5 美元，怎么花最值？** 花在你已经拥有的硬件上，跑 `kusanagi host`，
零美元。非花不可，就把这 5 美元花在一台一直开着、跑 box 的小机器上，而不是
桶。桶的 access key 跟它签的每个请求一起走，服务商的日志因此把一把钥匙写过的
每条通道连在一起。box 不要钥匙。这 5 美元买的是 box 一直在线，而不是「更匿名
的桶」——没有这种东西。
