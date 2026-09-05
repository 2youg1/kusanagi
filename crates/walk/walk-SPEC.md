# walk-SPEC

`kusanagi-walk`：一条流怎样从宿主读出来。四个文件从 `kusanagi` 搬来，一字未改其规则；搬的理由是 `kusanagi`
撞了 4 000 行的 crate 预算（`ARCHITECTURE.md` §5：撞线的答案是删一个功能或把一个想法搬到有理由持有它的 crate），
而这四个文件本来就是一个想法——**读请求是隐私决策**（D-20）。规则的权威仍在 `kusanagi-SPEC.md`「附：读取路径是隐私决策」，
本文只管边界与不变量，不复述。

## 1 需求拆解

1. `Lane`：一条道——作者、bin、钥匙（`Keyring`，含棘轮）、通道开于哪个 period；`verified` 报本端验到对端多少条。
2. `Sweeping`：逐 period 把 ward 的 bin 交出来（`take() -> Option<Taken>`：列举 + 只 GET 上次列举之外的键）；`CAP`、`DIGITS`。
3. `Stepping`：一条 lane 在手头 bin 里能走多远走多远——开封、解码、验作者、验链——下一个高度不在就停，等下一个 bin。
4. `track_all`/`track`/`peek`：`track_all` 是唯一的取回路径——N 条同 ward 的 lane 共用一个 `Sweeping`，每个 bin 逐 lane `advance`，翻页前丢弃 bin；决定每条 lane 从哪个 cairn 续、sweep 从哪个 period 起（有一条 lane 整链行走即从 `opened` 起、不带 known）、最后写 N 条 cairn 与一条 `(name, ward)` sweep 记录。`track` = 一条 lane 的 `track_all`。`peek` 是唯一按地址点名的读（rendezvous bin 里的介绍流）。
   **删除**：`Source` seam 与 `walk()`——按地址取的实现在生产里没有调用者，1→2→4→8 窗口在 W1 后只是本机 HashMap 查找。

## 2 验收标准

`kusanagi` 的白盒判据不动：`unwatched.rs`、`resuming.rs`、`released.rs`、`at_rest.rs`、`lying.rs`；黑盒 `adversary/Sweep.hs` H20/H21。
F8 加一条：`room.rs::a_read_of_three_members_lists_the_host_as_often_as_a_read_of_one`（三条 lane 与一条 lane 列举次数相同）。
`cargo tree -i kusanagi-walk` 只有 `kusanagi` 一个上游。

## 3 假设与歧义

`Complaint` 来自 `door`，所以本 crate 依赖 `door`——与 `traffic.rs` 搬出去之前一样，失败的码与恢复语只有一个权威。

## 5 权威信源 · 6 命名统一

`ARCHITECTURE.md` §4 的 Stream、Waypoint、Cairn、Ward/Period/Bin；`kusanagi-SPEC.md` 附录 D-20。

## 7 模块边界

```
lib.rs       索引与再导出
lane.rs      Lane、verified
sweep.rs     Sweeping、Taken、CAP、DIGITS
stepping.rs  Stepping、Held、decode
walk.rs      track_all / track / peek、Reach、Walked、starting、confirm
```

依赖：kernel、chain、seal、site、door。**不依赖** waypoint（只经 `kernel::Waypoint` trait）、不采样时钟、不取随机——每个函数收 `now`。
`kusanagi` 再导出全部公开项，所以它的测试仍写 `kusanagi::{Lane, Reach, track}`。

## 11 边界枚举 · 12 错误处理

同 `kusanagi-SPEC.md` 附录「诚实边界」三条；`ward_overfull` 是 `CAP` 的拒绝而非泄漏。

## 15 影响面

只有 `kusanagi`。`AGENTS.md` 的 crate 图与 `ARCHITECTURE.md` §5 各加一行。

## 16 测试与约束

本 crate 无自有测试目录：它的行为由 `kusanagi/tests` 经公开接口断言，由 `adversary/` 经二进制断言；
搬一个 `#[cfg(test)]` 过来只为凑数不合 `rust-coverage-meaningful-tests`。每文件 ≤ 400 行；`src` 881 / 4 000。

## 17 文档同步

`ARCHITECTURE.md` §5、`kusanagi-SPEC.md` §7、`.process/Roadmap.md`「已完成项的落地差别」。
