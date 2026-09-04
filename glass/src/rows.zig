// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! The bounded records the window keeps: what a verb answered, cut to fit.
//!
//! Every type here has a default for every field, so the model that holds
//! arrays of them can be created in place (`UiApp.create`) without a
//! constructor, and every string is a fixed buffer with a length — nothing
//! here allocates, so nothing here can grow with the work (law 2).

const std = @import("std");

pub const name_cap = 32;
pub const handle_cap = 12;
pub const code_cap = 48;
pub const line_cap = 256;
pub const text_cap = 512;
pub const path_cap = 1024;
pub const invite_cap = 640;
pub const draft_cap = 3584;
pub const max_channels = 64;
pub const max_groups = 16;
pub const max_members = 32;
pub const max_messages = 128;
pub const max_rows = 32;

/// A bounded string with a default, so the model can be `create`d.
pub fn Text(comptime cap: usize) type {
    return struct {
        buf: [cap]u8 = undefined,
        len: usize = 0,
        cut: bool = false,

        const Self = @This();

        pub fn slice(self: *const Self) []const u8 {
            return self.buf[0..self.len];
        }

        pub fn set(self: *Self, value: []const u8) void {
            const n = @min(value.len, cap);
            @memcpy(self.buf[0..n], value[0..n]);
            self.len = n;
            self.cut = value.len > cap;
        }

        pub fn clear(self: *Self) void {
            self.len = 0;
            self.cut = false;
        }

        pub fn isEmpty(self: *const Self) bool {
            return self.len == 0;
        }

        pub fn eql(self: *const Self, other: []const u8) bool {
            return std.mem.eql(u8, self.slice(), other);
        }
    };
}

/// One channel as `channels` reports it.
pub const ChannelRow = struct {
    slot: usize = 0,
    name: Text(name_cap) = .{},
    waypoint: Text(line_cap) = .{},
    peer: Text(handle_cap) = .{},
    root: bool = false,
    period: u32 = 0,
    releases: bool = false,
    can_send: bool = false,
    refused: Text(code_cap) = .{},
    peer_refused: Text(code_cap) = .{},
    expires_in: u64 = 0,

    pub fn title(row: *const ChannelRow) []const u8 {
        return row.name.slice();
    }
    pub fn hasPeer(row: *const ChannelRow) bool {
        return !row.peer.isEmpty();
    }
    /// The one second line a rail row earns: the peer has not arrived yet.
    /// Cadence and retention speak through their icons; nothing else here.
    pub fn waiting(row: *const ChannelRow) bool {
        return !row.hasPeer();
    }
    pub fn slotted(row: *const ChannelRow) bool {
        return row.period != 0;
    }
    pub fn isVoid(row: *const ChannelRow) bool {
        return !row.refused.isEmpty() or !row.peer_refused.isEmpty();
    }
    pub fn initials(row: *const ChannelRow) []const u8 {
        return row.name.slice()[0..@min(row.name.len, 2)];
    }
};

/// One group as `channels` reports it: a name over a list of channel names.
pub const GroupRow = struct {
    slot: usize = 0,
    name: Text(name_cap) = .{},
    members: [max_members]Text(name_cap) = @splat(.{}),
    count: usize = 0,

    pub fn title(row: *const GroupRow) []const u8 {
        return row.name.slice();
    }
    pub fn initials(row: *const GroupRow) []const u8 {
        return row.name.slice()[0..@min(row.name.len, 2)];
    }
    pub fn memberList(row: *const GroupRow, arena: std.mem.Allocator) []const u8 {
        var w = std.Io.Writer.Allocating.init(arena);
        for (row.members[0..row.count], 0..) |member, i| {
            if (i > 0) w.writer.writeAll(", ") catch break;
            w.writer.writeAll(member.slice()) catch break;
        }
        return w.written();
    }
};

/// One segment, as read or as just sent.
pub const Message = struct {
    index: u64 = 0,
    acknowledged: u64 = 0,
    text: Text(text_cap) = .{},
    is_hex: bool = false,
};

/// One author's window onto their stream: the newest `max_messages`.
pub const Lane = struct {
    items: [max_messages]Message = @splat(.{}),
    count: usize = 0,
    height: ?u64 = null,

    pub fn push(lane: *Lane, message: Message) void {
        if (lane.count == max_messages) {
            std.mem.copyForwards(Message, lane.items[0 .. max_messages - 1], lane.items[1..max_messages]);
            lane.count -= 1;
        }
        lane.items[lane.count] = message;
        lane.count += 1;
    }
    pub fn all(lane: *const Lane) []const Message {
        return lane.items[0..lane.count];
    }
    pub fn clear(lane: *Lane) void {
        lane.count = 0;
        lane.height = null;
    }
};

/// One bubble in the thread, built per rebuild from the two lanes.
pub const Bubble = struct {
    /// Unique across both lanes, which a segment's index alone is not.
    key: u64,
    mine: bool,
    /// The first bubble of a run: the speaker changed, so the gap before it widens.
    turn: bool,
    text: []const u8,
    is_hex: bool,
    cut: bool,
};

/// What went wrong last, in the words the verb used.
pub const Status = struct {
    code: Text(code_cap) = .{},
    error_text: Text(line_cap) = .{},
    recover: Text(line_cap) = .{},
    note: Text(line_cap) = .{},

    pub fn clear(status: *Status) void {
        status.code.clear();
        status.error_text.clear();
        status.recover.clear();
        status.note.clear();
    }
};

/// One member row of a group being edited, or one capability of a host.
pub const CheckRow = struct {
    slot: usize = 0,
    name: Text(name_cap) = .{},
    note: Text(line_cap) = .{},
    checked: bool = false,

    pub fn title(row: *const CheckRow) []const u8 {
        return row.name.slice();
    }
    pub fn detail(row: *const CheckRow) []const u8 {
        return row.note.slice();
    }
    /// Whether the detail line is worth its own row under the title.
    pub fn hasNote(row: *const CheckRow) bool {
        return !row.note.isEmpty();
    }
};
