# glass — 一扇看向站点的窗

**位置**：`glass/`，工作区之外。不是 crate，不进 `just budget` 的总数（每文件 400 行仍然算），
不进发布物。**删掉这个目录，网络一字不变**——它与 `adversary/` 同一种关系：只经一个人也会走的门
（`kusanagi --json`）说话，不链接、不 FFI、不共享类型。

## 1 需求拆解

| # | 单元 | 来源 |
|---|---|---|
| F0 | **视觉语域**：一套从产品意义长出来的主题（调色、字阶、圆角、面板），跟随系统明暗，禁用纯白纯黑 | 用户：深度打磨的美学；攻壳机动队 |
| F0a | **G4 弧边**：曲率连续到二阶导的转角，用在 glass 自己画的面上（对话面板） | 用户：每个需要弧边处都用 |
| F0b | **中文正文**：注册一份 OFL 字体覆盖 CJK，不显示豆腐块 | 用户是中文使用者；字体须 OFL |
| F1 | Native SDK 骨架：Zig 薄状态机，每个操作 `fx.spawn` 一次性动词，收 `--json` 应答 | Roadmap 31 |
| F2a | 通道列表、对话视图、撰写与发送；同一发言人的连续段成一组 | Roadmap 32 |
| F2b | 备份：`export` 落盘，恢复密钥只展示一次；释放通道上的备份提示 | Roadmap 32 · S1 · D-07 |
| F2c | 群组：名册编辑、广播发送、逐成员投递行 | 用户：群组管理简陋 |
| F2d | 时隙通道：窗口开着时由 glass 按周期跑 `tick` | I3「F2 以后接管」 |
| F3 | 邀请当面核对：`invite` / `join` 都把四位校验码放大展示 | Roadmap 33 · C2 |
| N | 网络体验：`doctor --here` 状态行、`doctor <waypoint>` 测宿主、错误带恢复命令 | 用户：网络连接体验 |
| D1 | **缺陷**：`CheckRow.title/detail` 是 `Text` 结构体，标记绑定失败，群组面板、体检结果、名册一显示就重建失败（快照 `dispatch_errors=6`） | 本轮发现 |
| D2 | **缺陷**：`native check` 在 Windows 上把根路径拼成 `src\app.native`，随后解析不到 `components/…`；`native markup check src/app.native` 正常 | 本轮发现，CLI 侧 |
| D3 | **缺陷**：`open()` 无条件清 `m.sheet`，欢迎页上 mint 成功后 `channels` 应答触发自动选中，把邀请 sheet 当场关掉——邀请行与四位校验码取不回，步骤 2/3 不可达 | 本轮以 GUI 自动化实测发现 |
| F4 | **设置区收拢**：rail 只剩一页——身份行（开设置）、搜索、通道、群组、必要的门；端点事实、Look、体检/备份/刷新全部折进一张设置 sheet（`Ctrl+,`）；rail 通道行不再印 handle——名字是引见时自己取的 petname，handle 只在对话头（截断）与设置页作为证据出现；第二轮视觉检查还修：invite sheet 步进条与提示文溢出裁切、doctor/delivery 行细节溢出裁切、thread 头 meta 换行溢出（改为 `peer · cadence` 单行省略）、欢迎页三键降为两键（Measure a host 归设置） | 用户：太满，不够留白与聚焦；实测截图 |

## 2 验收标准

- `native markup check src/app.native --strict` 零告警；`native test` 绿；`native build -Dautomation=true` 绿。
- 主题测试：调色板里没有任何一个通道落在 `< 8` 或 `> 250`（纯黑纯白被禁），明暗两套各自通过。
- G4 测试：单位转角总转向角 `= π/2 ± 1e-4`；两端曲率为零；关于 45° 线对称；伸出量等于调用方给的值。
- 布局测试：把整棵视图排一遍，右格的 x 与宽等于 `plate.paneFrame` 所说，`plate.frame` 落在右格之内，
  消息区落在面板之内——`rail_width`、`inset` 与标记里的数由这一条测试绑在一起。
- 自动化：驱动窗口打开邀请、加入、备份、名册、体检五张 sheet 与群组面板，快照 `--absent 'error event='`；
  截图两张（明、暗），中文正文可见。
- 邀请 sheet 结果里有 `invite` 全文与四位 `check`；join 结果同样有 `check`。
- 释放通道被选中时出现备份横幅；导出后横幅换成「已写到 <路径>」并展示恢复密钥。
- 任何 `Complaint` 都以 `code` + `recover` 出现在对话面板底部的状态行，不弹窗、不静默。
- 名字、邀请、正文**一律走 stdin**（`-` 约定），argv 里只有动词与旗标。

## 3 假设与歧义

- **主题从哪里来**。这是一条安静的山路，不是热闹的聊天室：协议里没有在线、没有已读、没有时间戳、
  每个 drop 一个尺寸。所以界面**不显示**在线点、未读数、消息预览与时间——显示不存在的东西是撒谎，
  预览还会把别的通道的正文摊到屏幕上（glass 只读打开的那条）。攻壳机动队给的是夜色与钢：
  暗色是主身份，主色是一抹钝青（thermoptic 的那种冷），亮色是冷灰纸面。**纯白 `#ffffff` 与纯黑
  `#000000` 一处不用**，包括文字。
- **G4 在哪里画**。控件（按钮、气泡、卡片、sheet）的角由引擎在 `widget_render.zig` 用
  `fillRoundedRect` 画，那是 npm 包，不打补丁。glass 自己画的面只有一处：**对话面板**（chrome 前缀命令），
  它的矩形只由窗口尺寸与模型里的分栏比例推出，所以 resize 与拖分栏都不滞后。`Options.sync`
  能读回布局框架，但读到的是**上一次**的布局，chrome 在**本次**布局前发射——用它给滚动区里的气泡垫底板
  会每帧错位，故不做。
- **G4 的定义（自己写，不抄 RefRain）**。转角是弧长 s∈[0,L] 上曲率 κ(s)=K·b(s/L) 的平面曲线，
  b(u)=64u³(1−u)³ 在两端有三阶零点，故 κ、κ′、κ″ 在与直边相接处都为零——这就是 G4。
  ∫b = 16/35，令总转向 π/2 得 K = 35π/(32L)。b 关于 u=½ 对称，曲线关于 45° 线对称，
  两轴伸出量相等，记为 E（单位 L=1 时的数值，由积分得到）；要伸出 e 像素就把单位曲线放大 e/E。
  曲线以 24 段折线发射，一块面板 4 角约 110 个路径元素，远在每帧 2048 的预算内。
- **左栏定宽，不可拖**。可拖分栏的分隔子件不继承任何样式，那条线永远是 `border` 色：贴着面板画成缺口，
  离开面板又成第二条线，chrome 后缀盖住它会连悬停反馈一起盖掉。一个聊天窗口不需要拖栏，定宽 248 少掉一条线、
  一段复刻的钳制公式、一个 `pane` 字段和一条 `resize` 消息；面板矩形 = 右格四边各内缩 12，由 `rail_width`
  一个常数决定，标记的 `width="248"` 与它由 §2 的布局测试绑住。
- **rail 是一页，设置是另一张 sheet**。rail 回答「现在和谁说话」，设置回答「这个端点是什么、怎么维护」——
  两个问题两种时间尺度，不该叠在同一个列表的第二页上。身份行按下去开 `settings` sheet（`Ctrl+,` 同），
  `Rail` 枚举与 `show_endpoint/show_channels` 随之删除——少两个消息、少一次状态分叉。
- **handle 是证据，不是装饰**。名字是引见时自己取的 petname（`invite/join --name`），是每行真正的标识；
  handle 只在两处出现：对话头截断 12 位（正在和谁说话的凭据）与设置页全文（复制给要核对的人）。
  实测：同一个 handle 发给每一个对端，合谋的对端本就能关联；把它印在每行下面只制造「要跟哈希打交道」的错觉，
  不增加任何隐私，也不帮助区分 a/b/c/d——区分靠 petname（`mai`、`mai-agent`）。
- **每行只在协议确有话可说时才有第二行**：`waiting for them`（对端未到）；时隙与释放仍用图标说；
  作废仍用 badge。没有预览、没有未读、没有 handle——单行名单扫得快，纸面留白。
- **字体**。Windows 渲染器把 DirectWrite 回退表**故意置空**，注册的字面也不级联到别的字族，
  所以正文字面必须自己覆盖 CJK。Noto Sans SC（OFL，Google Fonts 的 TrueType 构建，17 772 300 字节，
  `maxp` 最大 584 点 / 84 轮廓、无复合）作正文，Geist Mono（OFL，SDK 捆绑）作等宽。
  字体不进 git：`just glass-fonts` 按固定 URL 下载并核对 SHA-256，`.gitignore` 排除 `*.ttf`。
- **对话怎么排序**。段不带时间。C4 的 `acknowledged` 是两条流之间唯一的先后关系；`order.zig` 做因果归并。
  归并后**同一发言人的连续段成一组**：组内间距 8，换人间距 24（SDK 的气泡指南是 8/32，24 更贴近
  这个面板的密度）。
- **群组是广播名单，不是群聊**（E1）。群组视图 = 名册 + 广播撰写 + 投递行。
- **`import` 不做**：stdin 上限 4 KiB，归档进不去。界面写明「在终端跑 `kusanagi import`」。
- **二进制在哪**：与 glass 同目录的 `kusanagi(.exe)` 优先，否则 PATH 上的 `kusanagi`。

## 4 现状分析

骨架已跑起来并被快照过：侧栏、对话页、五张 sheet 的标记都在，`native test` 10/10。快照头部
`dispatch_errors=6`，六条全是 `MarkupBuild`，根因是 D1：`{d.title}` 落在一个 `Text` 结构体上。
`native check` 因 D2 整体判红，`native markup check src/app.native` 与 `native test` 是可用的门。
`kusanagi` 每个动词 15–40 ms，轮询周期 20 s，只读当前打开的对话。

## 5 权威信源

`ARCHITECTURE.md` §1–§4（定义、词汇、七条性质——它们决定了界面不能显示什么）；`door-SPEC.md` §3
（`acknowledged`）；`kusanagi-SPEC.md`（动词与 `-` 约定）。Native SDK 0.10.1：`native-ui.md`
（元素表、Style token attributes、Effects、Testing pattern、automation）；SDK 源码
`primitives/canvas/tokens.zig`（`DesignTokens.theme/withOverrides`、`accentOverrides`）、
`runtime/ui_app.zig`（`Options.tokens_fn/chrome/fonts/on_appearance/sync`、`rebuild` 的调用顺序）、
`primitives/canvas/widget_layout.zig`（`splitEffectiveFraction`）、`runtime/canvas_limits.zig`
（字体上限 24 MiB、8 槽）、`platform/windows/gpu_surface_renderer.cpp`（空回退表）。

## 6 命名统一

Glass（本目录）；Channel / Roster / Cadence / Retention / Offer / Cairn 沿用 §4；`check` 沿用
`Outcome::Invited.check`。glass 自己的三个词：**rail**（左栏，直接坐在纸面上）、**plate**（对话面板，
glass 画的唯一一块面）、**theme**（token 语域）。界面文案英文，中文文档在 README。

## 7 模块边界

```
main.zig      场景、字体注册、tokens_fn、chrome、create、run —— 只有接线
model.zig     Model / 有界存储 / 壳与对话的绑定方法
sheets.zig    五张 sheet 各自的状态结构体与绑定方法（嵌套路径 {invite.nameText}）
rows.zig      有界记录：Text、ChannelRow、GroupRow、Message、Lane、Bubble、Status、CheckRow
theme.zig     调色板与 tokens(appearance) → DesignTokens；纯函数，有测试
plate.zig     G4 转角、面板几何（复刻分栏公式）、chrome 发射；纯函数，有测试
update.zig    update：每个 Msg 一臂，副作用只在这里发出
verbs.zig     每个动词一个 spawn 构造：argv 与 stdin 的唯一权威
answer.zig    `--json` 应答 → 模型字段；Complaint → 状态行
order.zig     两条流的因果归并（纯函数，有测试）
app.native    壳：rail + plate 内容 + sheet
components/   rail / thread / sheets / more
tests.zig     假执行器下的派发与 spawn 断言、布局绑定测试
```

数据只朝一个方向流：视图派发 Msg → update 改模型或 spawn → 退出 Msg 带回 JSON → answer 写模型 → 重建视图。
外观走另一条：OS → `on_appearance` → `Msg.appearance` → `model.appearance` → `tokens_fn` 与 chrome。

## 8 接口先行

```zig
// theme.zig
pub const body_font_id: canvas.FontId = 64;          // Noto Sans SC；等宽沿用 SDK 的 Geist Mono
pub fn tokens(appearance: platform.Appearance) canvas.DesignTokens;
pub fn palette(scheme: canvas.ColorScheme) canvas.ColorTokens;   // 测试用：逐通道断言不触纯白纯黑

// plate.zig
pub const rail_width: f32 = 248;              // 左栏宽，与标记 width 相同
pub const inset: f32 = 12;                    // 面板与右格四边的距离
pub const corner: f32 = 56;                   // 每条边被转角吃掉的长度 e；读起来约等于 28 的圆角
pub const unit: Corner;                       // 单位 G4 转角，comptime 积分，{ points, extent }
pub fn paneFrame(size: geometry.SizeF) geometry.RectF;             // 右格
pub fn frame(size: geometry.SizeF) geometry.RectF;                 // 右格四边内缩 inset
pub fn outline(builder: *canvas.Builder, rect: geometry.RectF, extent: f32) ![]const PathElement;
pub fn build(m: *const Model, builder: *canvas.Builder, size: geometry.SizeF, tokens: DesignTokens) !void;
pub const prefix_commands: usize = 3;         // shadow · fill_path · stroke_path

// model.zig 增
appearance: platform.Appearance = .{},
pub const view_unbound = .{ … };              // 只被 update/fx 读的状态，strict 检查要它
pub fn thread(m, arena) []const Bubble;       // Bubble 增 `turn: bool`——是否开启新的一组
```

Msg 增 `appearance: platform.Appearance`。

## 9 工作流程

启动 → 注册字体（安装帧，首次布局前）→ `on_appearance` 送来系统明暗 → `tokens_fn` 解出语域 →
`boot`：`doctor --here`、`id`、`channels` 并发 → rail 出现 → 选中一条通道 → `read`（peer）与
`read --mine` → 归并、分组、展示 → 定时器每 20 s `read --after h`（时隙通道改为每 period 秒 `tick`）
→ 发送：`send --to -`，stdin = `name\ntext` → 成功后自己的段直接追加进 ring，不重读。
每次重建：chrome 先按 `size` 与 `pane` 画面板（阴影、G4 填充、描边），再是控件。

## 10 实现逻辑

1. **D1**：`CheckRow` 字段改名 `name/note`，加 `title()/detail()` 方法，与 `ChannelRow` 同形。
2. **theme.zig**：`DesignTokens.theme(.{scheme, contrast, reduce_motion, .pack = .house})`
   → `.withOverrides(accentOverrides(accent, scheme))` → `.withOverrides(调色板/圆角/字阶)`。
   高对比时跳过主色包（无障碍高于品牌，与 SDK 自己的规则一致）。
3. **plate.zig**：单位转角在 comptime 用中点法积分 24 段，后半段由镜像得到，端点精确落在对角线上；
   `frame` = 右格四边内缩；`build` 发三条命令，个数固定，`prefix_commands = 3`。
4. **model 拆分**：五张 sheet 的 `TextBuffer` 与绑定搬进 `sheets.zig`，模型字段
   `invite: InviteSheet` 等，标记改绑 `{invite.nameText}`；`model.zig` 回到 400 行以内。
5. **标记重排**：`row` 里一个定宽 `column` 是 rail，直接坐在纸面；右格 `padding` 等于面板 inset；
   plate 内是 header · banners · thread · composer · 状态行；
   状态行从窗口底部搬进 plate。rail 单页：身份行（`show_settings`）· 搜索 · 通道行（单行名字 + 图标，
   只在 `waiting for them` 时有第二行）· 群组行（名字 + 成员数）· 底部两扇门（有通道时；欢迎页由 hero 负责）。
   设置 sheet：handle 块与复制 · 端点事实 · Look · 体检/备份/刷新 · import 说明（原端点页整体迁入）。
   气泡：己方 `primary`，对方 `default`（`surface_subtle` 洗色），换人前插一个 24 的 spacer。
   校验码：等宽、display 字阶、字符间空格，由 `checkSpaced` 在 arena 里拼。
   sheet 内长文一律独占一行向下 wrap，不与定宽字段同行（行内 flex 不收缩，溢出即裁切——invite 提示行与
   doctor/delivery 行的教训）；invite sheet 去掉步进条（两态界面不需要三步的装饰），表单字段 `on-submit`
   直达主键（Enter 从任一字段提交）。
6. **字体**：`assets/fonts/NotoSansSC.ttf` 由 `just glass-fonts` 下载并校验；`main.zig`
   `@embedFile` 后在 `Options.fonts` 注册为 id 64；`theme.zig` 的 `typography.font_id = 64`。
7. **strict**：`view_unbound` 列出只被 update/fx 读的状态；派生方法只留视图真绑的。

## 11 边界枚举

- 窗口窄：右格随窗口缩，面板与内容一起缩；转角伸出量取 `min(corner, 宽/2, 高/2)`，面板永不自交。
- 高对比：主色包跳过，取 house 的高对比语域；纯黑纯白的禁令只对 glass 自己的调色板负责。
- reduce-motion：`DesignTokens.theme` 已把 motion 换成 reduced，glass 不另加动效。
- 字体注册失败：SDK 记进派发错误环，窗口照常跑，中文成豆腐块——快照里能看见，验收会红。
- 没有通道：欢迎页在 plate 内；二进制不在：欢迎页给出「put kusanagi beside glass, or on PATH」。
- 应答被截断（`output_truncated`）：状态行提示「history too long to show whole」，已解析部分照常。
- 通道 `peer == null`：对话页显示「waiting for them to join」，撰写框禁用。
- `refused` / `peer_refused` 非空：rail 行带 void 标记，对话页说明码。

## 12 错误处理

一切动词失败都是 `Complaint`：`status = { code, error, recover }`，渲染在 plate 底部状态行；
`rejected` / `spawn_failed` 是 glass 自己的失败，用固定文案。视图永不因错误退出。
chrome 命令数不对会让 SDK 报 `InvalidChromeCommandCount`——`prefix_commands` 是常数，测试断言 `build`
恰好发三条。

## 13 依赖选型

Native SDK 0.10.1（Zig 0.16）；`std.json` 解析。字体：Noto Sans SC（SIL OFL 1.1，Google Fonts
TrueType 构建）、Geist Mono（SIL OFL 1.1，SDK 捆绑）。不用系统字体（微软雅黑不是 OFL）。

## 14 硬编码声明

| 项 | 值 | 意图 |
|---|---|---|
| 亮色 | paper `#f0f1f4` · surface `#f8f9fa` · subtle `#e4e7ec` · pressed `#d6dae1` · text `#1c2027` · muted `#68707e` · border `#d3d7de` · accent `#0f766e` / ink `#f1faf8` | 冷灰纸面，钝青主色 |
| 暗色 | paper `#0f1217` · surface `#171c23` · subtle `#242b35` · pressed `#2f3844` · text `#e3e7ed` · muted `#8f98a7` · border `#2e3642` · accent `#5cc8bb` / ink `#0f1a19` | 夜色与钢；相邻两阶至少差一档，次级按钮不与底融合 |
| 控件 | `button_secondary` = subtle 填充 + border 描边；`button_outline` = border 描边 | 次级动作在面板与纸面上都读得出是按钮 |
| 圆角 | sm 8 · md 10 · lg 14 · xl 20；`controls.bubble.radius` 18 | 控件在 md，卡片在 lg |
| 字阶 | body 14 · label 13 · title 18 · heading 22 · display 40 | 对话头用 heading，校验码与词标用 display（等宽） |
| 面板 | inset 12（四边）· corner 56 · 阴影 blur 18 / y 6 | 转角吃掉每边 56，读起来约等于 28 的圆角 |
| 左栏 | `rail_width` 248，内边距 16，行内 6 | 与标记 `width="248"` 相同，由测试绑住 |
| 设置 | `Ctrl+,` 开；身份行同；handle 全文、端点事实、Look、三个维护动作、import 说明都在这一张 sheet | rail 二页制的替代，少一个枚举少两臂消息 |
| 面板内 | header padding 20 · 正文列 padding 24 · 撰写框高 64 | 文字离面板边至少 24 |
| 分组 | 组内 8 · 换人 24 | SDK 指南 8/32 的密度版 |
| 其他 | 轮询 20 s；ring 128 条/流；正文 3 584 字节；备份写到 `<home>/kusanagi-backup-<unix ms>.ksnb` | 同上一版 |

## 15 影响面

`door::Entry` 增 `acknowledged`（已落地，JSON 增字段，契约不变）。`justfile` 增 `glass-fonts`、
`glass`、`glass-test`；根 `.gitignore` 增 `glass/assets/fonts/*.ttf`。`app.json` 不变。
标记文件 `components/sidebar.native` 改名 `rail.native`，`main.zig` 的 `markup_sources` 同步。

## 16 测试与约束

`tests.zig`：(1) 点 rail 行 → 派发 `select` 并 spawn `read`，argv 里不含名字，stdin 含名字；
(1b) sheet 开着时 `channels` 应答触发自动选中，sheet 仍开着（D3 的回归）；(2) 撰写后按发送 → spawn `send`，stdin = `name\ntext`；(3) 邀请 sheet 结果解析出 `check`；
(4) Complaint 落到状态行；(5) 布局：面板矩形在第二格内、内容列在面板内；(6) 群组面板、名册、体检、
备份四个状态各建一次树（D1 的回归）；(7) 身份行开设置 sheet，Look 在其中改写 `appearanceFor`。
`theme.zig`：明暗两套逐通道 ∈ [8, 250]。`plate.zig`：转向角、
端点曲率、对称、伸出量、`build` 恰好三条命令。`order.zig`：既有四条。约束：每文件 < 400 行。

## 17 文档同步

`glass/README.md`（命令、字体来源与许可证、主题说明）；`ARCHITECTURE.md` §4 加 Glass 一行
（原地换行，不加行数——本轮不动，列为待办）；`.process/Roadmap.md` F1–F3 关闭；
`.process/HANDOFF.md` 下一次重写时记录 D2（CLI 路径缺陷）与字体下载步骤。
