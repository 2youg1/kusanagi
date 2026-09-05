// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! How the group thread is kept current: one member per poll, round-robin.
//!
//! Apart from `update.zig` because it is the one place a screen drives more
//! than two reads. A window on a group of thirty must make the same rhythm on
//! the host as a window on one channel — one `read` per tick — so the cursor
//! walks the members and a catch-up round (on opening, after a broadcast)
//! chains member to member until every lane has been looked at once.

const std = @import("std");
const model_mod = @import("model.zig");
const room = @import("room.zig");
const update = @import("update.zig");
const verbs = @import("verbs.zig");

const Model = model_mod.Model;
const Effects = update.Effects;

pub fn open(m: *Model, fx: *Effects, slot: usize) void {
    m.selected_group = @min(slot, model_mod.max_groups - 1);
    m.screen = .group;
    m.delivered_count = 0;
    update.stopTimers(fx);
    // The thread is rebuilt from the roster: each member's channel row lends
    // its label (the alias they signed, or their short handle) and whether
    // anybody has joined it yet. Then one catch-up round reads every member
    // in turn, and after it the poll timer reads one member per tick.
    const thread = &m.group_thread;
    thread.clear();
    const roster = m.currentGroup();
    thread.room = roster.room;
    for (roster.members[0..roster.count]) |member| {
        // A room's members are authors by handle, mine among them; a group's
        // are channels, each lending its label and whether anybody joined.
        if (thread.room) {
            if (!m.handle.eql(member.slice())) _ = thread.admit(member.slice());
            continue;
        }
        var out: @TypeOf(thread.members[0]) = .{};
        out.name.set(member.slice());
        for (m.channelRows()) |row| {
            if (!std.mem.eql(u8, row.name.slice(), member.slice())) continue;
            out.label.set(if (row.hasPeer()) row.peer.slice() else row.name.slice());
            out.has_peer = row.hasPeer();
        }
        thread.members[thread.count] = out;
        thread.count += 1;
    }
    thread.catching_up = if (thread.room) 0 else thread.count;
    step(m, fx);
    fx.startTimer(.{ .key = verbs.key(.poll_timer), .interval_ms = update.poll_ms, .mode = .repeating, .on_fire = Effects.timerMsg(.poll) });
}

/// Reads the member under the cursor: their stream first, mine on its exit.
/// A room is one read for everybody, floors included.
pub fn step(m: *Model, fx: *Effects) void {
    if (!m.onGroup()) return;
    const thread = &m.group_thread;
    if (thread.room) {
        const fed = room.stdin(&thread.stdin.buf, m.groupTitle(), m.handle.slice(), &thread.me, thread.all());
        thread.stdin.len = fed.len;
        return verbs.roomRead(fx, m, fed);
    }
    if (thread.count == 0) return;
    const member = m.group_thread.current();
    verbs.read(fx, m, .group_theirs, member.name.slice(), member.theirs.height, &m.group_thread.stdin.buf);
}

/// One member is read; the cursor moves, and a catch-up round goes on by itself.
fn advance(m: *Model, fx: *Effects) void {
    if (m.group_thread.advance()) step(m, fx);
}

/// Every member's lane may now hold a copy of what was just broadcast; one
/// round collects them.
pub fn round(m: *Model, fx: *Effects) void {
    m.group_thread.cursor = 0;
    m.group_thread.catching_up = m.group_thread.count;
    step(m, fx);
}

/// Handles the exit of a group read; false when `key` was not one.
///
/// A member whose stream refused still yields the cursor: a group of five
/// with one dead channel must go on reading the other four.
pub fn exited(m: *Model, fx: *Effects, key: verbs.Key, failed: bool) bool {
    switch (key) {
        .room_read => {},
        // What was just said is on the host; one read shows it in its place.
        .room_send => if (!failed) step(m, fx),
        .group_theirs => {
            const member = m.group_thread.current();
            // A read that succeeded on a member still marked as waiting has
            // just met them; the label comes from the channel list.
            if (!failed and !member.has_peer) verbs.channels(fx, m);
            if (failed or !member.has_peer) {
                advance(m, fx);
            } else {
                verbs.read(fx, m, .group_mine, member.name.slice(), member.mine.height, &m.group_thread.stdin.buf);
            }
        },
        .group_mine => advance(m, fx),
        else => return false,
    }
    return true;
}
