# vault-SPEC

> `kusanagi-vault` —— 把一份字节交给操作系统保管的那一层：创建时定下的权限、留在物理内存里的读缓冲、以及静态加密的标签字节。
>
> 权威顺序：用户裁决 → `ARCHITECTURE.md` → 本文 → 代码与测试。本文先于代码改动。

## 1 这个 crate 为什么存在

它是从 `site` 里拆出来的，触发条件是 `ARCHITECTURE.md` §5 的 4 000 行/crate 预算：W1 盲读要
往身份记录与通道记录里加 ward，而 `site/src` 已到 3 645。`site-SPEC.md` §7 早就写下了下一次
该拆的是哪一块，理由也早就写在那里：**这里装的不是「端点在自己盘上存了什么」，而是「怎么请
操作系统把一个文件锁在一个账户上」**。

拆成 crate 而不是留作模块，买到的是一句更硬的话：全仓唯一的 `unsafe`、唯一的 `windows-sys`
依赖、唯一的 `cfg` 平台文件对，现在都在一条 crate 边界之内，根 `Cargo.toml` 的抑制允许清单
第三条因此从「一个模块」变成「一个 crate」。

拆分本身没有改变任何行为：四个文件整体搬迁，`SiteError` 换成 `VaultError`，`site::error` 新增
一处 `From<VaultError> for SiteError` 的逐臂映射。

## 2 接口先行

```rust
pub fn create_dir(path: &Path, action: &'static str) -> Result<(), VaultError>;
pub fn write(path: &Path, bytes: &[u8], action: &'static str) -> Result<(), VaultError>;
pub fn write_new(path: &Path, bytes: &[u8], action: &'static str) -> Result<(), VaultError>;
pub fn read(path: &Path, action: &'static str) -> Result<Option<Locked>, VaultError>;
pub fn store() -> &'static str;

pub struct Locked { /* 私有；Deref<Target = [u8]> */ }
pub enum VaultError { Local{action, source}, Permissions{what, source}, ForeignRecord{tag} }
```

`action` 是**调用方用人话说的那句「当时在做什么」**，因为知道这件事的是调用方，而知道稳定码与
恢复命令的是门。三个变体各自对应平台能给出的一种答复：拒绝了、本来会成功但会把字节留给别人读、
交回的字节由本机没有的存储封过。

**`VaultError` 与 `SiteError` 都不是 `#[non_exhaustive]`。** 二者逐臂映射，任何一侧多出一种
没人定过价的失败，都会让构建停下来。

## 3 模块边界

```
lib.rs      模块索引与这一层的全部理由
files.rs    每个平台都一样的那一半：暂存、改名、拒绝碰不是本构建创建的东西
locked.rs   Locked —— 读上来的字节留在物理内存里直到被擦掉
at_rest.rs  标签字节 0x00 明文 / 0x01 DPAPI / 0x02 起留给下一个平台
unix.rs     创建时定模式位：目录 0700、文件 0600
windows.rs  创建时挂受保护 DACL + DPAPI + VirtualLock —— 全仓唯一含 `unsafe` 的模块
```

`Locked` 只借出切片，所以调用方读它和读 `Vec<u8>` 一样，却拿不到一份活得比这次固定更久的副本。
从它里面**解码**出来的东西（`Secret`、展开后的密钥）住在普通页里并各自擦除自己——这条边界写在
这里，而不是留给下一个人去发现。

## 4 平台差异是文件，不是分支

新平台 = 新文件 + `lib.rs` 一行 `cfg` 分发 + 一个新的静态加密标签。`unix.rs` 的 `lock`/`unlock`
今天是空实现并且在此说明：**本工作区只在一个平台上验证**，第二个平台被验证时它才变成 `mlock`
与它的 `munlock`，契约不变——固定失败不上报，因为一份没能被固定的记录仍然是对的记录。

## 5 依赖选型

| 依赖 | 理由 |
|---|---|
| `thiserror` | 与其他 crate 同一套错误派生 |
| `zeroize` | `Locked` 析构时先擦后解固定 |
| `windows-sys` 0.61（仅 `cfg(windows)`） | 微软自己发布的绑定，七个特性。两个现成的封装 crate（`windows-acl` 2019、`windows-permissions` 2020）都无人维护——把 `unsafe` 塞进一个五年没动的依赖不是消除它 |

**不依赖 `kernel`。** 这一层不认识本项目的任何一种标识符，它只认识路径与字节；这也是它能被
`site` 之外的任何东西复用的原因，尽管今天只有 `site` 与 `kusanagi`（后者只为 `doctor` 报出
`store()` 那一个词）用它。

## 6 影响面

上游是 `site`（每一次读写盘）与 `kusanagi`（`doctor` 报 `at_rest`）。`SiteError::Permissions`
与 `SiteError::ForeignRecord` 两个变体留在 `site`，因为门在那里给它们定了稳定码
`site.permissions` 与 `site.foreign_record`；跨边界的那条断言仍在
`kusanagi/src/complaint.rs` 的测试模块。

## 7 测试与约束

`windows.rs` 自带一条 `room_to_lock`。行为判据全部在上游：`kusanagi/tests/at_rest.rs` 断言
站点目录里 grep 不到明文，`kusanagi/tests/profile.rs` 断言权限，`adversary/` 的 S 组断言
同机他人看到的是计数而不是关系图。**本 crate 不新增集成测试目录**：它没有自己的端到端行为，
它的行为就是站点的行为。
