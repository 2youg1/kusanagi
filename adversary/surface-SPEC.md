# surface-SPEC

> `adversary/` 的第二份 SPEC：**攻击面矩阵**。`adversary-SPEC.md` 管这个目录是什么、怎么跑、
> 为什么在仓外；本文只管一件事——对手站在哪、拿得到什么、必须学不到什么，一格一条性质。
> 权威顺序不变：用户裁决 → `ARCHITECTURE.md` §3/§8 → `adversary-SPEC.md` → 本文 → 代码。
> 用户给的门槛：「金融机构都愿意用其做内部交流而不担心任何可能的危险」。

## 1 需求拆解：矩阵

每格一条**关系**，不预测任何输出；每格写明**哪一种改动会让它红**（`adversary-SPEC.md` §2 第 5 条）。
列「今日」是本轮开工前用二进制探过的事实：红 = 已经复现的缺陷，绿 = 已成立，? = 未探。

### 宿主（持有全部对象，可以撒谎）— `Forging.hs`、`Leakage.hs`、`Reach.hs`

| # | 性质 | 会让它红的改动 | 今日 |
|---|---|---|---|
| H1 | 全部对象里 grep 不到任一正文、通道名、任一 handle、邀请秘密、恢复密钥 | 密封漏了任一字段 | ? |
| H2 | 一个身份两条通道，全部对象两两 `Veil.apart` | 地址或密钥派生少混了通道秘密 | ? |
| H3 | 一个身份两台宿主，地址无共同前缀、正文两两 `apart` | 同上，或 locator 进了派生 | ? |
| H4 | 同一句群发给两个成员，两个 drop `apart` | 群发复用了密文 | ? |
| H5 | 首字节、中间字节、**尾字节（pad 区）**各翻一位 → 拒 | pad 不再校验；AEAD 标签没覆盖全体 | ? |
| H6 | 截短一字节、加长一字节、全零、空文件、等长随机 → 拒或止于其下，从不当段落报出 | 读侧不再核尺寸 | ? |
| H7 | Bob 的 drop 放到 Alice 的下一地址 → 从不作为 Alice 的话报出 | 密钥派生不再含作者，或 Trail 检查退化 | ? |
| H8 | 通道二的 drop 放到通道一的地址 → 拒 | 密钥派生不再含通道秘密 | ? |
| H9 | 中间一段消失：**无记忆的读者**止于缺口之下；**有记忆的读者**不低于自己的水位 | 链接检查退化；cairn 不再是下限 | ? |
| H10 | 相邻两址内容互换 → 拒或止于其下 | 地址不再进密钥 | ? |
| H11 | 宿主多放 100 个等长随机对象 → `read` 的 JSON 逐字节不变 | 读者开始列目录（法则 1） | ? |
| H12 | 宿主先在 Alice 的下一地址放对象 → Alice 的 `send` 带码拒绝或落在别处；Bob 永不读到垃圾 | `put_if_absent` 退化为覆盖 | ? |
| H13 | Alice 读过 Bob 后，宿主删 Bob 的接受 drop，Mallory 用同一邀请 `join` → Alice 的对端仍是 Bob；Mallory 的段永不出现 | 对端不再钉在首见 | ? |
| H14 | `--release` 通道：先拷贝 drop；对端确认后宿主已删；把拷贝放回原址 → 读者永不再报它 | 棘轮不烧旧键 | **红后已修**：受邀者记录的 retention 曾是自己的默认 `keep`，两端密钥表不同，`--release` 通道根本读不通；offer 现在携带 retention（版本 2） |
| H15 | 宿主接受连接永不应答 → 带码拒绝，**总时长有上界**（`PATIENCE` 一分钟 + 余量） | 全局超时被拆成 idle 超时 | 绿（实测 60 s 内） |
| H16 | 宣称 999 999 字节只给 2 字节；纯垃圾字节 → 带码拒绝，退出码仍 ∈ {0,1} | 解析 panic | ? |
| H17 | `302` 指向第二个监听器 → 第二个监听器**零连接**，带码拒绝 | 客户端跟随重定向 | 绿 |
| H18 | 对象数 = 段数 + 每通道一个 offer——**诚实边界**，§3 写明，不断言 | — | — |
| H19 | 宿主从备份回滚（作者自己的流变短）→ 作者的 `send` 与读者的 `read` **同一个码**拒绝，绝不写出一段链到已消失前驱的段落 | `send` 不再确认前驱在宿主上 | 已修：`track` 在 `Reach::Head` 上 `peek` 记录的头 |
| H20 | 宿主往读者的 bin 里多放两个陌生对象 → 读者的 GET 集合 == 该 bin 全部对象（含陌生人），报告只有自己的三段 | 读者只取自己的那几个，等于向宿主指认它们 | 绿（`Sweep.hs`，经 Relay 取证） |
| H21 | 邀请、三次 send、两次 read 期间，除 period 0 的 rendezvous 外，无请求点名列举之外的地址；列举只含 period 与 ward；无 DELETE | 某个动词绕开 sweep 直接按地址取，或释放时删除 | 绿（`Sweep.hs`） |

### 路径（看得见时刻与请求头，看不见字节）— `Reach.hs`；时序已在 `Tempo.hs`

| # | 性质 | 会让它红的改动 | 今日 |
|---|---|---|---|
| T1 | 监听器收到的每个请求头集合 ⊆ `{host, content-length, content-type, accept, if-none-match, cache-control}`，无 `user-agent`，任一头名或值不含 `kusanagi` | 加了一个自报家门的头 | 绿（探针见 §4） |
| L1 | **locator 永不指向网络路径**：`\\host\share`、`//host/share` 在 `invite` 与 `join` 两处都以**与未知 scheme 同一个码**拒绝，而不是去连。SMB 不走代理，一条 UNC 邀请就让受邀者的 Windows 向邀请者指定的主机做 NTLM 认证 | locator 解析放行 UNC | **红**：`\\127.0.0.1\…` 被接受，`//127.0.0.1/…` 报 `os error 53`——两者都已尝试连接 |

### 代理的正门：MCP `port` — `Port.hs`

| # | 性质 | 会让它红的改动 | 今日 |
|---|---|---|---|
| P1 | `kusanagi_read` 的工具结果：`content[].text` 里对端字节在**恰好一对 nonce 围栏**内，围栏外文本只是长度的函数，无控制字节；`structuredContent` 是 `--json` 的同一份 `Outcome` | 工具结果回到裸 JSON | 已修：`content` 散文带围栏，`structuredContent` = `--json` |
| P2 | `tools/list` 的每个 `kusanagi_<verb>` 对应一个 `kusanagi <verb> --help` 退出码 0 的动词；被拒的调用是 `isError: true` 且 `structuredContent.code` 非空 | 两扇门的动词集分叉 | ? |

### 同机他人／拿到磁盘的人 — `Leakage.hs`

| # | 性质 | 会让它红的改动 | 今日 |
|---|---|---|---|
| S1 | 站点根下所有字节 grep 不到任一正文（**所有平台**） | 记录开始存最后一段 | ? |
| S2 | 站点字节 grep 不到通道名、任一 handle、邀请秘密（Windows；其他平台走 §3 边界） | DPAPI 漏斗少了一个文件 | ? |
| S3 | 根下**任何路径分量**不含通道名、不含任一 handle 的任意 8 位子串 | 文件名回到明文 | 已修：`naming::filed_author` |
| S4 | 同一对端的两条通道 → 两个通道目录下**无同名文件** | 同 S3 | 已修 |
| S5 | 两个站点同一对端 → 两站点除固定名（`identity` 等）外无同名文件 | 同 S3 | 已修 |
| S6 | 归档 grep 不到正文/通道名/handle/恢复密钥/邀请秘密 | 归档漏封 | ? |
| S7 | 错密钥 → 拒且根仍空；归档翻一位 → 拒；非空根 → 拒 | 导入不再原子 | ? |
| S8 | 导入到新根后 `read` 的 JSON == 导出前 | 归档漏了 cairn 或记录版本 | ? |
| S9 | `forget` 后该通道目录消失、根下文件数减少、`read` 带码拒；宿主对象数不变 | forget 删了宿主对象（法则：宿主的删除是卫生，不是保证） | ? |
| S10 | 任意动词序列后宿主 `.staging` 为空、站点根下无临时名 | 原子替换退化 | ? |

### 对端（持有通道秘密，可以恶意）— `Terminal.hs`

| # | 性质 | 会让它红的改动 | 今日 |
|---|---|---|---|
| M1 | 等长的良性正文与恶意正文（伪闭合围栏、伪元数据行、CR、CSI、OSC 52、RTL 覆盖、C1）→ 归一化 nonce 后**围栏外文本逐字节相同** | 程序在围栏外说了依赖内容的话 | ? |
| M2 | 散文输出除 `\n` `\t` 外**无 C0、无 DEL、无 C1**，无论对端发什么 | 文本分类只看 UTF-8 | 已修：`Carried::of` 判终端代码为 `Payload` |
| M3 | 64 KiB 与 100 KiB 正文：要么带码拒，要么逐字节回读——**永不静默截断** | 某层截断 | ? |
| M4 | 整条轨迹所有输出里，邀请秘密只在 `invited` 出现一次 | 某个报告回显了秘密 | ? |

### 对端，对着窗口（H8，D-18）— `Glass.hs`

窗口把对端字节渲染成 markdown；D-18 裁定渲染永不引发 I/O。这里不信「按构造如此」：本机监听端口数连接、automation server 报告画了什么、会话后读回磁盘与剪贴板。窗口未构建（`native build -Dautomation=true -Dtrace=off`）或 `native` 不在 PATH 时整组答「skipped」——CI 从不构建 GUI（Roadmap 事实 21），这是出货机器的门，不是合并机器的门。

| # | 性质 | 会让它红的改动 | 今日 |
|---|---|---|---|
| W1 | 正文含对端命名的远程图片与链接 → 监听端口**零连接**，图片只画 alt 文本 | `<markdown>` 给了 `images` | 绿 |
| W2 | `http:`/`javascript:`/`file:` 三种链接的控件都**没有 `press` 动作**；按下也零连接、无 `error event` | 绑了 `on-link` | 绿 |
| W3 | OSC 52 / CSI 2J / 裸 CR → 控件树里无 ESC、无 CR，正文以十六进制显示 | glass 自己解析 payload 或 CLI 的 `Carried::of` 放宽 | 绿 |
| W4 | 会话（含在窗口里铸一张邀请）后，站点之外的盘上只有清单里的文件（两个偏好、`windows.zon`、有 trace 时的 `native-sdk.jsonl`），且 grep 不到正文、`kusanagi2:`、宿主路径 | 窗口或 SDK 多写一个文件 | 绿；查出 SDK 默认写每帧事件日志（使用时间线），发布构建改 `-Dtrace=off` |
| W5 | 铸出邀请后剪贴板仍是哨兵；按下「复制邀请」后才是 `kusanagi2:…`，且窗口说明剪贴板是日志 | 自动复制，或复制不说明 | 绿；查出复制后**无 B4 警告**，已加说明 + 60 s 回收（`scrub`） |

### 群组内鬼与前对端 — `Insider.hs`

| # | 性质 | 会让它红的改动 | 今日 |
|---|---|---|---|
| G1 | Bob 交出站点全部字节 + 所有可读对象 → grep 不到 Mallory 的 handle 与通道名 | 群组变成共享名册 | ? |
| G2 | 群发段与私发段的 JSON 键集合相等 | 段上带了群标记 | ? |
| G3 | 撤销 Bob 后 `send --to-group`：Bob 的 `Landed` 为拒，Mallory 送达；Bob 的 `read` 看不到新话 | — | 已修：`appended` 问 peer 的 standing |
| X2 | 撤销后对该通道 `send` → `grant.revoked`，`recover` 指向 `forget` | 撤销只管读不管写 | 已修 |

### 持有某件东西的人 — `Twins.hs`

| # | 性质 | 会让它红的改动 | 今日 |
|---|---|---|---|
| A7 | 同一归档导入两个根，两边各 `send` → 至多一个成功，另一方带码拒；读者每高度恰一段 | 写入不再 `put_if_absent` | ? |
| K1 | 同一站点并发 8 个 `send` → 读者的链无缺口无重复；每个拒绝带码 | 记录替换不原子 | ? |

### 扫描者（无地址，对着真宿主发原始 HTTP）— `Scanner.hs`

| # | 性质 | 会让它红的改动 | 今日 |
|---|---|---|---|
| N1 | `/`、`/robots.txt`、`/.well-known/x`、`/d/`、`/d/zz`、`OPTIONS /` → 状态行与体逐字节相同，无 `server`/产品头 | 加了 banner | ? |
| B1 | 超尺寸与欠尺寸 `PUT` → 拒，宿主目录无新文件 | 尺寸检查后置 | ? |
| B2 | 同址二写 → 第二次拒，内容仍是第一次 | write-once 退化 | ? |
| B3 | `/d/../../x`、`/d/%2e%2e/x` → 404，宿主目录外无新文件 | 路径拼接 | ? |
| B4 | 设 `KUSANAGI_PROXY` 指向监听器 → 宿主监听器**零直连**；代理死 → 零直连且带码拒 | 任一请求绕过代理（`doctor` 最可疑） | 绿（探针） |

## 2 验收标准

1. 每格一条 tasty 用例，名字用人话陈述关系；全部绿。
2. 标「红」的四处（S3–S5、M2、G3/X2）**先跑红再修 Rust**，修法写进对应 crate 的 SPEC。
3. 新模块每个 ≤400 行；`test/Main.hs` 拆出 `test/Surface.hs`。
4. 总时长：黑洞一条占一分钟，与其余并发；其余全部秒级。

## 3 假设与边界（诚实的残余）

- **对象数**（H18）与**归档尺寸**随通道数变化：宿主与拿到归档的人各学到一个计数。
- S2 在无 DPAPI 的平台上不成立：记录是明文，全盘加密是前提（D-04）。测试按 `System.Info.os` 选断言。
- 撤销不能收回对方**已经**持有的秘密：被撤者仍能解开撤销前的历史（X2 只断言撤销**后**的话不再送出）。
- 释放（H14）只烧钥匙，不再删除（D-20：DELETE 会点名地址）；宿主上字节的清除对两种 retention 都由宿主的生命周期决定。
- 同 ward 读者的匿名集是黑盒测不到的量（ward 由身份随机选定，黑盒无法造出两个同 ward 的身份）；白盒 `unwatched.rs::two_readers_of_one_ward_ask_the_host_for_the_same_things` 断言它。
- 时序特征只有 `Tempo.hs` 那两个；本文不新增时序断言。

## 4 现状分析（开工前的探针）

- 撤销后 `send` 成功，被撤者 `read` 得到 `after revoke`（X2 红）。
- 宿主回滚后 Alice 的 `send` 成功于 index 3，Bob 与 Alice 的 `read` 均报 `kusanagi.history_changed`（H19 红）。
- MCP `kusanagi_read` 的 `content[0].text` 是原始 `--json`，`structuredContent` 为空（P1 红）。
- UNC locator 两种写法都触发了对 `127.0.0.1` 的 SMB 访问（L1 红）。
- `cairns/<filed>/<peer handle>`：cairn 以对端明文 handle 命名（S3 红）。
- 散文 `read` 把 `ESC]52;c;…BEL`、`ESC[2J`、U+202E 原样送到终端（M2 红）。
- 黑洞宿主 60 s 内以 `waypoint.io` 返回（`client.rs::PATIENCE`）；死宿主 55 ms；重定向 `waypoint.redirected`，第二监听器零连接；代理死 2.1 s 拒绝且宿主零直连；请求头集合 = T1 所列。

## 5–17 其余各节

命名、依赖、错误处理、工作流程与 `adversary-SPEC.md` 完全相同，不复述。新增文件：
`Forging.hs`、`Leakage.hs`、`Insider.hs`、`Terminal.hs`、`Reach.hs`、`Listener.hs`（脚本化 TCP 监听器，
`Relay.hs` 之外唯一起套接字的地方）、`Scanner.hs`、`Twins.hs`、`test/Surface.hs`。
`Door.hs` 新增动词：`InviteReleasing`、`Group`、`SendGroup`、`ReadMine`、`Forget`、`Export`、`Import`。
`Veil.hs` 导出 `apart`。文档同步：本文、`adversary-SPEC.md` §7 模块表一行、修 Rust 时对应 crate 的 SPEC 与 `docs/codes.md`。

---

*本文档采用 MPL-2.0。*
