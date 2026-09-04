// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! The window driven the way the runtime drives it — real markup, real
//! dispatch, a fake effects executor that records what would have been run.
//!
//! What these hold: no name, invitation or message ever reaches argv; a click
//! on a row reads that channel; an answer from the verb lands in the model;
//! every pane and sheet builds; and the plate drawn under the widgets lies
//! exactly where the widgets expect it.

const std = @import("std");
const native_sdk = @import("native_sdk");
const main = @import("main.zig");
const verbs = @import("verbs.zig");
const answer = @import("answer.zig");
const plate = @import("plate.zig");
const model_mod = @import("model.zig");

const canvas = native_sdk.canvas;
const geometry = native_sdk.geometry;
const testing = std.testing;

const AppUi = main.AppUi;
const Model = main.Model;
const Msg = main.Msg;
const Effects = main.Effects;

const AppMarkup = canvas.MarkupView(Model, Msg);

const Fixture = struct {
    arena_state: std.heap.ArenaAllocator,
    fx: Effects,
    model: *Model,

    fn init() Fixture {
        var fx = Effects.init(testing.allocator);
        fx.executor = .fake;
        return .{ .arena_state = std.heap.ArenaAllocator.init(testing.allocator), .fx = fx, .model = modelWithBob() };
    }
    fn deinit(f: *Fixture) void {
        f.fx.deinit();
        f.arena_state.deinit();
        std.heap.page_allocator.destroy(f.model);
    }
    fn arena(f: *Fixture) std.mem.Allocator {
        return f.arena_state.allocator();
    }
    fn dispatch(f: *Fixture, msg: Msg) void {
        main.update(f.model, msg, &f.fx);
    }
    fn tree(f: *Fixture) !AppUi.Tree {
        return buildTree(f.arena(), f.model);
    }
    fn exited(f: *Fixture, key: verbs.Key, output: []const u8) void {
        f.dispatch(.{ .exited = .{ .key = verbs.key(key), .code = 0, .output = output } });
    }
};

fn buildTree(arena: std.mem.Allocator, model: *const Model) !AppUi.Tree {
    // The same path the runtime takes: imports resolve against the embedded
    // source set, never the disk.
    var set_loader = canvas.ui_markup.SourceSetLoader{ .set = &main.markup_sources };
    var diagnostic: canvas.ui_markup.MarkupErrorInfo = .{};
    const document = canvas.ui_markup.resolveImports(arena, "", main.app_markup, set_loader.loader(), &diagnostic) catch |err| {
        std.debug.print("{s}:{d}:{d}: {s}\n", .{ diagnostic.path, diagnostic.line, diagnostic.column, diagnostic.message });
        return err;
    };
    var view = AppMarkup.fromDocument(try canvas.ui_markup.canonicalize(arena, document));
    var ui = AppUi.init(arena);
    const node = view.build(&ui, model) catch |err| {
        if (err == error.MarkupBuild) {
            std.debug.print("app.native:{d}:{d}: {s}\n", .{ view.diagnostic.line, view.diagnostic.column, view.diagnostic.message });
        }
        return err;
    };
    return ui.finalize(node);
}

fn findByText(widget: canvas.Widget, kind: canvas.WidgetKind, text: []const u8) ?canvas.Widget {
    // A `label=` replaces the announced name, so a row is found by either.
    if (widget.kind == kind and (std.mem.eql(u8, widget.text, text) or std.mem.eql(u8, widget.semantics.label, text))) return widget;
    for (widget.children) |child| {
        if (findByText(child, kind, text)) |found| return found;
    }
    return null;
}

fn expectByText(widget: canvas.Widget, kind: canvas.WidgetKind, text: []const u8) !canvas.Widget {
    return findByText(widget, kind, text) orelse {
        std.debug.print("no {t} with text \"{s}\" in the view\n", .{ kind, text });
        return error.WidgetNotFound;
    };
}

fn findByLabel(widget: canvas.Widget, label: []const u8) ?canvas.Widget {
    if (std.mem.eql(u8, widget.semantics.label, label)) return widget;
    for (widget.children) |child| {
        if (findByLabel(child, label)) |found| return found;
    }
    return null;
}

/// A model that has already heard `channels` answer with one channel.
fn modelWithBob() *Model {
    const model = std.heap.page_allocator.create(Model) catch unreachable;
    model.* = main.initialModel();
    model.bin.set("kusanagi");
    answer.apply(model, .{
        .key = verbs.key(.channels),
        .code = 0,
        .output =
        \\{"contract":1,"command":"channels","channels":[{"name":"bob","waypoint":"http://127.0.0.1:8963","standing":"root","peer":"3f9a1c0e7b2d","period":null,"retention":"keep","can":["send","read"]}],"groups":[{"name":"friends","members":["bob"]}]}
        ,
    });
    return model;
}

fn argvHolds(fx: *Effects, needle: []const u8) bool {
    var i: usize = 0;
    while (i < fx.pendingSpawnCount()) : (i += 1) {
        const request = fx.pendingSpawnAt(i).?;
        for (request.argv) |arg| {
            if (std.mem.indexOf(u8, arg, needle) != null) return true;
        }
    }
    return false;
}

fn stdinOf(fx: *Effects, key: u64) ?[]const u8 {
    var i: usize = 0;
    while (i < fx.pendingSpawnCount()) : (i += 1) {
        const request = fx.pendingSpawnAt(i).?;
        if (request.key == key) return request.stdin;
    }
    return null;
}

test "clicking a channel reads it, with the name on stdin and never in argv" {
    var f = Fixture.init();
    defer f.deinit();
    var tree = try f.tree();
    const row = try expectByText(tree.root, .list_item, "bob");
    f.dispatch(tree.msgForPointer(row.id, .up).?);

    try testing.expectEqual(model_mod.Screen.thread, f.model.screen);
    try testing.expect(f.fx.pendingSpawnCount() >= 2);
    try testing.expect(!argvHolds(&f.fx, "bob"));
    try testing.expectEqualStrings("bob\n", stdinOf(&f.fx, verbs.key(.read_theirs)).?);
    try testing.expect(argvHolds(&f.fx, "--mine"));

    tree = try f.tree();
    _ = try expectByText(tree.root, .text, "bob");
}

test "sending puts the name and the text on stdin, then shows the segment" {
    var f = Fixture.init();
    defer f.deinit();
    f.dispatch(.{ .select = 0 });
    f.model.draft.set("hello bob");
    f.dispatch(.send);

    try testing.expectEqual(verbs.key(.send), f.model.busy);
    try testing.expectEqualStrings("bob\nhello bob", stdinOf(&f.fx, verbs.key(.send)).?);
    try testing.expect(!argvHolds(&f.fx, "hello"));

    f.exited(.send,
        \\{"contract":1,"command":"sent","name":"bob","index":0,"id":"x","address":"y"}
    );
    try testing.expectEqual(@as(u64, 0), f.model.busy);
    try testing.expectEqual(@as(usize, 1), f.model.mine.count);
    try testing.expectEqualStrings("", f.model.draft.text());
    var tree = try f.tree();
    _ = try expectByText(tree.root, .text, "hello bob");

    // Their reply sits at index 0 on their own stream, beside mine at index 0
    // on mine: two segments, one index, and the view must still build.
    f.exited(.read_theirs,
        \\{"contract":1,"command":"read","name":"bob","author":"3f9a","height":0,"segments":[{"index":0,"acknowledged":1,"text":"hi back"}]}
    );
    tree = try f.tree();
    _ = try expectByText(tree.root, .text, "hi back");
    _ = try expectByText(tree.root, .text, "hello bob");
    const bubbles = f.model.thread(f.arena());
    try testing.expectEqual(@as(usize, 2), bubbles.len);
    try testing.expect(bubbles[0].turn and bubbles[1].turn);
}

test "an invitation answer shows the line and the four characters, spaced" {
    var f = Fixture.init();
    defer f.deinit();
    f.dispatch(.show_invite);
    f.model.invite.name.set("carol");
    f.model.invite.waypoint.set("http://127.0.0.1:8963");
    f.dispatch(.mint);
    try testing.expectEqualStrings("carol\n", stdinOf(&f.fx, verbs.key(.invite)).?);
    try testing.expect(!argvHolds(&f.fx, "carol"));

    f.exited(.invite,
        \\{"contract":1,"command":"invited","name":"carol","invite":"kusanagi2:0201ab","check":"7f3a","expires_at":1,"expires_in":604800}
    );
    const tree = try f.tree();
    _ = try expectByText(tree.root, .text, "7 f 3 a");
    _ = try expectByText(tree.root, .text, "kusanagi2:0201ab");
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

test "every pane and sheet builds: group, roster, doctor, backup, join, forget" {
    var f = Fixture.init();
    defer f.deinit();
    f.dispatch(.{ .select_group = 0 });
    f.exited(.fanout,
        \\{"contract":1,"command":"fanned_out","name":"friends","delivered":[{"member":"bob","status":"sent"},{"member":"carol","status":"failed","error":"no_peer_yet"}]}
    );
    var tree = try f.tree();
    _ = try expectByText(tree.root, .text, "friends");
    _ = try expectByText(tree.root, .text, "no_peer_yet");

    f.dispatch(.show_roster);
    tree = try f.tree();
    _ = try expectByText(tree.root, .checkbox, "bob");

    f.dispatch(.show_doctor);
    f.model.doctor.waypoint.set("http://127.0.0.1:8963");
    f.dispatch(.examine);
    f.exited(.doctor,
        \\{"contract":1,"command":"examined","waypoint":"http://127.0.0.1:8963","kind":"http","tier":"box","capabilities":[{"capability":"write_once","verdict":"held"},{"capability":"expiry","verdict":"unknown","detail":"no ttl header"}]}
    );
    tree = try f.tree();
    _ = try expectByText(tree.root, .text, "no ttl header");

    f.dispatch(.show_backup);
    f.dispatch(.export_now);
    f.dispatch(.{ .exited = .{ .key = verbs.key(.export_), .code = 0, .output = "ARCHIVE", .stderr_tail =
        \\{"contract":1,"command":"exported","recovery":"word word word"}
    } });
    tree = try f.tree();
    _ = try expectByText(tree.root, .text, "word word word");
    try testing.expectEqualStrings("ARCHIVE", f.fx.pendingFileAt(0).?.bytes);

    f.dispatch(.show_join);
    tree = try f.tree();
    _ = try expectByText(tree.root, .text, "Accept an invitation");
    f.dispatch(.show_forget);
    tree = try f.tree();
    _ = try expectByText(tree.root, .text, "Forget this channel?");
}

test "the settings sheet opens from the rail and a chosen look overrides the system's" {
    var f = Fixture.init();
    defer f.deinit();
    f.exited(.identity,
        \\{"contract":1,"command":"id","handle":"89958e2fc5440a1b"}
    );
    var tree = try f.tree();
    const door = try expectByText(tree.root, .list_item, "This endpoint");
    f.dispatch(tree.msgForPointer(door.id, .up).?);
    try testing.expect(f.model.sheetSettings());
    tree = try f.tree();
    _ = try expectByText(tree.root, .text, "89958e2f c5440a1b");
    _ = try expectByText(tree.root, .button, "Measure a host");

    f.dispatch(.{ .appearance = .{ .color_scheme = .light } });
    try testing.expectEqual(native_sdk.Appearance{ .color_scheme = .light }, f.model.appearanceFor());
    f.dispatch(.{ .set_look = .dark });
    try testing.expectEqual(native_sdk.platform.ColorScheme.dark, f.model.appearanceFor().color_scheme);
    f.dispatch(.close_sheet);
    try testing.expectEqual(model_mod.Sheet.none, f.model.sheet);
    tree = try f.tree();
    _ = try expectByText(tree.root, .list_item, "bob");
}

test "a mint on the welcome page keeps its sheet when the channel list answers" {
    var f = Fixture.init();
    defer f.deinit();
    f.model.channel_count = 0;
    f.dispatch(.show_invite);
    try testing.expect(f.model.sheetInvite());
    // The mint exits; `channels` is re-spawned; its answer lands while the
    // sheet is still up and the window walks over to read the new channel.
    f.exited(.channels,
        \\{"contract":1,"command":"channels","channels":[{"name":"carol","waypoint":"http://127.0.0.1:8963","standing":"root","peer":null,"period":null,"retention":"keep","can":["send","read"]}],"groups":[]}
    );
    try testing.expectEqual(model_mod.Screen.thread, f.model.screen);
    try testing.expect(f.model.sheetInvite());
    const tree = try f.tree();
    _ = try expectByText(tree.root, .text, "New invitation");
    _ = try expectByText(tree.root, .text, "carol");
}

test "the plate lies inside the thread pane and the pane's content lies inside the plate" {
    var f = Fixture.init();
    defer f.deinit();
    f.dispatch(.{ .select = 0 });
    const tree = try f.tree();
    const size = geometry.SizeF.init(1080, 720);
    var nodes: [1024]canvas.WidgetLayoutNode = undefined;
    const layout = try canvas.layoutWidgetTree(tree.root, geometry.RectF.fromSize(size), &nodes);
    const pane = findByLabel(tree.root, "Thread") orelse return error.WidgetNotFound;
    const messages = findByLabel(tree.root, "Messages") orelse return error.WidgetNotFound;
    const pane_frame = layout.findById(pane.id).?.frame;
    const messages_frame = layout.findById(messages.id).?.frame;
    const drawn = plate.frame(size);
    try testing.expectApproxEqAbs(pane_frame.x, plate.paneFrame(size).x, 0.5);
    try testing.expectApproxEqAbs(pane_frame.width, plate.paneFrame(size).width, 0.5);
    try testing.expect(pane_frame.containsRect(drawn));
    try testing.expect(drawn.containsRect(messages_frame));
}
