// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! A window onto one site. Wiring only: the scene, where the binary is, and
//! the runner. What the window knows is `model.zig`; what it does is
//! `update.zig`; every command line it runs is `verbs.zig`.
//!
//! Glass holds no state a kill could lose and no secret the site does not
//! already hold. Delete this directory and the network is unchanged.

const std = @import("std");
const runner = @import("runner");
const native_sdk = @import("native_sdk");
const model_mod = @import("model.zig");
const update_mod = @import("update.zig");
const theme = @import("theme.zig");
const plate = @import("plate.zig");
const font = @import("font.zig");
const strings = @import("strings.zig");

pub const panic = std.debug.FullPanic(native_sdk.debug.capturePanic);

const canvas = native_sdk.canvas;
const geometry = native_sdk.geometry;

pub const Model = model_mod.Model;
pub const Msg = update_mod.Msg;
pub const Effects = update_mod.Effects;
pub const update = update_mod.update;
pub const boot = update_mod.boot;
pub const onKey = update_mod.onKey;

const canvas_label = "glass-canvas";
const window_width: f32 = 1080;
const window_height: f32 = 720;

const app_permissions = [_][]const u8{
    native_sdk.security.permission_command,
    native_sdk.security.permission_view,
    native_sdk.security.permission_filesystem,
    native_sdk.security.permission_clipboard,
};
const shell_views = [_]native_sdk.ShellView{
    .{ .label = canvas_label, .kind = .gpu_surface, .fill = true, .role = "Conversations", .accessibility_label = "kusanagi", .gpu_present_mode = .timer, .gpu_vsync = true },
};
const shell_windows = [_]native_sdk.ShellWindow{.{
    .label = "main",
    .title = "kusanagi",
    .width = window_width,
    .height = window_height,
    .views = &shell_views,
}};
const shell_scene: native_sdk.ShellConfig = .{ .windows = &shell_windows };

pub const AppUi = canvas.Ui(Msg);
pub const app_markup = @embedFile("app.native");
pub const markup_sources = [_]canvas.ui_markup.SourceFile{
    .{ .path = "components/rail.native", .source = @embedFile("components/rail.native") },
    .{ .path = "components/thread.native", .source = @embedFile("components/thread.native") },
    .{ .path = "components/sheets.native", .source = @embedFile("components/sheets.native") },
    .{ .path = "components/more.native", .source = @embedFile("components/more.native") },
};

const GlassApp = native_sdk.UiApp(Model, Msg);

/// The one registration this machine earns, or none when it holds no face that
/// writes Chinese. `font.zig` says why having none is an ordinary state; the
/// bytes come from an arena that lives as long as the window, because the name
/// the runner teaches with outlives registration.
fn bodyFonts(choice: font.Choice, arena: std.mem.Allocator) []const GlassApp.FontRegistration {
    if (choice.refused.len > 0)
        std.debug.print("glass: the chosen face `{s}` cannot be used ({s}), so the window stays in English\n", .{ choice.refused, choice.reason });
    const face = choice.face orelse return &.{};
    // A face is decoration here: failing to hold it costs Chinese, not the app,
    // so an allocator saying no takes the same road as a machine with none.
    const held = arena.create(GlassApp.FontRegistration) catch return &.{};
    held.* = .{ .id = theme.body_font_id, .name = face.file, .ttf = face.ttf };
    return held[0..1];
}

pub fn initialModel() Model {
    return .{};
}

fn tokensFor(m: *const Model) canvas.DesignTokens {
    return theme.tokensWith(m.appearanceFor(), m.has_cjk);
}

/// The binary beside this one, or `kusanagi` on PATH. Never taken from
/// anything a peer could write: a locator names a host, not a program.
fn locateBinary(io: std.Io, into: *model_mod.Text(model_mod.path_cap)) void {
    const suffix = if (@import("builtin").os.tag == .windows) "kusanagi.exe" else "kusanagi";
    var self_buf: [std.Io.Dir.max_path_bytes]u8 = undefined;
    const self_len = std.process.executablePath(io, &self_buf) catch 0;
    if (self_len > 0) {
        if (std.fs.path.dirname(self_buf[0..self_len])) |dir| {
            var beside: [std.Io.Dir.max_path_bytes]u8 = undefined;
            const candidate = std.fmt.bufPrint(&beside, "{s}{c}{s}", .{ dir, std.fs.path.sep, suffix }) catch "";
            if (candidate.len > 0) {
                if (std.Io.Dir.cwd().statFile(io, candidate, .{})) |_| {
                    into.set(candidate);
                    return;
                } else |_| {}
            }
        }
    }
    into.set("kusanagi");
}

pub fn main(init: std.process.Init) !void {
    var face_arena = std.heap.ArenaAllocator.init(std.heap.page_allocator);
    const home = init.environ_map.get("USERPROFILE") orelse init.environ_map.get("HOME") orelse ".";
    const choice = font.choose(init.io, face_arena.allocator(), home);
    const app_state = try GlassApp.create(std.heap.page_allocator, .{
        .name = "glass",
        .scene = shell_scene,
        .canvas_label = canvas_label,
        .update_fx = update,
        .init_fx = boot,
        .on_key = onKey,
        .on_appearance = update_mod.onAppearance,
        .on_drop = update_mod.onDrop,
        .tokens_fn = tokensFor,
        .fonts = bodyFonts(choice, face_arena.allocator()),
        .chrome = .{ .prefix_commands = plate.prefix_commands, .build = plate.build },
        .markup = .{
            .source = app_markup,
            .sources = &markup_sources,
            .watch_path = "src/app.native",
            .io = init.io,
        },
    });
    defer app_state.destroy();
    app_state.model = initialModel();
    locateBinary(init.io, &app_state.model.bin);
    app_state.model.home.set(home);
    // Chinese is offered only once a face can draw it; the system's language
    // is then the default and the settings sheet can change it.
    app_state.model.has_cjk = choice.face != null;
    if (choice.face) |face| app_state.model.face.registered.set(face.file);
    if (choice.refused.len > 0) {
        const noted = std.fmt.bufPrint(&app_state.model.scratch, "{s}: {s}", .{ choice.refused, choice.reason }) catch choice.refused;
        app_state.model.face.refused.set(noted);
    }
    app_state.model.setLanguage(strings.remembered(init.io, home) orelse strings.detect(init.environ_map));

    try runner.runWithOptions(app_state.app(), .{
        .app_name = "glass",
        .window_title = "kusanagi",
        .bundle_id = "dev.kusanagi.glass",
        .icon_path = "assets/icon.png",
        .default_frame = geometry.RectF.init(0, 0, window_width, window_height),
        .js_window_api = false,
        .security = .{
            .permissions = &app_permissions,
            .navigation = .{ .allowed_origins = &.{ "zero://inline", "zero://app" } },
        },
    }, init);
}

test {
    _ = @import("tests.zig");
    _ = @import("order.zig");
    _ = @import("theme.zig");
    _ = @import("plate.zig");
    _ = @import("font.zig");
    _ = @import("strings.zig");
    _ = @import("wording.zig");
}
