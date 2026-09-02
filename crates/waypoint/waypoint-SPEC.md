# waypoint-SPEC

> `kusanagi-waypoint` —— 存放字节的地方，以及一个替换实现必须通过的契约。
>
> 权威顺序：用户裁决 → `ARCHITECTURE.md` → 本文 → 代码与测试。本文先于代码改动。

## 1 需求拆解

| 单元 | 交付物 | 独立验收 |
|---|---|---|
| U1 契约 | `conformance::run` | 五条子句；两个本地适配器与一个 TCP 上的宿主都必须通过 |
| U2 本地适配器 | `DirWaypoint` / `MemoryWaypoint` | 通过契约；八线程并发写同一地址恰有一个 `Stored` |
| U3 条件能力 seam | `Conditional`（`Validator` / `Fetched` / `TtlOutcome`） | 由 HTTP 与 S3 实现；目录如实回答「不提供」 |
| U4 HTTP 盒子的客户端 | `HttpWaypoint`（服务端在 `kusanagi-box`） | **两个进程经 TCP** 收发；CI 内可跑，不需外网 |
| U5 对象存储 | `S3Waypoint`（SigV4） | 签名复现 AWS 公开向量；有凭据时对真实桶跑通，无凭据时跳过而非假通过 |
| U6 路由 | `Locator` / `Place` | 一个字符串决定用哪个适配器；四种写法各得正确的 `kind()` |
| U7 体检 | `probe::examine` → `Certificate` | 四项能力各得 held / not offered / BROKEN；只有 write-once 决定 tier |

## 2 验收标准

1. 两个本地适配器 + HTTP 盒子都通过 `conformance::run`。
2. `Server` 与 `HttpWaypoint` 之间经真实 socket 完成写读、拒绝覆写、304、TTL 四件事。
3. SigV4 签名对 AWS 文档中 `GET /test.txt` 的向量输出 `f0e8bdb8…6bdb41`。
4. 一个**接受覆写**的假宿主被 `examine` 判为 `Broken` 且 tier 降为 `ack-first-seen`。
5. 目录被判为 `write-once` tier，条件读与过期为 `not offered`（具名降级，不是失败）。
6. `Credentials` 与凭据相关的 `Debug` 不打印 secret。

## 3 假设与歧义

| 歧义 | 假设 | 何时失效 |
|---|---|---|
| S3 兼容宿主是否都支持 `If-None-Match: *` | **不假设**。用 `doctor` 实测 | 永不失效；这是本 crate 存在的核心理由之一 |
| S3 的寻址风格 | path-style：`https://ENDPOINT/BUCKET/KEY` | R2 与 MinIO 支持；AWS 新桶要求 virtual-hosted，届时把 bucket 放进 endpoint |
| 每对象 TTL | S3 没有，只有桶级生命周期规则 | 报告为 `NotOffered`，不是失败 |
| 盒子是否需要认证 | 不需要 | 见 `docs/box-protocol.md`：宿主知道调用者是谁，就知道了设计承诺它不知道的东西 |

**阶段 0 遗留的假设行已删除。** 旧版本此处写着「MinIO 的行为待 `doctor` 实测替换」；现在 `doctor` 已存在，判断由它当场做出，本文不再复述任何一家宿主的行为。

## 4 现状分析

`DirWaypoint` 的一次性写入靠**暂存 + `fsync` + `hard_link`**：硬链接在目标已存在时失败，于是「占位」与「检查」是同一个操作而不是两个之间夹着竞态的操作。写到一半崩溃只会在暂存目录留下垃圾，不会在一个按定义永不能重写的地址上留下半个段。

## 5 权威信源

| 事实 | 来源 |
|---|---|
| S3 自 2024-08 支持 `If-None-Match: *` 的条件写 | AWS S3 用户指南 *Conditional writes* |
| R2 支持该条件头，失败返 `412 PreconditionFailed` | R2 的 S3 扩展文档 |
| MinIO 的两处语义分歧都朝失败开放（条件被静默忽略） | minio#20346、minio#21526 |
| SigV4 的规范请求、待签串与签名密钥推导 | AWS 一般参考；测试向量取自 `GET /test.txt` 的公开示例 |
| SHA-256 空载荷哈希 `e3b0c442…b7852b855` | 同上 |

## 6 命名统一

`Waypoint` 取自 `ARCHITECTURE.md` §4。`Place` 是已打开的具体地点，`Locator` 是尚未打开的字符串形式；`Server`（盒子的服务端一半）已移到 `kusanagi-box`。`Capability` 的四个名字（`write-once`、`conditional-read`、`stable-validator`、`expiry`）是**公开标识符**，会出现在 `doctor` 输出与读它的脚本里，改名即改公开接口。

## 7 模块边界

```
lib.rs          模块索引
conformance.rs  契约：一个函数，不是一组 #[test]
conditional.rs  Conditional seam：Validator / Fetched / TtlOutcome
dir.rs          目录适配器
memory.rs       内存适配器
http.rs         盒子的客户端一半
s3.rs           对象存储适配器
sigv4.rs        Signature Version 4：凭据、日期、签名
place.rs        Locator / Place —— 唯一知道存在多个适配器的地方
probe.rs        examine —— 唯一实测宿主的地方
certificate.rs  Capability / Verdict / Tier / Certificate —— 实测结果的公开词汇
```

依赖：`kernel`、`seal`（契约与体检用真实派生地址）、`blake3`（ETag）、`hmac` + `sha2`（SigV4）、`ureq`（HTTP）、`thiserror`。

### 被推翻的决定：服务端搬出去了

本节原文写的是「**客户端与服务端同居一个 crate**，因为它们是同一份协议的两半；
分在两个 crate 就是一份协议两个权威」。服务端现已移到 `crates/box`，理由如下。

**新出现的事实**：本 crate 的 `src/` 碰到了 `ARCHITECTURE.md` §5 的 2,500 行上限（实测 2,555）。
那条预算写明「下一次对 waypoint 的实质改动以拆分开头」，所以问题不是拆不拆，而是沿哪条缝拆。
可选的缝只有两条：把几个适配器分开，或把服务端分出去。
**把同一个 seam 的多个实现分开更坏**——`Waypoint` 的四个实现必须能一起跑同一份 `conformance::run`；
而客户端与服务端是**两件不同的工作**：一个是「怎么去到一台宿主」，一个是「怎么当一台宿主」。

**原来怕的东西由别的东西拦住。** 当时怕的是两半协议各自漂移；拦住它的不是目录，而是测试：
`crates/box/src/serve.rs` 的测试用**出货的客户端**（`HttpWaypoint`）经真实 socket 驱动**出货的服务端**，
并对它跑一遍 `kusanagi_waypoint::conformance::run`。那条检查现在跨 crate 边界，强度不变。
协议的权威从来不是“两个文件挺近”，而是 `docs/box-protocol.md`，它没有动。

**额外买到的**：从不当宿主的端点现在不再编译一个服务器。见 `crates/box/box-SPEC.md`。

## 8 接口先行

```rust
pub fn conformance::run(waypoint: &impl Waypoint, namespace: &Stream) -> Result<(), Failure>;

pub trait Conditional {
    fn get_if_changed(&self, addr: &DropAddr, known: Option<&Validator>) -> Result<Fetched, WaypointError>;
    fn put_with_ttl(&self, addr: &DropAddr, bytes: &[u8], seconds: u64) -> Result<TtlOutcome, WaypointError>;
}

pub enum Locator { Directory(PathBuf), Box { base: String }, Bucket { .. } }   // FromStr
pub enum Place { Directory(..), Box(..), Bucket(..) }                          // impl Waypoint + Conditional
impl Place { pub fn open(&Locator, Option<Credentials>, now: u64) -> Result<Self, LocatorError>; }

pub fn probe::examine<P: Waypoint + Conditional>(place: &P, namespace: &Stream) -> Certificate;
pub enum Verdict { Held, NotOffered { because: String }, Broken { detail: String } }
pub enum Tier { WriteOnce, AckFirstSeen }
```

**为什么 `Conditional` 是第二个 trait 而不是 `Waypoint` 的两个新方法**：`Waypoint` 是每个地方都要实现的 seam，每多一个方法就多一件某人的 U 盘适配器必须假装会做的事。条件读是**传输能力**，可以如实地被拒绝提供。

**为什么 `Verdict` 分三档而不是布尔**：`NotOffered` 与 `Broken` 是两件不同的事——目录没有 ETag 是有名字的缺席，宿主声称支持却做不到是故障。合成一档就等于让「诚实的目录」和「撒谎的宿主」得到同一个待遇。

## 9 工作流程

```
locator 字符串 → Locator::from_str → Place::open(credentials, now) → Waypoint / Conditional
doctor：Place → probe::examine → Certificate{ 四项 Finding } → Tier → CLI 渲染
盒子的服务端已移出本 crate，流程见 `crates/box/box-SPEC.md` §9
```

## 10 实现逻辑

**步骤 1：契约是函数不是测试。** 仓外写的适配器要能跑同一批子句，`doctor` 要能对**活着的宿主**跑并打印是哪一条不过。所以失败是数据（`Failure`），不是 panic。

**步骤 2：契约用真实派生地址。** `run` 接收一个 `Stream`，地址经 `seal::derive` 得出，与生产流量完全同路。这样把它指向真实宿主是安全的：从一个别人没有的秘密派生，不可能撞上任何人的地址。

**步骤 3：盒子没有无条件写。** `PUT` 缺 `If-None-Match: *` 返回 `428`。协议里不存在能覆写的请求，于是这台宿主**没有办法**意外失去一次性写入语义。

**步骤 4：TTL 为 0 意为「已过期」。** 这让过期能被确定性地检验：不必睡一秒，也不必相信宿主的确认。宿主把过期时刻写在字节前面的 8 字节里，读取时与自己的时钟比较——被清扫的对象与从未存在的对象因此是同一个答案 `404`，没有需要对账的簿记。

**步骤 5：SigV4 手写。** 用到的 S3 接口只有两种请求；一个 SDK 会带来异步运行时与数百个 crate。签名九十行，并且用 AWS 公开的向量钉住——否则这是一段**没有任何本地测试能证伪**的代码。

**步骤 6：日期换算手写。** 全仓唯一的日期算术，用 Howard Hinnant 的 `civil_from_days`，每一步 `checked_*`。荒谬的时钟读数返回 `None` 而不是回绕成一个「因为无人能诊断的原因被拒绝」的签名。

**步骤 7：`host` 头不随签名一起发送。** 传输层自己会设置它；发两次正是让签名与承载它的请求不一致的经典原因。

## 11 边界枚举

| 情形 | 期望 |
|---|---|
| 读一个空地址 | `Ok(None)` / `Fetched::Absent` |
| 写一个已占用地址 | `PutOutcome::AlreadyPresent`（不是错误） |
| 八线程抢同一地址 | 恰有一个 `Stored` |
| 根目录其实是一个文件 | `waypoint.io` |
| locator 写成不认识的 scheme（`ftp://…`） | `LocatorError::UnknownScheme`，码 `locator.unknown_scheme`。**不得当成相对目录**：那会让 `doctor` 去实测一个文件名，给出四条与问题无关的 BROKEN |
| 盒子返回 428 | `WaypointError::OverwriteNotRefused` |
| 盒子返回未知状态码 | `UnusableAddress`，把状态码写进 reason |
| 请求头超过 8 KiB / body 超过 1 MiB | `400` |
| 空 payload | 正常往返；**空与缺席是两件事** |
| S3 返回 403 | 视同 `Absent`（多数桶对不存在的 key 返回 403 而非 404） |
| 桶 locator 缺 bucket 段 | `LocatorError::BucketIncomplete` |
| 桶 locator 无凭据 | `LocatorError::CredentialsMissing` |

## 12 错误处理

传输失败一律 `WaypointError`，带 `action` 说明当时在做什么。**状态码不当作传输错误**（`http_status_as_error(false)`）：404、412、304 都是本协议的正常回答，分不清它们与断线的适配器只能靠猜。

`probe::examine` **从不返回 `Err`**：宿主行为不端是结果而不是中断，因此不可达的宿主会让四项都成为 `Broken` 并附上 io 消息。

## 13 依赖选型

| 依赖 | 理由 | 替代方案与代价 |
|---|---|---|
| `ureq` 3（rustls） | 阻塞式 HTTP + TLS，纯 Rust；每个 verb 都是一次性命令，异步运行时在这里无事可做 | `reqwest` 带 tokio；裸 TCP 无法访问真实 HTTPS 桶 |
| `hmac` + `sha2` | SigV4 规定 HMAC-SHA256，不是我们能选的 | 无 |
| `blake3` | ETag，与全仓同源，且让稳定性成为构造性质 | 用 mtime 或计数器会让 ETag 不稳定 |
| 服务端不引入 HTTP 框架 | 三个请求、两百行、零依赖 | 一个框架的表面积比它要服务的协议大一个数量级 |

## 14 硬编码声明

| 硬编码 | 意图 |
|---|---|
| `MAX_HEAD = 8 KiB`、`MAX_BODY = 1 MiB`、`IDLE = 30s` | 让恶意调用者无法用一个请求耗尽宿主 |
| `SHARD_WIDTH = 2` | 目录分片；地址本已均匀，直接取前缀比再哈希一次更省 |
| `BANNER = "kusanagi-box/1 …"` | 一句自述，**不是证据**；`doctor` 无视它并实测 |
| `TTL_HEADER = "X-Kusanagi-Ttl"` | 盒子协议的扩展头，见 `docs/box-protocol.md` |

## 15 影响面

`kusanagi::assembly` 通过 `Place` 使用全部适配器；`certificate::Capability` 的名字出现在 `doctor` 的 JSON 输出里；盒子协议同时约束本 crate 的 `http.rs`、`kusanagi-box` 的 `serve.rs` 与 `docs/box-protocol.md`，三者必须同一次改动。

## 16 测试与约束

25 个单元测试 + 2 个真实桶测试（无凭据时打印 skipped 并返回）。承重的三个：

- `the_whole_contract_holds_over_tcp`——契约经真实 socket 全过；
- `a_host_that_overwrites_is_reported_broken_not_ignored`——假宿主静默覆写被判 `Broken`；
- `signing_reproduces_the_published_aws_vector`——无凭据也能证伪 SigV4。

**真实桶测试无凭据时不算通过**：它打印 `skipped: …` 并说明需要哪三个环境变量。一个静默略过了唯一验证核心假设的测试的绿色套件，比没有测试更糟。

## 17 文档同步

1. 本文。
2. `docs/box-protocol.md`——盒子协议的任何改动。
3. `ARCHITECTURE.md` §5 行数表、§6 seam 表。
4. `README.md` 的「Where drops can live」与 `doctor` 段落。
