// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! The order two streams are shown in, derived from what each segment
//! acknowledged rather than from a clock nobody carries.
//!
//! A segment on one stream says how many of the other stream's segments its
//! author had verified when writing it. That is the only relation between the
//! two streams the protocol records, and it is enough: a segment stands after
//! every segment it counts. Two segments that count nothing of each other were
//! written without knowledge of each other, and this endpoint's own goes first —
//! a stable rule, and the one a person expects when they have just typed.
//!
//! This is a presentation order, not a protocol rule. The verbs never need it;
//! a window that shows one thread does. `glass-SPEC.md` §3 says where it moves
//! if a second front end wants it.

const std = @import("std");

/// Which stream the next shown segment comes from.
pub const Side = enum { mine, theirs };

/// Interleaves two windows onto two streams.
///
/// `mine[i].acknowledged` counts `theirs` segments; `theirs[j].acknowledged`
/// counts `mine`. Both lists are ascending by `index` and may start above zero
/// (older segments dropped from a bounded window), which is why readiness is
/// judged against the next segment's absolute index and not against a count of
/// what this function has emitted. Returns how many sides were written to `out`.
pub fn merge(comptime T: type, mine: []const T, theirs: []const T, out: []Side) usize {
    var i: usize = 0;
    var j: usize = 0;
    var n: usize = 0;
    while (n < out.len and (i < mine.len or j < theirs.len)) : (n += 1) {
        const mine_ready = i < mine.len and (j >= theirs.len or mine[i].acknowledged <= theirs[j].index);
        const theirs_ready = j < theirs.len and (i >= mine.len or theirs[j].acknowledged <= mine[i].index);
        if (mine_ready or (!theirs_ready and i < mine.len)) {
            out[n] = .mine;
            i += 1;
        } else {
            out[n] = .theirs;
            j += 1;
        }
    }
    return n;
}

const Seg = struct { index: u64, acknowledged: u64 };

fn expectOrder(mine: []const Seg, theirs: []const Seg, expected: []const Side) !void {
    var out: [16]Side = undefined;
    const n = merge(Seg, mine, theirs, &out);
    try std.testing.expectEqualSlices(Side, expected, out[0..n]);
}

test "a reply stands after what it acknowledged" {
    // I say m0; they read it and reply r0 (ack 1); they add r1; I read both
    // and say m1 (ack 2).
    try expectOrder(
        &.{ .{ .index = 0, .acknowledged = 0 }, .{ .index = 1, .acknowledged = 2 } },
        &.{ .{ .index = 0, .acknowledged = 1 }, .{ .index = 1, .acknowledged = 1 } },
        &.{ .mine, .theirs, .theirs, .mine },
    );
}

test "segments that know nothing of each other put mine first" {
    try expectOrder(
        &.{.{ .index = 0, .acknowledged = 0 }},
        &.{.{ .index = 0, .acknowledged = 0 }},
        &.{ .mine, .theirs },
    );
}

test "a window that lost its oldest segments still orders by absolute index" {
    // My window starts at m3, theirs at r5. m3 had read r5 (six of theirs);
    // r6 had read m3 (four of mine); m4 had read r6.
    try expectOrder(
        &.{ .{ .index = 3, .acknowledged = 6 }, .{ .index = 4, .acknowledged = 7 } },
        &.{ .{ .index = 5, .acknowledged = 3 }, .{ .index = 6, .acknowledged = 4 } },
        &.{ .theirs, .mine, .theirs, .mine },
    );
}

test "one empty stream yields the other whole" {
    try expectOrder(&.{}, &.{ .{ .index = 0, .acknowledged = 0 }, .{ .index = 1, .acknowledged = 0 } }, &.{ .theirs, .theirs });
    try expectOrder(&.{.{ .index = 0, .acknowledged = 0 }}, &.{}, &.{.mine});
}
