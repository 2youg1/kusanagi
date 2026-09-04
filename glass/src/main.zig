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

/// The body face: Noto Sans SC, fetched by `just glass-fonts` and checked
/// against its published hash, so a message in Chinese is text. OFL 1.1;
/// the licence sits beside the file.
const app_fonts = [_]GlassApp.FontRegistration{
    .{ .id = theme.body_font_id, .name = "NotoSansSC.ttf", .ttf = @embedFile("fonts/NotoSansSC.ttf") },
};

pub fn initialModel() Model {
    return .{};
}

fn tokensFor(m: *const Model) canvas.DesignTokens {
    return theme.tokens(m.appearanceFor());
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
    const app_state = try GlassApp.create(std.heap.page_allocator, .{
        .name = "glass",
        .scene = shell_scene,
        .canvas_label = canvas_label,
        .update_fx = update,
        .init_fx = boot,
        .on_key = onKey,
        .on_appearance = update_mod.onAppearance,
        .tokens_fn = tokensFor,
        .fonts = &app_fonts,
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
    const home = init.environ_map.get("USERPROFILE") orelse init.environ_map.get("HOME") orelse ".";
    app_state.model.home.set(home);

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
}
