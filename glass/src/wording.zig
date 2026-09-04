// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! The three ways a value becomes a sentence on screen. Pure: every result is
//! written into the build arena and lives exactly one view build.

const std = @import("std");

/// `sentence` with each `{d}` replaced by the next number. The sentence comes
/// from `strings.zig`, so the placeholder is found at run time, not compiled.
pub fn counted(arena: std.mem.Allocator, sentence: []const u8, numbers: []const u64) []const u8 {
    var w = std.Io.Writer.Allocating.init(arena);
    var rest = sentence;
    for (numbers) |n| {
        const at = std.mem.indexOf(u8, rest, "{d}") orelse break;
        w.writer.print("{s}{d}", .{ rest[0..at], n }) catch return sentence;
        rest = rest[at + 3 ..];
    }
    w.writer.writeAll(rest) catch return sentence;
    return w.written();
}

/// `text` in groups of `every` characters with a space between, the way a
/// fingerprint is read across a table; a paragraph then wraps at the groups.
pub fn grouped(arena: std.mem.Allocator, text: []const u8, every: usize) []const u8 {
    const groups = (text.len + every - 1) / every;
    const out = arena.alloc(u8, text.len + groups) catch return text;
    var n: usize = 0;
    for (text, 0..) |c, i| {
        if (i > 0 and i % every == 0) {
            out[n] = ' ';
            n += 1;
        }
        out[n] = c;
        n += 1;
    }
    return out[0..n];
}

/// Every character followed by a space, the way four characters are read aloud.
pub fn spaced(arena: std.mem.Allocator, code: []const u8) []const u8 {
    const out = arena.alloc(u8, code.len * 2) catch return code;
    for (code, 0..) |c, i| {
        out[i * 2] = c;
        out[i * 2 + 1] = ' ';
    }
    return std.mem.trimEnd(u8, out, " ");
}

test "a counted sentence takes its numbers in order and keeps its tail" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    try std.testing.expectEqualStrings("2 of theirs · 3 of yours", counted(arena.allocator(), "{d} of theirs · {d} of yours", &.{ 2, 3 }));
    try std.testing.expectEqualStrings("plain", counted(arena.allocator(), "plain", &.{7}));
    try std.testing.expectEqualStrings("89958e2f c5440a1b", grouped(arena.allocator(), "89958e2fc5440a1b", 8));
    try std.testing.expectEqualStrings("7 f 3 a", spaced(arena.allocator(), "7f3a"));
}
