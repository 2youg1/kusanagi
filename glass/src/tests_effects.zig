// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! The effects a message arm leaves behind: clipboard, timers, the quiet
//! read on a channel nobody has joined, and a complaint's landing place.
//! Same fixture as `tests.zig`; split so that each file stays under 400.

const std = @import("std");
const testing = std.testing;
const shared = @import("tests.zig");
const answer = @import("answer.zig");
const verbs = @import("verbs.zig");

const Fixture = shared.Fixture;
const Effects = @import("main.zig").Effects;
const expectByText = shared.expectByText;
const findByText = shared.findByText;
const stdinOf = shared.stdinOf;
const argvHolds = shared.argvHolds;

fn scrubWrites(fx: *Effects) usize {
    var count: usize = 0;
    var i: usize = 0;
    while (i < fx.pendingClipboardCount()) : (i += 1) {
        const request = fx.pendingClipboardAt(i).?;
        if (request.key == verbs.key(.clipboard_scrub) and request.op == .write and request.text.len == 0) count += 1;
    }
    return count;
}

test "what was copied is taken back a minute later, unless the clipboard has moved on" {
    var f = Fixture.init();
    defer f.deinit();
    f.dispatch(.show_invite);
    f.exited(.invite,
        \\{"contract":1,"command":"invited","name":"carol","invite":"kusanagi2:0201ab","check":"7f3a","expires_at":1,"expires_in":604800}
    );
    f.dispatch(.copy_invite);
    // The write and the one-shot timer both leave on the effects channel, and
    // the note says the clipboard will be cleared.
    try testing.expectEqual(@as(usize, 1), f.fx.pendingClipboardCount());
    try testing.expectEqualStrings("kusanagi2:0201ab", f.fx.pendingClipboardAt(0).?.text);
    try testing.expectEqual(@as(usize, 1), f.fx.pendingTimerCount());
    try testing.expectEqual(verbs.key(.scrub_timer), f.fx.pendingTimerAt(0).?.key);
    try testing.expect(f.model.clipboardHeld());
    var tree = try f.tree();
    _ = try expectByText(tree.root, .text, "Copied. The clipboard is a log — Windows keeps a history of it and may sync it — so this window clears it again a minute from now, unless you have copied something else by then.");

    // The timer fires; the clipboard still holds the invitation; it is emptied.
    f.dispatch(.{ .scrub = .{ .key = verbs.key(.scrub_timer), .outcome = .fired } });
    f.dispatch(.{ .scrubbing = .{ .key = verbs.key(.clipboard_read), .op = .read, .outcome = .ok, .text = "kusanagi2:0201ab" } });
    try testing.expect(!f.model.clipboardHeld());
    try testing.expect(scrubWrites(&f.fx) == 1);

    // Copied again, but by then the person copied something of their own:
    // theirs is left alone.
    f.dispatch(.copy_invite);
    f.dispatch(.{ .scrub = .{ .key = verbs.key(.scrub_timer), .outcome = .fired } });
    f.dispatch(.{ .scrubbing = .{ .key = verbs.key(.clipboard_read), .op = .read, .outcome = .ok, .text = "their grocery list" } });
    try testing.expect(!f.model.clipboardHeld());
    try testing.expect(scrubWrites(&f.fx) == 1);
    tree = try f.tree();
    try testing.expect(findByText(tree.root, .text, "Copied. The clipboard is a log — Windows keeps a history of it and may sync it — so this window clears it again a minute from now, unless you have copied something else by then.") == null);
}

test "a channel nobody has joined is still read, quietly, and a read that meets them refreshes the list" {
    var f = Fixture.init();
    defer f.deinit();
    // The list says the peer has not arrived.
    answer.apply(f.model, .{ .key = verbs.key(.channels), .code = 0, .output =
        \\{"contract":1,"command":"channels","channels":[{"name":"bob","waypoint":"http://127.0.0.1:8963","standing":"root","peer":null,"period":null,"retention":"keep","can":["send","read"]}],"groups":[]}
    });
    f.dispatch(.{ .select = 0 });
    // Their stream is asked for — that read is the greeting — and nothing of ours is.
    try testing.expect(stdinOf(&f.fx, verbs.key(.read_theirs)) != null);
    try testing.expect(stdinOf(&f.fx, verbs.key(.read_mine)) == null);
    // Confirmation that nobody has joined stays out of the status line.
    f.dispatch(.{ .exited = .{ .key = verbs.key(.read_theirs), .code = 1, .stderr_tail =
        \\{"contract":1,"error":"nobody has joined `bob` yet","code":"kusanagi.no_peer_yet","recover":"wait"}
    } });
    try testing.expect(f.model.status.code.isEmpty());
    // A read that succeeds while the row still waits has met them: the list is asked again.
    const spawned = f.fx.pendingSpawnCount();
    f.exited(.read_theirs,
        \\{"contract":1,"command":"read","name":"bob","author":"3f9a1c0e7b2d","height":0,"segments":[{"index":0,"acknowledged":0,"text":"hi"}]}
    );
    try testing.expect(f.fx.pendingSpawnCount() == spawned + 1);
    try testing.expect(stdinOf(&f.fx, verbs.key(.channels)) != null or argvHolds(&f.fx, "channels"));
}

test "a complaint lands in the status line with its code and recovery" {
    var f = Fixture.init();
    defer f.deinit();
    f.dispatch(.{ .select = 0 });
    f.dispatch(.{ .exited = .{ .key = verbs.key(.read_theirs), .code = 1, .stderr_tail =
        \\{"contract":1,"error":"the host did not answer","code":"waypoint.timeout","recover":"retry; if it persists the host is down"}
    } });
    try testing.expectEqualStrings("waypoint.timeout", f.model.status.code.slice());
    const tree = try f.tree();
    _ = try expectByText(tree.root, .text, "the host did not answer");
}
