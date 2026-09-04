// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! The body face a message is written in, found on this machine.
//!
//! Chinese needs a face the SDK does not ship, and shipping one used to cost
//! every user 17.7 MB they might never read. So the face is looked for instead:
//! a path somebody chose wins, and otherwise the first family that this machine
//! already holds and that can draw the script. A machine with neither keeps the
//! Latin face the runtime registers itself, which shows a person English labels
//! rather than a row of boxes — the registered ids fall back to the built-in
//! outlines, so having no face here is an ordinary state and not a broken one.
//!
//! Judging happens on the bytes. A name is not evidence: `msyh.ttc` is the face
//! most people here would ask for and it is a collection the SDK refuses, and a
//! file called `NotoSansSC` may be the CFF build it refuses the same way.
//!
//! The search runs before the window exists because registration is permanent —
//! an id is held for the life of the process, with no unregister — so changing
//! face means restarting, and nothing here may be decided later.

const std = @import("std");
const native_sdk = @import("native_sdk");
const canvas = native_sdk.canvas;
const font_ttf = canvas.font_ttf;

/// The largest face the SDK will hold. Reading no further than this is what
/// turns an oversized family into a skip rather than a wasted 40 MB.
pub const max_face_bytes = 24 * 1024 * 1024;

/// 中. Every face that writes Chinese maps it; one that does not is not a
/// Chinese face, whatever it is called on disk.
const han: u21 = 0x4E2D;

/// The file holding a person's own choice: one path and nothing else. It sits
/// beside the backups rather than inside the site, because a font path is no
/// secret and the site encrypts everything it holds and owns the only copy of
/// the things that matter.
pub const preference_file = "kusanagi-glass.font";

pub const Face = struct {
    /// The name a person would recognise, for the settings sheet.
    file: []const u8,
    ttf: []const u8,
};

pub const Choice = struct {
    face: ?Face = null,
    /// A named face that could not be used, kept so that losing a person's own
    /// choice is never silent. Empty when nothing was chosen.
    refused: []const u8 = "",
    /// Why, in the SDK's own words where it has them; empty with `refused`.
    reason: []const u8 = "",
};

/// Said when the bytes parse as a face that cannot draw this script. The
/// other verdicts are the SDK's own sentences, passed on unchanged.
pub const no_han = "the face has no glyph for \u{4E2D}";

/// Why these bytes cannot be the body face, or null when they can. The one
/// judgement both the start-up search and the settings sheet use, so a face
/// accepted on the sheet is accepted at the next start.
pub fn verdict(bytes: []const u8) ?[]const u8 {
    if (font_ttf.parseFailureReason(bytes)) |reason| return reason;
    return if (usable(bytes)) null else no_han;
}

/// Families tried in order, favouring what arrives with the platform over what
/// somebody installed: those exist on more machines, and they do not need this
/// file's permission to change.
const candidates = [_][]const u8{
    "NotoSansSC-VF.ttf",
    "NotoSansSC-Regular.ttf",
    "simhei.ttf",
    "simfang.ttf",
    "Deng.ttf",
    "WenQuanYi Zen Hei.ttf",
};

/// Where a machine keeps its faces. Only the directories that exist are opened,
/// and one that does not is a skipped turn rather than a failure, so every
/// platform can be listed on every platform.
const directories = [_][]const u8{
    "C:\\Windows\\Fonts",
    "/usr/share/fonts",
    "/usr/share/fonts/truetype",
    "/Library/Fonts",
    "/System/Library/Fonts",
};

/// True when these bytes are one TrueType face that can draw this script. Total
/// for any input: the parser is bounds-checked, and the lookup reads an offset
/// the parse already accepted.
pub fn usable(bytes: []const u8) bool {
    const face = font_ttf.Face.parse(bytes) catch return false;
    return face.glyphIndex(han) != 0;
}

/// The face to register. `arena` must outlive the window: it holds both the
/// bytes and the name in `Choice`, and the runner copies the bytes it keeps.
pub fn choose(io: std.Io, arena: std.mem.Allocator, home: []const u8) Choice {
    var path: [std.Io.Dir.max_path_bytes]u8 = undefined;
    if (chosen(io, arena, home)) |named| {
        // The choice was the person's own, so saying which name failed and why
        // beats quietly searching a list they did not ask for.
        const bytes = std.Io.Dir.cwd().readFileAlloc(io, named, arena, .limited(max_face_bytes)) catch
            return .{ .refused = std.fs.path.basename(named), .reason = "the file could not be read" };
        return if (verdict(bytes)) |why|
            .{ .refused = std.fs.path.basename(named), .reason = why }
        else
            .{ .face = .{ .file = std.fs.path.basename(named), .ttf = bytes } };
    }

    for (directories) |directory| {
        for (candidates) |name| {
            const here = std.fmt.bufPrint(&path, "{s}{c}{s}", .{ directory, std.fs.path.sep, name }) catch continue;
            if (open(io, arena, here, name)) |face| return .{ .face = face };
        }
    }
    return .{};
}

/// One path read and judged, named by the caller. `file` outlives this call
/// because it is a literal from `candidates` or a slice of the arena; taking the
/// basename of a stack-built path here would hand back a pointer to a dead frame.
/// Null when the face is absent, bigger than the SDK can hold, or cannot write
/// this script: an unreadable file is a missing file, because a person who
/// cannot be asked is better served in English than stopped at.
fn open(io: std.Io, arena: std.mem.Allocator, path: []const u8, file: []const u8) ?Face {
    const bytes = std.Io.Dir.cwd().readFileAlloc(io, path, arena, .limited(max_face_bytes)) catch return null;
    if (!usable(bytes)) return null;
    return .{ .file = file, .ttf = bytes };
}

/// The path a person wrote, trimmed of the whitespace and newline that come from
/// typing it by hand. Null when there is no such file or it holds nothing.
fn chosen(io: std.Io, arena: std.mem.Allocator, home: []const u8) ?[]const u8 {
    var buffer: [std.Io.Dir.max_path_bytes]u8 = undefined;
    const where = std.fmt.bufPrint(&buffer, "{s}{c}{s}", .{ home, std.fs.path.sep, preference_file }) catch return null;
    const written = std.Io.Dir.cwd().readFileAlloc(io, where, arena, .limited(4096)) catch return null;
    const trimmed = std.mem.trim(u8, written, " \t\r\n");
    return if (trimmed.len == 0 or trimmed.len > buffer.len) null else trimmed;
}

test "a face is judged by whether it draws this script, not by its name" {
    // Nothing, nonsense, and the two shapes the SDK teaches against: a
    // collection and a CFF build. All four are refused rather than trusted,
    // and the reason given is the SDK's own sentence.
    try std.testing.expect(!usable(&.{}));
    try std.testing.expect(!usable("not a font at all"));
    try std.testing.expect(!usable("ttcf" ++ [_]u8{0} ** 48));
    try std.testing.expect(!usable("OTTO" ++ [_]u8{0} ** 48));
    try std.testing.expectEqualStrings(
        "font is a TrueType collection (.ttc); extract the single face to register",
        verdict("ttcf" ++ [_]u8{0} ** 48).?,
    );
    try std.testing.expect(std.mem.indexOf(u8, verdict("OTTO" ++ [_]u8{0} ** 48).?, "TrueType 'glyf'") != null);
}

test "the search holds only names, and every one of them a single face" {
    for (candidates) |name| {
        try std.testing.expect(std.fs.path.dirname(name) == null);
        try std.testing.expectEqualStrings(".ttf", std.fs.path.extension(name));
    }
    // The preference file is named, never pathed: its directory comes from the
    // machine, so a name carrying a separator would be a path in disguise.
    try std.testing.expect(std.fs.path.dirname(preference_file) == null);
}

test "a path that is not there is a skipped face, never a failure" {
    var threaded = std.Io.Threaded.init(std.heap.page_allocator, .{});
    defer threaded.deinit();
    var arena = std.heap.ArenaAllocator.init(std.heap.page_allocator);
    defer arena.deinit();
    try std.testing.expectEqual(
        null,
        open(threaded.io(), arena.allocator(), "no-such-directory/NoSuchFace.ttf", "NoSuchFace.ttf"),
    );
}
