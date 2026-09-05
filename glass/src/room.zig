// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! A room as one thread: my stream and every member's, in the order they were
//! filed, and the stdin one `room-read` takes.
//!
//! Apart from `group.zig` because a room is not a fan-out: nothing is
//! duplicated across lanes, so nothing is deduplicated here, and the order
//! across N authors is not causal but the period each segment was filed in —
//! the one clock the host already sees. Within a period a lower index comes
//! first and my own line comes last, so a reply never shows above what it
//! answers when both landed in one ten-minute bin.

const std = @import("std");
const group = @import("group.zig");
const rows = @import("rows.zig");

const Bubble = group.Bubble;
const Lane = group.Lane;
const Member = group.Member;

/// One segment of one lane, and where it sorts.
const Placed = struct {
    /// Which lane: `null` for mine, a member index otherwise.
    member: ?usize,
    item: usize,
    filed: u64,
    index: u64,

    fn before(_: void, a: Placed, b: Placed) bool {
        if (a.filed != b.filed) return a.filed < b.filed;
        if (a.index != b.index) return a.index < b.index;
        // Theirs before mine at equal period and height.
        return a.member != null and b.member == null;
    }
};

/// Lays the room out into `out`, returning how many bubbles were written.
///
/// Bounded work: at most `window * (1 + members.len)` segments, sorted once.
pub fn merge(me: *const Lane, members: []const Member, out: []Bubble) usize {
    var placed: [group.window * (1 + rows.max_members)]Placed = undefined;
    var n: usize = 0;
    for (me.all(), 0..) |message, item| {
        placed[n] = .{ .member = null, .item = item, .filed = message.filed, .index = message.index };
        n += 1;
    }
    for (members, 0..) |member, k| {
        for (member.theirs.all(), 0..) |message, item| {
            if (n == placed.len) break;
            placed[n] = .{ .member = k, .item = item, .filed = message.filed, .index = message.index };
            n += 1;
        }
    }
    std.mem.sort(Placed, placed[0..n], {}, Placed.before);

    var written: usize = 0;
    var last: ?usize = null;
    for (placed[0..n], 0..) |place, k| {
        if (written == out.len) break;
        const message = if (place.member) |m| &members[m].theirs.items[place.item] else &me.items[place.item];
        const who = if (place.member) |m| members[m].label.slice() else "";
        out[written] = .{
            .key = (@as(u64, if (place.member) |m| m + 1 else 0) << 32 | message.index),
            .mine = place.member == null,
            .turn = k == 0 or last != place.member,
            .who = who,
            .text = message.text.slice(),
            .is_hex = message.is_hex,
            .cut = message.text.cut,
            .reached = 0,
            .of = 0,
        };
        written += 1;
        last = place.member;
    }
    return written;
}

/// The stdin of `room-read --name - --after -`: the name, then one
/// `HANDLE=HEIGHT` line per stream whose height this window already holds.
///
/// Every stream is still verified whole on the other side; the floors only
/// let the read resume from what was verified and report what is new.
pub fn stdin(out: []u8, name: []const u8, me: []const u8, mine: *const Lane, members: []const Member) []const u8 {
    var w = std.Io.Writer.fixed(out);
    w.print("{s}\n", .{name}) catch return out[0..0];
    if (mine.height) |height| w.print("{s}={d}\n", .{ me, height }) catch return w.buffered();
    for (members) |member| {
        const height = member.theirs.height orelse continue;
        w.print("{s}={d}\n", .{ member.name.slice(), height }) catch break;
    }
    return w.buffered();
}

fn said(lane: *Lane, index: u64, filed: u64, text: []const u8) void {
    var message: rows.Message = .{ .index = index, .filed = filed };
    message.text.set(text);
    lane.push(message);
}

test "a room reads in filing order, theirs before mine within one period" {
    var me: Lane = .{};
    said(&me, 0, 100, "morning");
    said(&me, 1, 102, "lunch?");
    var a: Member = .{};
    a.label.set("Alice");
    said(&a.theirs, 0, 100, "hi");
    said(&a.theirs, 1, 103, "yes");
    var b: Member = .{};
    b.label.set("Bob");
    said(&b.theirs, 0, 101, "hello all");
    var out: [8]Bubble = undefined;
    const n = merge(&me, &.{ a, b }, &out);
    try std.testing.expectEqual(@as(usize, 5), n);
    try std.testing.expectEqualStrings("hi", out[0].text);
    try std.testing.expectEqualStrings("morning", out[1].text);
    try std.testing.expectEqualStrings("hello all", out[2].text);
    try std.testing.expectEqualStrings("lunch?", out[3].text);
    try std.testing.expectEqualStrings("yes", out[4].text);
    try std.testing.expect(out[1].mine and out[3].mine);
    try std.testing.expectEqualStrings("Bob", out[2].who);
    try std.testing.expect(out[0].turn and out[1].turn and out[2].turn);
}

test "the stdin names the room, then a floor per stream that has a height" {
    var me: Lane = .{};
    me.height = 4;
    var a: Member = .{};
    a.name.set("aaaa");
    a.theirs.height = 7;
    var b: Member = .{};
    b.name.set("bbbb");
    var buf: [256]u8 = undefined;
    const fed = stdin(&buf, "team", "meme", &me, &.{ a, b });
    try std.testing.expectEqualStrings("team\nmeme=4\naaaa=7\n", fed);
}
