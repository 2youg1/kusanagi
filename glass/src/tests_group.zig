// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! The group page as one thread (F7): my broadcast once, each reply under it
//! with the member's name, read one member at a time.

const std = @import("std");
const tests = @import("tests.zig");
const verbs = @import("verbs.zig");
const answer = @import("answer.zig");

const testing = std.testing;
const Fixture = tests.Fixture;

/// Whether the newest spawn under `key` carries `stdin`: spawns share the
/// model's `name_scratch`, which the next read rewrites in place, so only the
/// newest request's bytes are trustworthy — and that is exactly the member the
/// cursor has just moved to.
fn lastSpawnStdin(f: *Fixture, key: verbs.Key) ?[]const u8 {
    var i = f.fx.pendingSpawnCount();
    var found: ?[]const u8 = null;
    while (i > 0) : (i -= 1) {
        const request = f.fx.pendingSpawnAt(i - 1).?;
        if (request.key == verbs.key(key)) {
            found = request.stdin;
            break;
        }
    }
    return found;
}

/// Feeds the exit of the request under `key`, releasing the fake executor's
/// slot first: a live slot delivers on exit and frees the key, but a parked
/// fake one never does, so the next same-key read of the round would be
/// rejected without this. Production never parks; the cancel only exists here.
fn release(f: *Fixture, key: verbs.Key, output: []const u8) void {
    f.fx.cancel(verbs.key(key));
    f.exited(key, output);
}

fn releaseFailed(f: *Fixture, key: verbs.Key, code: i32, output: []const u8) void {
    f.fx.cancel(verbs.key(key));
    f.dispatch(.{ .exited = .{ .key = verbs.key(key), .code = code, .output = output } });
}

fn twoMembers(f: *Fixture) void {
    answer.apply(f.model, .{
        .key = verbs.key(.channels),
        .code = 0,
        .output =
        \\{"contract":1,"command":"channels","channels":[{"name":"bob","waypoint":"h","standing":"root","peer":"Bob","alias":"Bob","period":null,"retention":"keep","can":["send","read"]},{"name":"carol","waypoint":"h","standing":"root","peer":"3f9a1c0e7b2d","period":null,"retention":"keep","can":["send","read"]}],"groups":[{"name":"friends","members":["bob","carol"]}]}
        ,
    });
}

test "opening a group reads one member at a time, and the thread names each reply" {
    var f = Fixture.init();
    defer f.deinit();
    twoMembers(&f);
    f.dispatch(.{ .select_group = 0 });
    // One read is in flight, for the first member only: no burst of 2N spawns.
    try testing.expectEqual(@as(usize, 1), f.fx.pendingSpawnCount());
    try testing.expectEqualStrings("bob\n", lastSpawnStdin(&f, .group_theirs).?);

    release(&f, .group_theirs,
        \\{"contract":1,"command":"read","name":"bob","author":"x","alias":"Bob","height":0,"segments":[{"index":0,"acknowledged":1,"text":"yes"}]}
    );
    release(&f, .group_mine,
        \\{"contract":1,"command":"read","name":"bob","author":"me","height":0,"segments":[{"index":0,"acknowledged":0,"text":"lunch?"}]}
    );
    // The catch-up round moved on to carol by itself.
    try testing.expectEqualStrings("carol\n", lastSpawnStdin(&f, .group_theirs).?);
    release(&f, .group_theirs,
        \\{"contract":1,"command":"read","name":"carol","author":"y","height":0,"segments":[{"index":0,"acknowledged":1,"text":"no"}]}
    );
    release(&f, .group_mine,
        \\{"contract":1,"command":"read","name":"carol","author":"me","height":0,"segments":[{"index":0,"acknowledged":0,"text":"lunch?"}]}
    );

    const tree = try f.tree();
    _ = try tests.expectByLabel(tree.root, "Bob");
    _ = try tests.expectByText(tree.root, .text, "reached 2/2");
    const bubbles = f.model.groupThread(f.arena());
    try testing.expectEqual(@as(usize, 3), bubbles.len);
    try testing.expect(bubbles[0].mine);
    try testing.expectEqualStrings("Bob", bubbles[1].who);
    try testing.expectEqualStrings("3f9a1c0e7b2d", bubbles[2].who);
    // One broadcast, though two lanes hold a copy.
    var copies: usize = 0;
    for (bubbles) |b| {
        if (std.mem.eql(u8, b.text, "lunch?")) copies += 1;
    }
    try testing.expectEqual(@as(usize, 1), copies);
}

test "a member whose read fails yields the cursor to the next" {
    var f = Fixture.init();
    defer f.deinit();
    twoMembers(&f);
    f.dispatch(.{ .select_group = 0 });
    releaseFailed(&f, .group_theirs, 1,
        \\{"contract":1,"error":"gone","code":"kusanagi.unknown_channel","recover":"x"}
    );
    try testing.expectEqualStrings("carol\n", lastSpawnStdin(&f, .group_theirs).?);
}


test "a room is one read for everybody, and the next poll names each floor" {
    var f = Fixture.init();
    defer f.deinit();
    answer.apply(f.model, .{
        .key = verbs.key(.identity),
        .code = 0,
        .output =
        \\{"contract":1,"command":"identity","handle":"me00","site":"s"}
        ,
    });
    answer.apply(f.model, .{
        .key = verbs.key(.channels),
        .code = 0,
        .output =
        \\{"contract":1,"command":"channels","channels":[],"groups":[],"rooms":[{"name":"team","members":["me00","aa11","bb22"]}]}
        ,
    });
    f.dispatch(.{ .select_group = 0 });
    try testing.expectEqual(@as(usize, 1), f.fx.pendingSpawnCount());
    try testing.expectEqualStrings("team\n", lastSpawnStdin(&f, .room_read).?);

    release(&f, .room_read,
        \\{"contract":1,"command":"room","name":"team","threads":[
        \\{"author":"me00","height":0,"segments":[{"index":0,"acknowledged":0,"filed":5,"text":"hello room"}]},
        \\{"author":"aa11","height":1,"segments":[{"index":0,"acknowledged":0,"filed":4,"text":"first"},{"index":1,"acknowledged":0,"filed":6,"text":"reply"}]},
        \\{"author":"bb22","height":null,"segments":[]}]}
    );
    const bubbles = f.model.groupThread(f.arena());
    try testing.expectEqual(@as(usize, 3), bubbles.len);
    try testing.expectEqualStrings("first", bubbles[0].text);
    try testing.expect(bubbles[1].mine);
    try testing.expectEqualStrings("aa11", bubbles[2].who);

    // The poll resumes each stream from what this window holds, by handle.
    f.dispatch(.{ .poll = .{ .key = verbs.key(.poll_timer), .outcome = .fired } });
    try testing.expectEqualStrings("team\nme00=0\naa11=1\n", lastSpawnStdin(&f, .room_read).?);

    // A sentence goes on my own stream, not out to N channels.
    f.model.draft.set("lunch?");
    f.dispatch(.broadcast);
    try testing.expectEqualStrings("team\nlunch?", lastSpawnStdin(&f, .room_send).?);
}
