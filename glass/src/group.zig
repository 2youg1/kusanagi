// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! A group as one thread: my broadcasts once, each member's replies under
//! the broadcast they answered, every reply labelled with who wrote it.
//!
//! **The protocol has no rooms** (D-17, F7). A group is a list of private
//! channels, and a broadcast is one copy per member on that member's channel.
//! This module merges what those N channels hold into a single view on this
//! machine and changes nothing on the wire: members still see nobody but me.
//!
//! Two facts do the merging. Within one channel, `order.zig` interleaves my
//! copy and the member's replies by what each acknowledged. Across channels,
//! my copies of one broadcast carry **the same text in the same order** on
//! every member's lane, so equal text in lane order is one broadcast; a reply
//! hangs under the last of my copies that precedes it on its own channel.

const std = @import("std");
const order = @import("order.zig");
const rows = @import("rows.zig");

pub const Text = rows.Text;
/// A member's window is smaller than a channel's: N of them live in the model at once.
pub const window = 32;
pub const Lane = rows.LaneOf(window);

/// One member of the open group: their channel, what to call them, both lanes.
pub const Member = struct {
    name: Text(rows.name_cap) = .{},
    /// What the rail calls them: the alias they signed, or their short handle.
    label: Text(rows.name_cap) = .{},
    has_peer: bool = false,
    mine: Lane = .{},
    theirs: Lane = .{},
};

/// The thread of the open group, and where the round-robin poll stands.
pub const Thread = struct {
    members: [rows.max_members]Member = @splat(.{}),
    count: usize = 0,
    /// The stdin of the in-flight group read, owned here instead of the
    /// model's shared `name_scratch`: each member's read rewrites it in
    /// place, and every executor that holds the slice rather than a copy
    /// would otherwise see every request arrive with one name.
    stdin: Text(rows.name_cap + 1) = .{},
    /// Which member the next poll reads. One member per poll, so a window on
    /// a group of thirty makes the same rhythm as one on a channel (I3).
    cursor: usize = 0,
    /// How many members are still to be read in the round that follows a
    /// broadcast or an opening; zero means the timer alone drives the cursor.
    catching_up: usize = 0,

    pub fn clear(thread: *Thread) void {
        thread.count = 0;
        thread.cursor = 0;
        thread.catching_up = 0;
    }
    pub fn current(thread: *Thread) *Member {
        return &thread.members[@min(thread.cursor, rows.max_members - 1)];
    }
    /// Moves the cursor on; true while a catch-up round still has members left.
    pub fn advance(thread: *Thread) bool {
        if (thread.count == 0) return false;
        thread.cursor = (thread.cursor + 1) % thread.count;
        if (thread.catching_up > 0) thread.catching_up -= 1;
        return thread.catching_up > 0;
    }
    pub fn all(thread: *const Thread) []const Member {
        return thread.members[0..thread.count];
    }
};

/// One bubble of the merged thread.
pub const Bubble = struct {
    key: u64,
    mine: bool,
    /// The speaker changed, so the gap before this bubble widens.
    turn: bool,
    /// Who wrote it: a member's label, or empty for my own.
    who: []const u8,
    text: []const u8,
    is_hex: bool,
    cut: bool,
    /// For my own: how many members' lanes hold this copy, out of how many.
    reached: usize,
    of: usize,
};

/// A reply and the broadcast it follows, before the two are laid out.
const Placed = struct { member: usize, item: usize, after: usize };

/// Lays the thread out into `out`, returning how many bubbles were written.
///
/// Bounded work: at most `window` broadcasts and `members.len * window` replies.
pub fn merge(members: []const Member, out: []Bubble) usize {
    // My broadcasts in order, each remembering how many lanes carried it.
    var texts: [window][]const u8 = undefined;
    var hex: [window]bool = undefined;
    var cut: [window]bool = undefined;
    var reached: [window]usize = @splat(0);
    var broadcasts: usize = 0;
    // Every reply, hung under the broadcast preceding it on its own channel.
    var placed: [rows.max_members * window]Placed = undefined;
    var replies: usize = 0;

    for (members, 0..) |member, k| {
        var sides: [window * 2]order.Side = undefined;
        const n = order.merge(rows.Message, member.mine.all(), member.theirs.all(), &sides);
        var taken: [2]usize = .{ 0, 0 };
        // Which broadcast the next reply on this channel hangs under.
        var after: usize = 0;
        for (sides[0..n]) |side| {
            const item = taken[@intFromEnum(side)];
            taken[@intFromEnum(side)] += 1;
            switch (side) {
                .mine => {
                    const text = member.mine.items[item].text.slice();
                    // Equal text in lane order is one broadcast; scan forward
                    // from the last match so a repeated sentence stays two.
                    var found: ?usize = null;
                    var at = after;
                    while (at < broadcasts) : (at += 1) {
                        if (std.mem.eql(u8, texts[at], text)) {
                            found = at;
                            break;
                        }
                    }
                    const slot = found orelse blk: {
                        if (broadcasts == window) break :blk null;
                        texts[broadcasts] = text;
                        hex[broadcasts] = member.mine.items[item].is_hex;
                        cut[broadcasts] = member.mine.items[item].text.cut;
                        broadcasts += 1;
                        break :blk broadcasts - 1;
                    } orelse continue;
                    reached[slot] += 1;
                    after = slot + 1;
                },
                .theirs => {
                    if (replies == placed.len) continue;
                    placed[replies] = .{ .member = k, .item = item, .after = after };
                    replies += 1;
                },
            }
        }
    }

    var n: usize = 0;
    var last_who: ?usize = null; // null = me; a member index otherwise
    var first = true;
    var slot: usize = 0;
    while (slot <= broadcasts and n < out.len) : (slot += 1) {
        if (slot > 0) {
            const b = slot - 1;
            out[n] = .{
                .key = @as(u64, b) * 2,
                .mine = true,
                .turn = first or last_who != null,
                .who = "",
                .text = texts[b],
                .is_hex = hex[b],
                .cut = cut[b],
                .reached = reached[b],
                .of = members.len,
            };
            n += 1;
            last_who = null;
            first = false;
        }
        for (placed[0..replies]) |reply| {
            if (reply.after != slot or n == out.len) continue;
            const member = &members[reply.member];
            const message = &member.theirs.items[reply.item];
            out[n] = .{
                .key = (@as(u64, reply.member) << 32 | message.index) * 2 + 1,
                .mine = false,
                .turn = first or last_who != reply.member,
                .who = member.label.slice(),
                .text = message.text.slice(),
                .is_hex = message.is_hex,
                .cut = message.text.cut,
                .reached = 0,
                .of = 0,
            };
            n += 1;
            last_who = reply.member;
            first = false;
        }
    }
    return n;
}

fn said(lane: *Lane, index: u64, acknowledged: u64, text: []const u8) void {
    var message: rows.Message = .{ .index = index, .acknowledged = acknowledged };
    message.text.set(text);
    lane.push(message);
}

test "one broadcast, two replies: me once, then A, then B, each labelled" {
    var a: Member = .{};
    a.label.set("Alice");
    said(&a.mine, 0, 0, "lunch?");
    said(&a.theirs, 0, 1, "yes");
    var b: Member = .{};
    b.label.set("Bob");
    said(&b.mine, 0, 0, "lunch?");
    said(&b.theirs, 0, 1, "no");
    var out: [8]Bubble = undefined;
    const n = merge(&.{ a, b }, &out);
    try std.testing.expectEqual(@as(usize, 3), n);
    try std.testing.expect(out[0].mine);
    try std.testing.expectEqualStrings("lunch?", out[0].text);
    try std.testing.expectEqual(@as(usize, 2), out[0].reached);
    try std.testing.expectEqual(@as(usize, 2), out[0].of);
    try std.testing.expectEqualStrings("Alice", out[1].who);
    try std.testing.expectEqualStrings("yes", out[1].text);
    try std.testing.expectEqualStrings("Bob", out[2].who);
    try std.testing.expect(out[1].turn and out[2].turn);
}

test "a reply written before a broadcast reached its author stays above it" {
    var a: Member = .{};
    a.label.set("Alice");
    said(&a.theirs, 0, 0, "hello?");
    said(&a.mine, 0, 1, "all: meeting at nine");
    said(&a.theirs, 1, 1, "fine");
    var b: Member = .{};
    b.label.set("Bob");
    said(&b.mine, 0, 0, "all: meeting at nine");
    var out: [8]Bubble = undefined;
    const n = merge(&.{ a, b }, &out);
    try std.testing.expectEqual(@as(usize, 3), n);
    try std.testing.expectEqualStrings("hello?", out[0].text);
    try std.testing.expect(out[1].mine);
    try std.testing.expectEqualStrings("fine", out[2].text);
}

test "a broadcast one member never got is counted as reaching the others only" {
    var a: Member = .{};
    said(&a.mine, 0, 0, "first");
    said(&a.mine, 1, 0, "second");
    var b: Member = .{};
    said(&b.mine, 0, 0, "first");
    var out: [8]Bubble = undefined;
    const n = merge(&.{ a, b }, &out);
    try std.testing.expectEqual(@as(usize, 2), n);
    try std.testing.expectEqual(@as(usize, 2), out[0].reached);
    try std.testing.expectEqual(@as(usize, 1), out[1].reached);
}

test "the same sentence sent twice is two broadcasts" {
    var a: Member = .{};
    said(&a.mine, 0, 0, "ping");
    said(&a.mine, 1, 0, "ping");
    var out: [8]Bubble = undefined;
    try std.testing.expectEqual(@as(usize, 2), merge(&.{a}, &out));
}

test "the cursor walks the members and a catch-up round ends by itself" {
    var thread: Thread = .{};
    thread.count = 3;
    thread.catching_up = 3;
    try std.testing.expect(thread.advance());
    try std.testing.expect(thread.advance());
    try std.testing.expect(!thread.advance());
    try std.testing.expectEqual(@as(usize, 0), thread.cursor);
}
