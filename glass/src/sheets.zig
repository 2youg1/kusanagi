// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! The state behind each sheet, one struct per sheet, bound from the markup
//! by nested path (`{invite.nameText}`). A sheet holds what its fields
//! type and what its verb answered; the model holds which sheet is open.

const std = @import("std");
const native_sdk = @import("native_sdk");
const canvas = native_sdk.canvas;
const rows = @import("rows.zig");

const Text = rows.Text;

/// Mint an invitation: who, where, and on what habit.
pub const Invite = struct {
    name: canvas.TextBuffer(rows.name_cap) = .{},
    waypoint: canvas.TextBuffer(rows.line_cap) = .{},
    every: canvas.TextBuffer(8) = .{},
    release: bool = false,
    line: Text(rows.invite_cap) = .{},

    pub fn nameText(s: *const Invite) []const u8 {
        return s.name.text();
    }
    pub fn waypointText(s: *const Invite) []const u8 {
        return s.waypoint.text();
    }
    pub fn everyText(s: *const Invite) []const u8 {
        return s.every.text();
    }
    pub fn minted(s: *const Invite) bool {
        return !s.line.isEmpty();
    }
    pub fn lineText(s: *const Invite) []const u8 {
        return s.line.slice();
    }
    pub fn period(s: *const Invite) ?u32 {
        return std.fmt.parseInt(u32, s.every.text(), 10) catch null;
    }
    pub fn ready(s: *const Invite) bool {
        return s.name.text().len > 0 and s.waypoint.text().len > 0;
    }
};

/// Accept an invitation somebody handed over.
pub const Join = struct {
    name: canvas.TextBuffer(rows.name_cap) = .{},
    invitation: canvas.TextBuffer(rows.invite_cap) = .{},
    release: bool = false,

    pub fn nameText(s: *const Join) []const u8 {
        return s.name.text();
    }
    pub fn invitationText(s: *const Join) []const u8 {
        return s.invitation.text();
    }
    pub fn ready(s: *const Join) bool {
        return s.name.text().len > 0 and s.invitation.text().len > 0;
    }
};

/// Export the site: the archive on its way to disk, the key shown once.
pub const Backup = struct {
    recovery: Text(80) = .{},
    path: Text(rows.path_cap) = .{},
    written: bool = false,
    archive: [512 * 1024]u8 = undefined,
    archive_len: usize = 0,

    pub fn hasRecovery(s: *const Backup) bool {
        return !s.recovery.isEmpty();
    }
    pub fn recoveryKey(s: *const Backup) []const u8 {
        return s.recovery.slice();
    }
    pub fn pathText(s: *const Backup) []const u8 {
        return s.path.slice();
    }
    pub fn bytes(s: *const Backup) []const u8 {
        return s.archive[0..s.archive_len];
    }
    pub fn keep(s: *Backup, recovery: []const u8, archive: []const u8) void {
        s.recovery.set(recovery);
        const n = @min(archive.len, s.archive.len);
        @memcpy(s.archive[0..n], archive[0..n]);
        s.archive_len = n;
        s.written = false;
    }
};

/// Edit one group's roster: a name over a tick per channel.
pub const Roster = struct {
    name: canvas.TextBuffer(rows.name_cap) = .{},
    members: [rows.max_channels]rows.CheckRow = @splat(.{}),

    pub fn nameText(s: *const Roster) []const u8 {
        return s.name.text();
    }
    pub fn ready(s: *const Roster) bool {
        return s.name.text().len > 0;
    }
    /// The channels the sheet offers, freshly listed from what `channels`
    /// answered, with the group's current members already ticked.
    pub fn prefill(s: *Roster, channels: []const rows.ChannelRow, group: ?*const rows.GroupRow) void {
        for (channels, 0..) |channel, i| {
            s.members[i] = .{ .slot = i };
            s.members[i].name.set(channel.name.slice());
        }
        const g = group orelse {
            s.name.clear();
            return;
        };
        s.name.set(g.name.slice());
        for (g.members[0..g.count]) |member| {
            for (s.members[0..channels.len]) |*row| {
                if (row.name.eql(member.slice())) row.checked = true;
            }
        }
    }
    pub fn toggle(s: *Roster, slot: usize, count: usize) void {
        if (slot < count) s.members[slot].checked = !s.members[slot].checked;
    }
    /// The ticked names, for the verb's stdin.
    pub fn ticked(s: *const Roster, count: usize, out: *[rows.max_channels][]const u8) []const []const u8 {
        var n: usize = 0;
        for (s.members[0..count]) |row| {
            if (!row.checked) continue;
            out[n] = row.name.slice();
            n += 1;
        }
        return out[0..n];
    }
};

/// Measure a host: the waypoint asked about, the tier and findings answered.
pub const Doctor = struct {
    waypoint: canvas.TextBuffer(rows.line_cap) = .{},
    measured: Text(rows.line_cap) = .{},
    tier: Text(rows.code_cap) = .{},
    findings: [rows.max_rows]rows.CheckRow = @splat(.{}),
    finding_count: usize = 0,

    pub fn waypointText(s: *const Doctor) []const u8 {
        return s.waypoint.text();
    }
    pub fn ready(s: *const Doctor) bool {
        return s.waypoint.text().len > 0;
    }
    pub fn hasTier(s: *const Doctor) bool {
        return !s.tier.isEmpty();
    }
    pub fn tierName(s: *const Doctor) []const u8 {
        return s.tier.slice();
    }
    /// The waypoint the tier speaks for, whole, the way the verb echoed it.
    pub fn measuredText(s: *const Doctor) []const u8 {
        return s.measured.slice();
    }
    pub fn clear(s: *Doctor) void {
        s.tier.clear();
        s.measured.clear();
        s.finding_count = 0;
    }
};
