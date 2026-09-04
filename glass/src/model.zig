// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! What the window knows, and nothing the site does not.
//!
//! Every field here is either a copy of something `kusanagi --json` answered
//! or the state of a control. Nothing is written to disk; closing the window
//! loses at most what a person had typed. The public methods are what the
//! markup binds: derived, never stored (`native-ui.md` "Derive, don't store").

const std = @import("std");
const native_sdk = @import("native_sdk");
const canvas = native_sdk.canvas;
const order = @import("order.zig");
const rows = @import("rows.zig");
const sheets = @import("sheets.zig");

pub const Text = rows.Text;
pub const ChannelRow = rows.ChannelRow;
pub const GroupRow = rows.GroupRow;
pub const Message = rows.Message;
pub const Lane = rows.Lane;
pub const Bubble = rows.Bubble;
pub const Status = rows.Status;
pub const CheckRow = rows.CheckRow;
pub const name_cap = rows.name_cap;
pub const path_cap = rows.path_cap;
pub const draft_cap = rows.draft_cap;
pub const max_channels = rows.max_channels;
pub const max_groups = rows.max_groups;
pub const max_members = rows.max_members;
pub const max_messages = rows.max_messages;
pub const max_rows = rows.max_rows;

pub const Screen = enum { welcome, thread, group };
pub const Sheet = enum { none, invite, join, backup, roster, doctor, forget, settings };
/// The colour scheme a person chose over the one the system reports.
pub const Look = enum { system, light, dark };

pub const Model = struct {
    bin: Text(path_cap) = .{},
    home: Text(path_cap) = .{},
    appearance: native_sdk.Appearance = .{},
    look: Look = .system,
    screen: Screen = .welcome,
    sheet: Sheet = .none,
    busy: u64 = 0,

    channels: [max_channels]ChannelRow = @splat(.{}),
    channel_count: usize = 0,
    groups: [max_groups]GroupRow = @splat(.{}),
    group_count: usize = 0,
    selected: usize = 0,
    selected_group: usize = 0,

    mine: Lane = .{},
    theirs: Lane = .{},
    draft: canvas.TextBuffer(draft_cap) = .{},
    search: canvas.TextBuffer(name_cap) = .{},
    status: Status = .{},
    output_cut: bool = false,
    scratch: [4096]u8 = undefined,
    name_scratch: [40]u8 = undefined,

    // `doctor --here` and `id`
    site: Text(path_cap) = .{},
    at_rest: Text(16) = .{},
    proxy: bool = false,
    binary: Text(80) = .{},
    handle: Text(80) = .{},

    // the check card both invitation sheets share: the last code minted or joined
    check: Text(8) = .{},
    check_for: Text(name_cap) = .{},

    invite: sheets.Invite = .{},
    join: sheets.Join = .{},
    backup: sheets.Backup = .{},
    roster: sheets.Roster = .{},
    doctor: sheets.Doctor = .{},

    // the last broadcast, one row per member
    delivered: [max_rows]CheckRow = @splat(.{}),
    delivered_count: usize = 0,

    /// The three looks the endpoint page offers, in the order it lists them.
    pub const looks = [_]Look{ .system, .light, .dark };

    /// State the view never binds directly: `update`, the effects and the
    /// chrome read it, and the methods below derive what the markup shows.
    pub const view_unbound = .{
        "bin",         "home",       "appearance",      "screen",
        "sheet",       "busy",       "channels",        "channel_count", "groups",
        "group_count", "mine",       "theirs",          "draft",         "search",
        "status",      "output_cut", "scratch",         "name_scratch",  "site",
        "at_rest",     "proxy",      "binary",          "handle",        "check",
        "check_for",   "delivered",  "delivered_count", "channelRows",   "onThread",
        "canSend",       "currentWaypoint",
    };

    /// The appearance the tokens resolve from: the system's, unless a look
    /// was chosen on the endpoint page.
    pub fn appearanceFor(m: *const Model) native_sdk.Appearance {
        var chosen = m.appearance;
        switch (m.look) {
            .system => {},
            .light => chosen.color_scheme = .light,
            .dark => chosen.color_scheme = .dark,
        }
        return chosen;
    }

    // ------------------------------------------------------------ the rail

    pub fn channelRows(m: *const Model) []const ChannelRow {
        return m.channels[0..m.channel_count];
    }
    /// Every channel whose name contains what was typed.
    pub fn visibleChannels(m: *const Model, arena: std.mem.Allocator) []const ChannelRow {
        const needle = m.search.text();
        if (needle.len == 0) return m.channelRows();
        var out = arena.alloc(ChannelRow, m.channel_count) catch return m.channelRows();
        var n: usize = 0;
        for (m.channelRows()) |row| {
            if (std.mem.indexOf(u8, row.name.slice(), needle) == null) continue;
            out[n] = row;
            n += 1;
        }
        return out[0..n];
    }
    pub fn searchText(m: *const Model) []const u8 {
        return m.search.text();
    }
    pub fn groupRows(m: *const Model) []const GroupRow {
        return m.groups[0..m.group_count];
    }
    pub fn isBusy(m: *const Model) bool {
        return m.busy != 0;
    }
    /// Whether the rail carries the two doors at its bottom: the welcome
    /// hero owns them while there is nothing to list.
    pub fn railDoors(m: *const Model) bool {
        return m.onThread() or m.onGroup();
    }
    pub fn noBinary(m: *const Model) bool {
        return m.bin.isEmpty();
    }

    // ------------------------------------------------------------ the settings sheet

    /// The handle in groups of eight, the way a fingerprint is read across a
    /// table; the paragraph wraps at the group boundaries.
    pub fn handleBlock(m: *const Model, arena: std.mem.Allocator) []const u8 {
        const handle = m.handle.slice();
        if (handle.len == 0) return "not answered yet";
        const groups = (handle.len + 7) / 8;
        const out = arena.alloc(u8, handle.len + groups) catch return handle;
        var n: usize = 0;
        for (handle, 0..) |c, i| {
            if (i > 0 and i % 8 == 0) {
                out[n] = ' ';
                n += 1;
            }
            out[n] = c;
            n += 1;
        }
        return out[0..n];
    }
    pub fn siteShown(m: *const Model) []const u8 {
        return if (m.site.isEmpty()) "not answered yet" else m.site.slice();
    }
    pub fn sealShown(m: *const Model) []const u8 {
        return if (m.at_rest.isEmpty()) "not answered yet" else m.at_rest.slice();
    }
    pub fn routeShown(m: *const Model) []const u8 {
        return if (m.site.isEmpty()) "not answered yet" else if (m.proxy) "through the proxy" else "direct";
    }
    pub fn binaryShown(m: *const Model) []const u8 {
        return if (m.bin.isEmpty()) "not found" else m.bin.slice();
    }

    // ------------------------------------------------------------ the plate

    pub fn current(m: *const Model) *const ChannelRow {
        return &m.channels[@min(m.selected, max_channels - 1)];
    }
    pub fn currentGroup(m: *const Model) *const GroupRow {
        return &m.groups[@min(m.selected_group, max_groups - 1)];
    }
    pub fn onThread(m: *const Model) bool {
        return m.screen == .thread and m.channel_count > 0;
    }
    pub fn onGroup(m: *const Model) bool {
        return m.screen == .group and m.group_count > 0;
    }
    pub fn onWelcome(m: *const Model) bool {
        return !m.onThread() and !m.onGroup();
    }
    pub fn currentInitials(m: *const Model) []const u8 {
        return m.current().initials();
    }
    pub fn currentName(m: *const Model) []const u8 {
        return m.current().name.slice();
    }
    pub fn currentPeer(m: *const Model) []const u8 {
        return m.current().peerShown();
    }
    pub fn currentWaypoint(m: *const Model) []const u8 {
        return m.current().waypoint.slice();
    }
    pub fn currentReleases(m: *const Model) bool {
        return m.onThread() and m.current().releases;
    }
    pub fn currentCadence(m: *const Model, arena: std.mem.Allocator) []const u8 {
        return m.current().cadence(arena);
    }
    pub fn currentVoid(m: *const Model) bool {
        return m.onThread() and m.current().isVoid();
    }
    pub fn currentRefusal(m: *const Model) []const u8 {
        const row = m.current();
        return if (!row.refused.isEmpty()) row.refused.slice() else row.peer_refused.slice();
    }
    pub fn waitingForPeer(m: *const Model) bool {
        return m.onThread() and !m.current().hasPeer();
    }
    pub fn theirHeight(m: *const Model) u64 {
        return if (m.theirs.height) |h| h + 1 else 0;
    }
    pub fn myHeight(m: *const Model) u64 {
        return if (m.mine.height) |h| h + 1 else 0;
    }
    pub fn canSend(m: *const Model) bool {
        const row = m.current();
        return m.onThread() and row.hasPeer() and row.can_send and !m.isBusy() and m.draft.text().len > 0;
    }
    pub fn sendDisabled(m: *const Model) bool {
        return !m.canSend();
    }
    pub fn composerDisabled(m: *const Model) bool {
        return !m.onThread() or !m.current().hasPeer() or !m.current().can_send;
    }
    pub fn draftText(m: *const Model) []const u8 {
        return m.draft.text();
    }
    pub fn groupInitials(m: *const Model) []const u8 {
        return m.currentGroup().initials();
    }
    pub fn groupTitle(m: *const Model) []const u8 {
        return m.currentGroup().name.slice();
    }
    pub fn groupMembers(m: *const Model, arena: std.mem.Allocator) []const u8 {
        return m.currentGroup().memberList(arena);
    }
    pub fn groupSize(m: *const Model) usize {
        return m.currentGroup().count;
    }
    pub fn deliveredRows(m: *const Model) []const CheckRow {
        return m.delivered[0..m.delivered_count];
    }
    pub fn hasDelivery(m: *const Model) bool {
        return m.delivered_count > 0;
    }

    /// The thread as it is shown: both lanes in the order `order.zig`
    /// derives, each bubble knowing whether it opens a new turn.
    pub fn thread(m: *const Model, arena: std.mem.Allocator) []const Bubble {
        var sides: [max_messages * 2]order.Side = undefined;
        const n = order.merge(Message, m.mine.all(), m.theirs.all(), &sides);
        const bubbles = arena.alloc(Bubble, n) catch return &.{};
        var taken: [2]usize = .{ 0, 0 };
        for (sides[0..n], 0..) |side, k| {
            const lane = if (side == .mine) &m.mine else &m.theirs;
            const source = &lane.items[taken[@intFromEnum(side)]];
            taken[@intFromEnum(side)] += 1;
            bubbles[k] = .{
                .key = source.index * 2 + @intFromEnum(side),
                .mine = side == .mine,
                .turn = k == 0 or sides[k - 1] != side,
                .text = source.text.slice(),
                .is_hex = source.is_hex,
                .cut = source.text.cut,
            };
        }
        return bubbles;
    }

    // ------------------------------------------------------------ status and sheets

    pub fn hasStatus(m: *const Model) bool {
        return !m.status.code.isEmpty() or !m.status.note.isEmpty();
    }
    pub fn statusIsError(m: *const Model) bool {
        return !m.status.code.isEmpty();
    }
    pub fn statusCode(m: *const Model) []const u8 {
        return m.status.code.slice();
    }
    pub fn statusText(m: *const Model) []const u8 {
        return if (m.statusIsError()) m.status.error_text.slice() else m.status.note.slice();
    }
    pub fn statusRecover(m: *const Model) []const u8 {
        return m.status.recover.slice();
    }
    pub fn hasCheck(m: *const Model) bool {
        return !m.check.isEmpty();
    }
    /// The four characters with a space between each, the way they are read aloud.
    pub fn checkSpaced(m: *const Model, arena: std.mem.Allocator) []const u8 {
        const code = m.check.slice();
        const out = arena.alloc(u8, code.len * 2) catch return code;
        for (code, 0..) |c, i| {
            out[i * 2] = c;
            out[i * 2 + 1] = ' ';
        }
        return std.mem.trimEnd(u8, out, " ");
    }
    pub fn checkFor(m: *const Model) []const u8 {
        return m.check_for.slice();
    }
    pub fn rosterRows(m: *const Model) []const CheckRow {
        return m.roster.members[0..m.channel_count];
    }
    pub fn findingRows(m: *const Model) []const CheckRow {
        return m.doctor.findings[0..m.doctor.finding_count];
    }
    pub fn sheetInvite(m: *const Model) bool {
        return m.sheet == .invite;
    }
    pub fn sheetJoin(m: *const Model) bool {
        return m.sheet == .join;
    }
    pub fn sheetBackup(m: *const Model) bool {
        return m.sheet == .backup;
    }
    pub fn sheetRoster(m: *const Model) bool {
        return m.sheet == .roster;
    }
    pub fn sheetDoctor(m: *const Model) bool {
        return m.sheet == .doctor;
    }
    pub fn sheetForget(m: *const Model) bool {
        return m.sheet == .forget;
    }
    pub fn sheetSettings(m: *const Model) bool {
        return m.sheet == .settings;
    }
};
