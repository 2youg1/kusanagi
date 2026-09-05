// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! Every sentence the window shows, in both languages, in one table.
//!
//! The markup binds `{t.key}` and nothing else, so a label that exists in one
//! language and not the other cannot compile: `Strings` is generated from the
//! table, and `of` fills every field from the column the language names.
//! Concept words stay as they are in `ARCHITECTURE.md` §4 — handle, drop,
//! cairn, waypoint — because a translated concept word is a second authority.
//!
//! Chinese here is only ever drawn through the body face `font.zig` found, so
//! the model refuses `zh` while there is no such face; that rule lives in
//! `Model.setLanguage`, not here.

const std = @import("std");
const builtin = @import("builtin");

pub const Language = enum { en, zh };

/// One label offered on the settings sheet, in its own language.
pub const LanguageRow = struct { tag: Language, label: []const u8 };
pub const language_rows = [_]LanguageRow{
    .{ .tag = .en, .label = "English" },
    .{ .tag = .zh, .label = "中文" },
};

const Entry = struct { [:0]const u8, []const u8, []const u8 };

const table = [_]Entry{
    // the rail
    .{ "this_endpoint", "You", "你" },
    .{ "open_settings", "open settings", "打开设置" },
    .{ "waiting_for_them", "waiting for them", "等对方加入" },
    .{ "slotted", "slotted", "定时隙" },
    .{ "releases", "releases", "阅后释放" },
    .{ "void_hint", "this channel no longer permits anything", "这条通道已不再允许任何操作" },
    .{ "void_badge", "void", "作废" },
    .{ "measure_its_host", "Measure its host", "测量它的宿主" },
    .{ "revoke_peer", "Revoke the peer", "撤销对方" },
    .{ "forget_channel", "Forget this channel", "忘掉这条通道" },
    .{ "no_channels_yet", "No channels yet", "还没有通道" },
    .{ "member", "member", "位成员" },
    .{ "members", "members", "位成员" },
    .{ "find_channel", "Find a channel", "找通道" },
    .{ "channels", "Channels", "通道" },
    .{ "groups", "Groups", "群组" },
    .{ "new_group", "New group", "新建群组" },
    .{ "new_invitation", "New invitation", "新邀请" },
    .{ "join_with_invitation", "Join with an invitation", "用邀请加入" },
    // the thread
    .{ "measure_this_host", "Measure this host", "测量这台宿主" },
    .{ "only_copy_title", "This site is the only copy of this conversation", "这个站点是这段对话唯一的副本" },
    .{ "only_copy_body", "Each drop is deleted once they have read it, and its key is burned. Keep a backup, or a lost disk is a lost conversation.", "对方读过的内容会被删除,密钥随之烧毁。做好备份,否则丢盘就是丢对话。" },
    .{ "back_up_now", "Back up now", "现在备份" },
    .{ "waiting_title", "Waiting for them to accept", "等对方接受" },
    .{ "waiting_body", "Nobody has joined this channel yet. Hand over the invitation line and read the four characters to each other; the first read after they join learns who they are.", "还没有人加入这条通道。把邀请行交给对方,两人互相念那四个字符;对方加入后的第一次读取会知道他们是谁。" },
    .{ "you", "you", "你" },
    .{ "them", "them", "对方" },
    .{ "cut_to_fit", "… cut to fit this window", "……为适应窗口已截断" },
    .{ "not_text", "bytes that are not text, as hex", "不是文本的字节,以十六进制显示" },
    .{ "nothing_here", "Nothing here yet", "这里还没有内容" },
    .{ "nothing_here_body", "What you send lands at an address only the two of you can derive.", "你发出的内容会落在只有你们两人能推导出的地址上。" },
    .{ "message", "Message", "消息" },
    .{ "messages", "Messages", "消息列表" },
    .{ "write_something", "Write something…", "写点什么……" },
    .{ "tally", "{d} of theirs verified · {d} of yours", "对方 {d} 条已验证 · 你 {d} 条" },
    .{ "send", "Send", "发送" },
    .{ "try_prefix", "try:", "试试:" },
    .{ "dismiss", "Dismiss", "关闭提示" },
    .{ "note", "note", "提示" },
    // settings
    .{ "settings", "Settings", "设置" },
    .{ "handle", "Handle", "Handle" },
    .{ "copy_handle", "Copy the handle", "复制 handle" },
    .{ "handle_hint", "A hash of the public key: what a peer sees, never a name.", "公钥的哈希:对端看到的是它,永远不是名字。" },
    .{ "site", "Site", "站点" },
    .{ "sealed_by", "Sealed at rest by", "静态加密由" },
    .{ "route", "Route", "线路" },
    .{ "binary", "Binary", "二进制" },
    .{ "not_answered", "not answered yet", "尚未应答" },
    .{ "not_found", "not found", "未找到" },
    .{ "through_proxy", "through the proxy", "经代理" },
    .{ "direct", "direct", "直连" },
    .{ "look", "Look", "外观" },
    .{ "look_system", "System", "跟随系统" },
    .{ "look_light", "Light", "浅色" },
    .{ "look_dark", "Dark", "深色" },
    .{ "language", "Language", "语言" },
    .{ "language_hint", "The Chinese option appears once a face that can draw it is chosen below.", "中文选项在下方选定一枚能画汉字的字面后出现。" },
    .{ "face_heading", "Body face for Chinese", "中文正文字面" },
    .{ "face_none", "none — labels stay in English", "未选:界面保持英文" },
    .{ "face_hint", "Drop a .ttf onto this window, or paste its path. TrueType only: .ttc collections and .otf are refused by the renderer, and a change takes effect at the next start.", "把一枚.ttf拖进窗口,或贴上它的路径。只收TrueType:.ttc集合与.otf会被渲染器拒绝;更换在下次启动时生效。" },
    .{ "face_path", "Path to a .ttf", ".ttf的路径" },
    .{ "use_this_face", "Use this face", "使用这枚字面" },
    .{ "face_probing", "reading the face…", "正在读取字面……" },
    .{ "face_saved", "Saved. It takes effect the next time glass starts.", "已保存。下次启动glass时生效。" },
    .{ "face_refused", "The face chosen last time could not be used", "上次选的字面无法使用" },
    .{ "face_too_large", "the file is larger than 24 MiB, the most the renderer holds per face", "文件超过24 MiB,这是渲染器每枚字面的上限" },
    .{ "face_no_han", "the face has no glyph for 中", "这枚字面没有「中」的字形" },
    .{ "face_unreadable", "the file could not be read", "文件读不出来" },
    .{ "face_unsaved", "the choice could not be written beside the backups", "选择无法写到备份旁边" },
    .{ "maintenance", "Maintenance", "维护" },
    .{ "measure_a_host", "Measure a host", "测量宿主" },
    .{ "back_up_site", "Back up this site", "备份这个站点" },
    .{ "refresh", "Refresh", "刷新" },
    .{ "import_hint", "To restore a backup, run kusanagi import in a terminal: the key and the archive go in on stdin, and this window never holds either.", "恢复备份请在终端运行:kusanagi import。密钥与归档从标准输入进入,这个窗口从不持有它们。" },
    .{ "close", "Close", "关闭" },
    // the check card, the invitation, the join
    .{ "read_these", "Read these four characters to the other person. Theirs must be the same.", "把这四个字符念给对方听。对方的必须一样。" },
    .{ "check_code", "check code", "校验码" },
    .{ "copy", "Copy", "复制" },
    .{ "name", "Name", "名字" },
    .{ "name_placeholder", "What to call them here (a-z, 0-9, -)", "在这里怎么称呼对方(a-z、0-9、-)" },
    .{ "waypoint", "Waypoint", "Waypoint" },
    .{ "waypoint_placeholder", "Where the drops live: http://host:port, s3://bucket, or a path", "drop存放处:http://host:port、s3://bucket或一个路径" },
    .{ "slot_period", "Slot period", "时隙周期" },
    .{ "every_placeholder", "Slot period in seconds; leave empty to write when asked", "时隙周期(秒);留空则按需写入" },
    .{ "period_hint", "A period fills every slot, whether or not there is anything to say.", "设了周期,每个时隙都会被填满,不管有没有话要说。" },
    .{ "release_switch", "Release each drop once they have read it", "对方读过后即释放" },
    .{ "release_hint", "This site then becomes the only copy. Back it up.", "此后这个站点就是唯一副本。记得备份。" },
    .{ "cancel", "Cancel", "取消" },
    .{ "mint", "Mint", "生成" },
    .{ "invitation_hint", "The invitation. One line, one use, a bearer credential: it carries the channel secret, so hand it over the way you would a key.", "邀请。一行、一次性、持有即有效:它携带通道密钥,交出去时要像交钥匙一样。" },
    .{ "copy_invitation", "Copy the invitation", "复制邀请" },
    .{ "clipboard_note", "Copied. The clipboard is a log — Windows keeps a history of it and may sync it — so this window clears it again a minute from now, unless you have copied something else by then.", "已复制。剪贴板是一本日志——系统会记下历史,还可能同步到别处——所以一分钟后这里会替你清掉它,除非你已经复制了别的东西。" },
    .{ "done", "Done", "完成" },
    .{ "accept_invitation", "Accept an invitation", "接受邀请" },
    .{ "invitation", "Invitation", "邀请" },
    .{ "paste_invitation", "Paste the kusanagi2: line here", "把kusanagi2:那一行贴在这里" },
    .{ "stdin_hint", "The line never enters a shell history; it goes straight to the program.", "这一行不会进入命令行历史,只直接交给程序。" },
    .{ "join", "Join", "加入" },
    // backup
    .{ "backup", "Backup", "备份" },
    .{ "recovery_hint", "The recovery key. Shown once, kept nowhere. Whoever holds it and the archive holds this endpoint.", "恢复密钥。只显示一次,不存任何地方。谁持有它和归档,谁就持有这个端点。" },
    .{ "recovery_key", "recovery key", "恢复密钥" },
    .{ "copy_key", "Copy the key", "复制密钥" },
    .{ "archive_written", "Archive written", "归档已写入" },
    .{ "writing_archive", "writing the archive…", "正在写入归档……" },
    .{ "restore_hint", "To restore on another machine: `kusanagi import --root NEW`, with the key on the first line of stdin and the archive after it.", "换机恢复:在新机器运行 kusanagi import,第一行输入密钥,其后粘贴归档。" },
    .{ "export_body", "Seals the identity, every channel, every cairn and every roster into one file, under a key drawn now and shown once.", "把身份、每条通道和每份名册封进一个文件。密钥现在生成,只显示一次。" },
    .{ "export_hint", "A channel that releases its drops has no other copy anywhere. Back up after opening one, and after any conversation you would mind losing.", "开了阅后释放的通道,别处没有任何副本;之后记得备份。" },
    .{ "export_", "Export", "导出" },
    // welcome, groups, doctor, forget
    // The welcome page uses the reader's own words, never the protocol's: what
    // it is, what the server in between cannot do (content and who-with-whom
    // are the two facts ARCHITECTURE.md §3 proves), and the two ways in as
    // the reader's situation. Nothing here needs a glossary.
    .{ "tagline", "Private messages between two machines.", "两台机器之间的私信。" },
    .{ "tagline_body", "The server in between cannot read them, or tell who is talking to whom.", "经过的服务器读不到内容,也看不出谁在和谁说话。" },
    .{ "open_channel", "Invite someone", "邀请对方" },
    .{ "open_channel_body", "You get one line to hand over in person.", "你会得到一行邀请,当面交给对方。" },
    .{ "have_invitation", "Got an invitation?", "拿到了邀请?" },
    .{ "have_invitation_body", "Paste it in, then compare four characters together.", "贴进来,然后两人一起核对四个字符。" },
    .{ "shortcuts", "Ctrl+N invite · Ctrl+J join · Ctrl+B backup · Ctrl+, settings", "Ctrl+N 邀请 · Ctrl+J 加入 · Ctrl+B 备份 · Ctrl+, 设置" },
    .{ "no_binary_title", "kusanagi is not beside this window", "这个窗口旁边没有kusanagi" },
    .{ "no_binary_body", "Put the kusanagi binary next to glass, or on PATH, and start again.", "把kusanagi二进制放到glass旁边或PATH上,再启动一次。" },
    .{ "last_broadcast", "Last broadcast", "上次广播" },
    .{ "delivered", "delivered", "已送达" },
    .{ "failed", "failed", "失败" },
    .{ "edit_members", "Edit members", "编辑成员" },
    .{ "group_is_list", "A group is a list, not a room", "群组是一份名单,不是一个房间" },
    .{ "group_body", "Write to everybody on this list. Each gets their own copy on their own channel, sees nobody else, and replies in their own conversation.", "写给名单上的每个人。各走各的通道,谁也看不见别人;回复会出现在各自的对话里。" },
    .{ "broadcast", "Broadcast", "广播" },
    .{ "write_to_everybody", "Write to everybody on this list…", "写给名单上的每个人……" },
    .{ "send_to_all", "Send to all", "发给所有人" },
    .{ "group_members", "Group members", "群组成员" },
    .{ "group_name", "Group name", "群组名" },
    .{ "group_name_placeholder", "What to call the group (a-z, 0-9, -)", "群组叫什么(a-z、0-9、-)" },
    .{ "tick_hint", "Tick the channels this name stands for. Saving replaces the whole list; an empty list retires the group.", "勾选这个名字代表的通道。保存会替换整份名单;空名单即解散群组。" },
    .{ "no_channels_to_choose", "No channels to choose from yet.", "还没有可选的通道。" },
    .{ "save", "Save", "保存" },
    .{ "measure_hint", "Hosts are measured, not believed: this writes twice and reads back before naming a tier. Nothing about this project is sent.", "宿主靠测量,不靠相信:先写两次再读回,然后才定级。不会发送任何关于这个项目的信息。" },
    .{ "waypoint_short", "http://host:port, s3://bucket, or a path", "http://host:port、s3://bucket或一个路径" },
    .{ "measure", "Measure", "测量" },
    .{ "reaching_host", "reaching the host…", "正在连接宿主……" },
    .{ "tier", "Tier", "等级" },
    .{ "held", "held", "成立" },
    .{ "not_held", "not held", "不成立" },
    .{ "forget_question", "Forget this channel?", "忘掉这条通道?" },
    .{ "forget_body", "The channel secret is deleted here and nowhere else. The host keeps its bytes, the peer is not told, and no invitation can reopen it.", "通道密钥只在这里删除。宿主保留它的字节,对方不会被告知,任何邀请都无法重开它。" },
    .{ "keep_it", "Keep it", "保留" },
    .{ "forget", "Forget", "忘掉" },
    // cadence, in words
    .{ "writes_when_asked", "writes when asked", "按需写入" },
    .{ "every_hours", "one drop every {d} h", "每{d}小时一个drop" },
    .{ "every_minutes", "one drop every {d} min", "每{d}分钟一个drop" },
    .{ "every_seconds", "one drop every {d} s", "每{d}秒一个drop" },
    // notes and complaints of the window's own
    .{ "note_group_saved", "group saved", "群组已保存" },
    .{ "note_forgotten", "channel forgotten here; the host keeps its bytes", "通道已在本地忘掉;宿主保留它的字节" },
    .{ "note_revoked", "peer revoked; every later read refuses them", "对方已撤销;此后每次读取都会拒绝他们" },
    .{ "note_cut", "history too long to show whole; the newest part is here", "历史太长,无法完整显示;这里是最新的部分" },
    .{ "note_queued", "queued for the next slot", "已排入下一个时隙" },
    .{ "note_minted", "invitation minted; hand over the line and read the code aloud", "邀请已生成;交出那一行,并念出校验码" },
    .{ "note_joined", "joined; read the code aloud and compare", "已加入;念出校验码并核对" },
    .{ "note_slot_message", "slot filled with your message", "时隙已填入你的消息" },
    .{ "note_slot_filler", "slot filled", "时隙已填充" },
    .{ "note_slot_taken", "slot already filled", "时隙已被填过" },
    .{ "err_no_binary", "kusanagi could not be started", "kusanagi无法启动" },
    .{ "rec_no_binary", "put kusanagi beside glass, or on PATH", "把kusanagi放到glass旁边或PATH上" },
    .{ "err_busy", "that command was refused by the window", "这条命令被窗口拒绝" },
    .{ "rec_busy", "wait for the running command to finish", "等正在运行的命令结束" },
    .{ "err_cancelled", "the command was cancelled", "命令已取消" },
    .{ "err_killed", "kusanagi was killed", "kusanagi被终止" },
    .{ "rec_run_again", "run it again", "再运行一次" },
    .{ "err_unreadable", "kusanagi answered something that is not JSON", "kusanagi的应答不是JSON" },
    .{ "err_not_object", "the answer was not an object", "应答不是一个对象" },
    .{ "rec_terminal", "run the same verb in a terminal", "在终端运行同一个动词" },
    .{ "err_backup_unwritten", "the archive could not be written", "归档无法写入" },
    .{ "rec_backup_unwritten", "run `kusanagi export > backup.ksnb` in a terminal", "在终端运行`kusanagi export > backup.ksnb`" },
};

/// One field per table row, every one a `[]const u8`; the markup binds `{t.<key>}`.
pub const Strings = blk: {
    var names: [table.len][]const u8 = undefined;
    for (table, &names) |entry, *name| name.* = entry[0];
    const attrs: [table.len]std.builtin.Type.StructField.Attributes = @splat(.{});
    break :blk @Struct(.auto, null, &names, &@as([table.len]type, @splat([]const u8)), &attrs);
};

pub fn of(language: Language) Strings {
    var out: Strings = undefined;
    inline for (table) |entry| {
        @field(out, entry[0]) = switch (language) {
            .en => entry[1],
            .zh => entry[2],
        };
    }
    return out;
}

/// The file holding a person's own choice of language: one tag and nothing
/// else, beside the font preference and for the same reason (`font.zig`).
pub const preference_file = "kusanagi-glass.language";

/// The language chosen on the settings sheet last time, if it was.
pub fn remembered(io: std.Io, home: []const u8) ?Language {
    var path: [std.Io.Dir.max_path_bytes]u8 = undefined;
    const where = std.fmt.bufPrint(&path, "{s}{c}{s}", .{ home, std.fs.path.sep, preference_file }) catch return null;
    var held: [16]u8 = undefined;
    const written = std.Io.Dir.cwd().readFile(io, where, &held) catch return null;
    return std.meta.stringToEnum(Language, std.mem.trim(u8, written, " \t\r\n"));
}

/// The language the machine is set to, reduced to the two this table holds.
/// Windows keeps it in the user's UI language; elsewhere `LANG` says.
pub fn detect(environ: *const std.process.Environ.Map) Language {
    if (builtin.os.tag == .windows) {
        return if (windows.GetUserDefaultUILanguage() & 0x3ff == chinese_primary_language) .zh else .en;
    }
    const spoken = environ.get("LC_ALL") orelse environ.get("LANG") orelse "";
    return if (std.mem.startsWith(u8, spoken, "zh")) .zh else .en;
}

const chinese_primary_language: u16 = 0x04;

const windows = struct {
    extern "kernel32" fn GetUserDefaultUILanguage() callconv(.winapi) u16;
};

test "every sentence exists in both languages, and neither carries a control character" {
    inline for (table) |entry| {
        try std.testing.expect(entry[1].len > 0);
        try std.testing.expect(entry[2].len > 0);
        for (entry[1] ++ entry[2]) |c| try std.testing.expect(c >= 0x20);
    }
    const en = of(.en);
    const zh = of(.zh);
    try std.testing.expectEqualStrings("Send", en.send);
    try std.testing.expectEqualStrings("发送", zh.send);
}

test "a language is a column, never a lookup that can miss" {
    try std.testing.expectEqual(table.len, @typeInfo(Strings).@"struct".fields.len);
}
