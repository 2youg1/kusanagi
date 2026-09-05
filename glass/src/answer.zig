// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! What a verb answered, read into the model.
//!
//! `kusanagi --json` writes one object per call: an outcome tagged `command`,
//! or a complaint carrying `code` and `recover`. Both arrive on the spawn's exit
//! message — stdout for outcomes, stderr for complaints and for `export`, whose
//! stdout is the archive itself. This file reads only the keys it needs and
//! ignores the rest, which is what lets the verbs grow a field without this
//! window noticing.

const std = @import("std");
const native_sdk = @import("native_sdk");
const model_mod = @import("model.zig");
const verbs = @import("verbs.zig");

const Model = model_mod.Model;
const Value = std.json.Value;

fn str(fields: std.json.ObjectMap, name: []const u8) []const u8 {
    const value = fields.get(name) orelse return "";
    return switch (value) {
        .string => |s| s,
        else => "",
    };
}

fn uint(fields: std.json.ObjectMap, name: []const u8) ?u64 {
    const value = fields.get(name) orelse return null;
    return switch (value) {
        .integer => |i| if (i >= 0) @intCast(i) else null,
        else => null,
    };
}

fn boolean(fields: std.json.ObjectMap, name: []const u8) bool {
    const value = fields.get(name) orelse return false;
    return switch (value) {
        .bool => |b| b,
        else => false,
    };
}

fn items(fields: std.json.ObjectMap, name: []const u8) []const Value {
    const value = fields.get(name) orelse return &.{};
    return switch (value) {
        .array => |a| a.items,
        else => &.{},
    };
}

fn object(value: Value) ?std.json.ObjectMap {
    return switch (value) {
        .object => |o| o,
        else => null,
    };
}

/// Reads one exit into the model. Every path ends with the status line saying
/// what happened, so nothing a verb reported goes unshown.
pub fn apply(m: *Model, exit: native_sdk.EffectExit) void {
    const key = verbs.keyOf(exit.key) orelse return;
    switch (exit.reason) {
        .exited => {},
        .spawn_failed => return failed(m, "glass.no_binary", m.t.err_no_binary, m.t.rec_no_binary),
        .rejected => return failed(m, "glass.busy", m.t.err_busy, m.t.rec_busy),
        .cancelled => return failed(m, "glass.cancelled", m.t.err_cancelled, m.t.rec_run_again),
        .signaled => return failed(m, "glass.killed", m.t.err_killed, m.t.rec_run_again),
    }
    var arena_state = std.heap.ArenaAllocator.init(std.heap.page_allocator);
    defer arena_state.deinit();
    const arena = arena_state.allocator();

    // A complaint is the one shape every failure has; `export` puts its
    // outcome on stderr too, because its stdout is the archive.
    const source = if (exit.code != 0 or key == .export_) exit.stderr_tail else exit.output;
    const parsed = std.json.parseFromSliceLeaky(Value, arena, std.mem.trim(u8, source, " \r\n\t"), .{}) catch {
        return failed(m, "glass.unreadable", m.t.err_unreadable, m.t.rec_terminal);
    };
    const answer = object(parsed) orelse return failed(m, "glass.unreadable", m.t.err_not_object, m.t.rec_terminal);
    if (exit.code != 0) {
        m.status.clear();
        // The banner on the thread already says nobody has joined; a poll
        // that confirms it is not news.
        if (std.mem.eql(u8, str(answer, "code"), "kusanagi.no_peer_yet")) return;
        m.status.code.set(str(answer, "code"));
        m.status.error_text.set(str(answer, "error"));
        m.status.recover.set(str(answer, "recover"));
        return;
    }
    m.output_cut = exit.output_truncated;
    m.status.clear();
    switch (key) {
        .here => here(m, answer),
        .identity => m.handle.set(str(answer, "handle")),
        .channels => channels(m, answer),
        .read_theirs => read(m, &m.theirs, answer),
        .read_mine => read(m, &m.mine, answer),
        .group_theirs => read(m, &m.group_thread.current().theirs, answer),
        .group_mine => read(m, &m.group_thread.current().mine, answer),
        .send => sent(m, answer),
        .invite => invited(m, answer),
        .join => joined(m, answer),
        .export_ => exported(m, answer, exit.output),
        .doctor => examined(m, answer),
        .group => m.status.note.set(m.t.note_group_saved),
        .fanout => fannedOut(m, answer),
        .room => m.status.note.set(m.t.note_room_founded),
        .room_invite => invited(m, answer),
        .room_join => joined(m, answer),
        .room_send => {},
        .room_read => roomRead(m, answer),
        .forget => m.status.note.set(m.t.note_forgotten),
        .revoke => m.status.note.set(m.t.note_revoked),
        .tick => ticked(m, answer),
        else => {},
    }
}

fn failed(m: *Model, code: []const u8, text: []const u8, recover: []const u8) void {
    m.status.clear();
    m.status.code.set(code);
    m.status.error_text.set(text);
    m.status.recover.set(recover);
}

fn here(m: *Model, answer: std.json.ObjectMap) void {
    m.site.set(str(answer, "site"));
    m.at_rest.set(str(answer, "at_rest"));
    m.proxy = boolean(answer, "proxy");
    m.binary.set(str(answer, "binary"));
}

fn channels(m: *Model, answer: std.json.ObjectMap) void {
    m.channel_count = 0;
    for (items(answer, "channels")) |value| {
        const row = object(value) orelse continue;
        if (m.channel_count == model_mod.max_channels) break;
        var out: model_mod.ChannelRow = .{ .slot = m.channel_count };
        out.name.set(str(row, "name"));
        out.waypoint.set(str(row, "waypoint"));
        out.peer.set(str(row, "peer"));
        out.root = std.mem.eql(u8, str(row, "standing"), "root");
        out.period = @intCast(@min(uint(row, "period") orelse 0, std.math.maxInt(u32)));
        out.releases = std.mem.eql(u8, str(row, "retention"), "release");
        for (items(row, "can")) |ability| {
            if (ability == .string and std.mem.eql(u8, ability.string, "send")) out.can_send = true;
        }
        out.refused.set(str(row, "refused"));
        out.peer_refused.set(str(row, "peer_refused"));
        out.expires_in = uint(row, "expires_in") orelse 0;
        m.channels[m.channel_count] = out;
        m.channel_count += 1;
    }
    m.group_count = 0;
    // Rooms are rows of the same list: a name over members. What differs is
    // how the thread is read, and the row says which it is.
    for ([_][]const u8{ "groups", "rooms" }) |kind| {
        for (items(answer, kind)) |value| {
            const row = object(value) orelse continue;
            if (m.group_count == model_mod.max_groups) break;
            var out: model_mod.GroupRow = .{ .slot = m.group_count, .room = kind[0] == 'r' };
            out.name.set(str(row, "name"));
            for (items(row, "members")) |member| {
                if (member != .string or out.count == model_mod.max_members) continue;
                out.members[out.count].set(member.string);
                out.count += 1;
            }
            m.groups[m.group_count] = out;
            m.group_count += 1;
        }
    }
    if (m.selected >= m.channel_count) m.selected = 0;
    if (m.selected_group >= m.group_count) m.selected_group = 0;
}

fn read(m: *Model, lane: anytype, answer: std.json.ObjectMap) void {
    lane.height = uint(answer, "height");
    for (items(answer, "segments")) |value| {
        const row = object(value) orelse continue;
        var message: model_mod.Message = .{
            .index = uint(row, "index") orelse 0,
            .acknowledged = uint(row, "acknowledged") orelse 0,
            .filed = uint(row, "filed") orelse 0,
        };
        const text = str(row, "text");
        if (text.len > 0 or row.get("text") != null) {
            message.text.set(text);
        } else {
            message.text.set(str(row, "payload"));
            message.is_hex = true;
        }
        // A resumed read may repeat the last known segment; the index says so.
        if (lane.count > 0 and lane.items[lane.count - 1].index >= message.index) continue;
        lane.push(message);
    }
    if (m.output_cut) m.status.note.set(m.t.note_cut);
}

/// One row per author: mine into the thread's own lane, every other into
/// the member it names — admitted on the spot when the roster grew.
fn roomRead(m: *Model, answer: std.json.ObjectMap) void {
    const thread = &m.group_thread;
    for (items(answer, "threads")) |value| {
        const row = object(value) orelse continue;
        const author = str(row, "author");
        if (m.handle.eql(author)) {
            read(m, &thread.me, row);
        } else if (thread.admit(author)) |member| {
            read(m, &member.theirs, row);
        }
    }
}

fn sent(m: *Model, answer: std.json.ObjectMap) void {
    if (std.mem.eql(u8, str(answer, "command"), "queued")) {
        m.status.note.set(m.t.note_queued);
        return;
    }
    var message: model_mod.Message = .{
        .index = uint(answer, "index") orelse 0,
        .acknowledged = m.theirHeight(),
    };
    message.text.set(m.draft.text());
    m.mine.push(message);
    m.mine.height = message.index;
    m.draft.clear();
}

fn invited(m: *Model, answer: std.json.ObjectMap) void {
    m.invite.line.set(str(answer, "invite"));
    m.check.set(str(answer, "check"));
    m.check_for.set(str(answer, "name"));
    m.status.note.set(m.t.note_minted);
}

fn joined(m: *Model, answer: std.json.ObjectMap) void {
    m.invite.line.clear();
    m.check.set(str(answer, "check"));
    m.check_for.set(str(answer, "name"));
    m.join.invitation.clear();
    m.status.note.set(m.t.note_joined);
}

fn exported(m: *Model, answer: std.json.ObjectMap, archive: []const u8) void {
    m.backup.keep(str(answer, "recovery"), archive);
}

fn examined(m: *Model, answer: std.json.ObjectMap) void {
    const doctor = &m.doctor;
    doctor.tier.set(str(answer, "tier"));
    doctor.measured.set(str(answer, "waypoint"));
    doctor.finding_count = 0;
    for (items(answer, "capabilities")) |value| {
        const row = object(value) orelse continue;
        if (doctor.finding_count == model_mod.max_rows) break;
        var out: model_mod.CheckRow = .{ .slot = doctor.finding_count };
        out.name.set(str(row, "capability"));
        out.checked = std.mem.eql(u8, str(row, "verdict"), "held");
        const detail = str(row, "detail");
        out.note.set(if (detail.len > 0) detail else str(row, "verdict"));
        doctor.findings[doctor.finding_count] = out;
        doctor.finding_count += 1;
    }
}

fn fannedOut(m: *Model, answer: std.json.ObjectMap) void {
    m.delivered_count = 0;
    for (items(answer, "delivered")) |value| {
        const row = object(value) orelse continue;
        if (m.delivered_count == model_mod.max_rows) break;
        var out: model_mod.CheckRow = .{ .slot = m.delivered_count };
        out.name.set(str(row, "member"));
        out.checked = std.mem.eql(u8, str(row, "status"), "sent");
        out.note.set(if (out.checked) "delivered" else str(row, "error"));
        m.delivered[m.delivered_count] = out;
        m.delivered_count += 1;
    }
    m.draft.clear();
}

fn ticked(m: *Model, answer: std.json.ObjectMap) void {
    const carried = str(answer, "carried");
    if (uint(answer, "heard")) |heard| m.theirs.height = heard;
    m.status.note.set(if (std.mem.eql(u8, carried, "message")) m.t.note_slot_message else if (std.mem.eql(u8, carried, "filler")) m.t.note_slot_filler else m.t.note_slot_taken);
}
