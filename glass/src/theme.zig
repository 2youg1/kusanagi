// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! The register glass draws in: night and steel.
//!
//! The dark scheme is the identity — a blue-black paper, one step lighter
//! surfaces, and a single dulled teal spent only where it means something:
//! the sent side of a thread, the primary button, the focus ring. The light
//! scheme is the same register on cool grey paper. No channel of any colour
//! here touches pure white or pure black; a screen that glares is not calm,
//! and this window shows a network whose resting state is silence.
//!
//! Everything else — control tables, states, motion, contrast — comes from
//! the SDK's own resolution of the system appearance, so high contrast and
//! reduced motion behave exactly as the platform asks. Only the palette,
//! the radii and the type scale are restated.

const std = @import("std");
const native_sdk = @import("native_sdk");
const canvas = native_sdk.canvas;
const Color = canvas.Color;

/// The body face registered in `main.zig`, found on this machine by `font.zig`
/// so a message in Chinese is text rather than a row of boxes. A machine holding
/// no such face leaves this id unregistered and the runtime keeps its built-in
/// Latin face; the labels are English there, so nothing comes out as a box.
/// Mono runs keep the SDK's Geist Mono.
pub const body_font_id: canvas.FontId = canvas.min_registered_font_id;

/// The darkest and lightest values any glass colour may hold, per channel.
/// Pure black and pure white are outside; the tests hold the line.
pub const channel_floor: u8 = 8;
pub const channel_ceiling: u8 = 250;

const Palette = struct {
    paper: Color,
    surface: Color,
    subtle: Color,
    pressed: Color,
    text: Color,
    muted: Color,
    border: Color,
    accent: Color,
    accent_ink: Color,
    focus_ring: Color,
};

const night: Palette = .{
    .paper = Color.rgb8(15, 18, 23),
    .surface = Color.rgb8(23, 28, 35),
    .subtle = Color.rgb8(36, 43, 53),
    .pressed = Color.rgb8(47, 56, 68),
    .text = Color.rgb8(227, 231, 237),
    .muted = Color.rgb8(143, 152, 167),
    .border = Color.rgb8(46, 54, 66),
    .accent = Color.rgb8(92, 200, 187),
    .accent_ink = Color.rgb8(15, 26, 25),
    .focus_ring = Color.rgb8(110, 180, 172),
};

const day: Palette = .{
    .paper = Color.rgb8(240, 241, 244),
    .surface = Color.rgb8(248, 249, 250),
    .subtle = Color.rgb8(228, 231, 236),
    .pressed = Color.rgb8(214, 218, 225),
    .text = Color.rgb8(28, 32, 39),
    .muted = Color.rgb8(104, 112, 126),
    .border = Color.rgb8(211, 215, 222),
    .accent = Color.rgb8(15, 118, 110),
    .accent_ink = Color.rgb8(241, 250, 248),
    .focus_ring = Color.rgb8(15, 118, 110),
};

fn paletteFor(scheme: canvas.ColorScheme) Palette {
    return switch (scheme) {
        .dark => night,
        .light => day,
    };
}

fn colorOverrides(p: Palette) canvas.ColorTokenOverrides {
    return .{
        .background = p.paper,
        .surface = p.surface,
        .surface_subtle = p.subtle,
        .surface_pressed = p.pressed,
        .text = p.text,
        .text_muted = p.muted,
        .border = p.border,
        .accent = p.accent,
        .accent_text = p.accent_ink,
        .focus_ring = p.focus_ring,
        .disabled = p.subtle,
    };
}

/// The complete token set for one appearance. High contrast keeps the
/// SDK's loud register untouched: accessibility beats brand, the same rule
/// the runtime applies to its own accent channel.
pub fn tokens(appearance: native_sdk.Appearance) canvas.DesignTokens {
    const scheme: canvas.ColorScheme = switch (appearance.color_scheme) {
        .dark => .dark,
        .light => .light,
    };
    const base = canvas.DesignTokens.theme(.{
        .color_scheme = scheme,
        .contrast = if (appearance.high_contrast) .high else .standard,
        .reduce_motion = appearance.reduce_motion,
        .pack = .house,
    });
    const shaped = base.withOverrides(.{
        .typography = .{ .font_id = body_font_id, .title_size = 18, .heading_size = 22, .display_size = 40 },
        .radius = .{ .sm = 8, .md = 10, .lg = 14, .xl = 20 },
        .controls = .{ .bubble = .{ .radius = 18 } },
    });
    if (appearance.high_contrast) return shaped;
    const p = paletteFor(scheme);
    // A secondary button is a filled step above its surface AND edged, so it
    // reads as a control on the plate and on the paper alike; outline keeps
    // the same edge with no fill.
    return shaped.withOverrides(.{
        .colors = colorOverrides(p),
        .controls = .{
            .button_secondary = .{ .background = p.subtle, .border = p.border },
            .button_outline = .{ .border = p.border },
        },
    });
}

/// The resolved colours for a scheme, for the tests that hold the palette
/// away from pure white and pure black.
pub fn palette(scheme: canvas.ColorScheme) canvas.ColorTokens {
    return colorOverrides(paletteFor(scheme)).apply(canvas.ColorTokens.theme(scheme, .standard));
}

fn channelIsInside(value: f32) bool {
    const byte: f32 = @round(value * 255);
    return byte >= @as(f32, @floatFromInt(channel_floor)) and byte <= @as(f32, @floatFromInt(channel_ceiling));
}

fn expectCalm(color: Color) !void {
    try std.testing.expect(channelIsInside(color.r));
    try std.testing.expect(channelIsInside(color.g));
    try std.testing.expect(channelIsInside(color.b));
}

test "neither scheme touches pure white or pure black" {
    inline for (.{ canvas.ColorScheme.dark, canvas.ColorScheme.light }) |scheme| {
        const colors = palette(scheme);
        try expectCalm(colors.background);
        try expectCalm(colors.surface);
        try expectCalm(colors.surface_subtle);
        try expectCalm(colors.surface_pressed);
        try expectCalm(colors.text);
        try expectCalm(colors.text_muted);
        try expectCalm(colors.border);
        try expectCalm(colors.accent);
        try expectCalm(colors.accent_text);
        try expectCalm(colors.focus_ring);
    }
}

test "the body face is the registered one and the display rung carries the check code" {
    const dark = tokens(.{ .color_scheme = .dark });
    try std.testing.expectEqual(body_font_id, dark.typography.font_id);
    try std.testing.expectEqual(@as(f32, 40), dark.typography.display_size);
    try std.testing.expectEqual(@as(f32, 18), dark.controls.bubble.radius.?);
}

test "high contrast keeps the platform's register and drops the brand palette" {
    const loud = tokens(.{ .color_scheme = .dark, .high_contrast = true });
    const stock = canvas.DesignTokens.theme(.{ .color_scheme = .dark, .contrast = .high });
    try std.testing.expectEqual(stock.colors.background, loud.colors.background);
    try std.testing.expectEqual(stock.colors.accent, loud.colors.accent);
}
