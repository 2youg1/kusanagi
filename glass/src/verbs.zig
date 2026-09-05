// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! Every command line this window ever runs, in one place.
//!
//! **Nothing that identifies anybody is an argument.** A channel name, an
//! invitation, a group's members and a message all go in on stdin, and `-` on
//! the command line is what says so — the same rule the shell user follows
//! (`ARCHITECTURE.md` §8). What is left in argv is a verb and its flags.
//!
//! Each spawn collects whole stdout, so the JSON answer arrives on the exit
//! message; `answer.zig` reads it. The key of each spawn is the verb, so two of
//! one verb never run at once and a reply is matched without a table.

const std = @import("std");
const model_mod = @import("model.zig");
const Model = model_mod.Model;

/// One key per verb. Keys are model-stored identities on the effects channel;
/// a timer shares the space and takes the last one.
pub const Key = enum(u64) {
    here = 1,
    identity,
    channels,
    read_theirs,
    read_mine,
    /// The same two reads on the member the group thread's cursor points at.
    group_theirs,
    group_mine,
    send,
    invite,
    join,
    export_,
    doctor,
    group,
    fanout,
    forget,
    revoke,
    tick,
    clipboard,
    backup_file,
    face_stream,
    face_file,
    language_file,
    poll_timer,
    slot_timer,
    scrub_timer,
    clipboard_read,
    clipboard_scrub,
};

pub fn key(k: Key) u64 {
    return @intFromEnum(k);
}

pub fn keyOf(raw: u64) ?Key {
    return std.enums.fromInt(Key, raw);
}

/// The one shape every spawn takes: the binary, `--json`, then the verb.
fn spawn(fx: anytype, m: *const Model, k: Key, argv: []const []const u8, stdin: ?[]const u8) void {
    var full: [12][]const u8 = undefined;
    full[0] = m.bin.slice();
    full[1] = "--json";
    for (argv, 0..) |arg, i| full[2 + i] = arg;
    const Effects = @TypeOf(fx.*);
    fx.spawn(.{
        .key = key(k),
        .argv = full[0 .. 2 + argv.len],
        .stdin = stdin,
        .output = .collect,
        .on_exit = Effects.exitMsg(.exited),
    });
}

pub fn here(fx: anytype, m: *const Model) void {
    spawn(fx, m, .here, &.{ "doctor", "--here" }, null);
}

pub fn identity(fx: anytype, m: *const Model) void {
    spawn(fx, m, .identity, &.{"id"}, null);
}

pub fn channels(fx: anytype, m: *const Model) void {
    spawn(fx, m, .channels, &.{"channels"}, null);
}

fn readsMine(k: Key) bool {
    return k == .read_mine or k == .group_mine;
}

/// Reads one stream, resuming above `after` when the window already holds it.
/// `scratch` is the caller's to own: a second read reuses no buffer of
/// the first, so an executor that keeps the slice sees each request whole.
pub fn read(fx: anytype, m: *const Model, k: Key, name: []const u8, after: ?u64, scratch: []u8) void {
    const stdin = std.fmt.bufPrint(scratch[0..], "{s}\n", .{name}) catch return;
    if (after) |floor| {
        var number: [20]u8 = undefined;
        const digits = std.fmt.bufPrint(&number, "{d}", .{floor}) catch return;
        const mine: []const []const u8 = &.{ "read", "--from", "-", "--after", digits, "--mine" };
        const theirs: []const []const u8 = &.{ "read", "--from", "-", "--after", digits };
        spawn(fx, m, k, if (readsMine(k)) mine else theirs, stdin);
        return;
    }
    const mine: []const []const u8 = &.{ "read", "--from", "-", "--mine" };
    const theirs: []const []const u8 = &.{ "read", "--from", "-" };
    spawn(fx, m, k, if (readsMine(k)) mine else theirs, stdin);
}

/// `name` on the first line, the text on the rest: the stdin form of `--to -`.
pub fn send(fx: anytype, m: *const Model, name: []const u8, text: []const u8, scratch: []u8) void {
    const stdin = std.fmt.bufPrint(scratch, "{s}\n{s}", .{ name, text }) catch return;
    spawn(fx, m, .send, &.{ "send", "--to", "-" }, stdin);
}

pub fn fanout(fx: anytype, m: *const Model, roster: []const u8, text: []const u8, scratch: []u8) void {
    const stdin = std.fmt.bufPrint(scratch, "{s}\n{s}", .{ roster, text }) catch return;
    spawn(fx, m, .fanout, &.{ "send", "--to-group", "-" }, stdin);
}

pub const Habit = struct { every: ?u32, release: bool };

pub fn invite(fx: anytype, m: *const Model, name: []const u8, waypoint: []const u8, habit: Habit, scratch: []u8) void {
    var argv: [8][]const u8 = undefined;
    var n: usize = 0;
    argv[n] = "invite";
    n += 1;
    argv[n] = "--name";
    n += 1;
    argv[n] = "-";
    n += 1;
    argv[n] = "--waypoint";
    n += 1;
    argv[n] = waypoint;
    n += 1;
    var period: [12]u8 = undefined;
    if (habit.every) |every| {
        argv[n] = "--every";
        n += 1;
        argv[n] = std.fmt.bufPrint(&period, "{d}", .{every}) catch return;
        n += 1;
    }
    if (habit.release) {
        argv[n] = "--release";
        n += 1;
    }
    const stdin = std.fmt.bufPrint(scratch, "{s}\n", .{name}) catch return;
    spawn(fx, m, .invite, argv[0..n], stdin);
}

/// The name on the first line, the invitation on the second — never in argv.
pub fn join(fx: anytype, m: *const Model, name: []const u8, invitation: []const u8, release: bool, scratch: []u8) void {
    const stdin = std.fmt.bufPrint(scratch, "{s}\n{s}", .{ name, std.mem.trim(u8, invitation, " \r\n\t") }) catch return;
    if (release) {
        spawn(fx, m, .join, &.{ "join", "--name", "-", "--release" }, stdin);
    } else {
        spawn(fx, m, .join, &.{ "join", "--name", "-" }, stdin);
    }
}

pub fn exportSite(fx: anytype, m: *const Model) void {
    spawn(fx, m, .export_, &.{"export"}, null);
}

pub fn doctor(fx: anytype, m: *const Model, waypoint: []const u8) void {
    spawn(fx, m, .doctor, &.{ "doctor", waypoint }, null);
}

/// The group's name on the first line, then one member per line.
pub fn group(fx: anytype, m: *const Model, name: []const u8, members: []const []const u8, scratch: []u8) void {
    var w: std.Io.Writer = .fixed(scratch);
    w.writeAll(name) catch return;
    for (members) |member| {
        w.writeByte('\n') catch return;
        w.writeAll(member) catch return;
    }
    spawn(fx, m, .group, &.{ "group", "--name", "-" }, w.buffered());
}

pub fn forget(fx: anytype, m: *const Model, name: []const u8, scratch: *[40]u8) void {
    const stdin = std.fmt.bufPrint(scratch[0..], "{s}\n", .{name}) catch return;
    spawn(fx, m, .forget, &.{ "forget", "--channel", "-" }, stdin);
}

pub fn revoke(fx: anytype, m: *const Model, name: []const u8, scratch: *[40]u8) void {
    const stdin = std.fmt.bufPrint(scratch[0..], "{s}\n", .{name}) catch return;
    spawn(fx, m, .revoke, &.{ "revoke", "--from", "-" }, stdin);
}

pub fn tick(fx: anytype, m: *const Model, name: []const u8, scratch: *[40]u8) void {
    const stdin = std.fmt.bufPrint(scratch[0..], "{s}\n", .{name}) catch return;
    spawn(fx, m, .tick, &.{ "tick", "--from", "-" }, stdin);
}
