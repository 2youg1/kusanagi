// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! The one surface glass draws itself: the conversation plate, a lifted
//! sheet on the paper with corners of continuous curvature.
//!
//! A corner here is not a circular arc. Along its length s ∈ [0, L] the
//! curvature is κ(s) = K · b(s/L) with b(u) = 64·u³·(1−u)³, a bump with a
//! triple zero at each end: κ, κ′ and κ″ all vanish where the curve meets a
//! straight edge, so the edge flows into the corner with nothing for the eye
//! to catch — fourth-order geometric continuity, G4. The bump integrates to
//! 16/35, so K = 35π/(32L) turns the curve through exactly a right angle.
//!
//! The plate's frame derives from the window size and one constant, the
//! rail's width, never from a layout read back after the fact: the chrome is
//! emitted before the widgets of the same rebuild, so anything read back
//! would be one rebuild stale. `tests.zig` holds the constant and the markup
//! in step.

const std = @import("std");
const native_sdk = @import("native_sdk");
const canvas = native_sdk.canvas;
const geometry = native_sdk.geometry;
const Model = @import("model.zig").Model;

/// The rail's width, the same number the markup gives its column.
pub const rail_width: f32 = 248;
/// How far the plate sits from every edge of its pane; the pane's content
/// column pads by the same amount, so widgets start at the plate's edge.
pub const inset: f32 = 12;
/// How much of each edge a corner consumes. Continuous curvature spends
/// most of its turn in the middle of the corner, so this is about twice
/// the circular radius it reads as.
pub const corner: f32 = 56;
/// Exactly what `build` emits: shadow, fill, hairline.
pub const prefix_commands: usize = 3;

/// Line segments per corner. Enough that a corner reads as a curve at any
/// scale factor; four corners stay well under the per-frame path budget.
pub const samples: usize = 24;
const substeps: usize = 8;

/// The unit corner: starts at the origin heading +x, ends at (extent,
/// extent) heading +y, symmetric about the diagonal.
pub const Corner = struct {
    points: [samples + 1]geometry.PointF,
    extent: f32,
};

fn bump(u: f64) f64 {
    const v = u * (1 - u);
    return 64 * v * v * v;
}

/// Midpoint integration of the first half, then the mirror: reflecting the
/// half-curve across the normal at its midpoint gives the second half
/// exactly, so the end lands at (extent, extent) to the bit.
fn integrate() Corner {
    @setEvalBranchQuota(20_000);
    const bump_area = 16.0 / 35.0;
    const k = (std.math.pi / 2.0) / bump_area;
    const half = samples / 2;
    const step = 1.0 / @as(f64, @floatFromInt(samples * substeps));
    var theta: f64 = 0;
    var x: f64 = 0;
    var y: f64 = 0;
    var points: [samples + 1]geometry.PointF = undefined;
    points[0] = geometry.PointF.init(0, 0);
    var sample: usize = 1;
    while (sample <= half) : (sample += 1) {
        var sub: usize = 0;
        while (sub < substeps) : (sub += 1) {
            const mid = (@as(f64, @floatFromInt((sample - 1) * substeps + sub)) + 0.5) * step;
            const turn = k * bump(mid) * step;
            const direction = theta + turn / 2;
            x += @cos(direction) * step;
            y += @sin(direction) * step;
            theta += turn;
        }
        points[sample] = geometry.PointF.init(@floatCast(x), @floatCast(y));
    }
    const extent: f32 = @floatCast(x + y);
    var i: usize = 0;
    while (i < half) : (i += 1) {
        points[samples - i] = geometry.PointF.init(extent - points[i].y, extent - points[i].x);
    }
    return .{ .points = points, .extent = extent };
}

pub const unit: Corner = integrate();

/// The pane to the right of the rail, then the plate inside it.
pub fn paneFrame(size: geometry.SizeF) geometry.RectF {
    return geometry.RectF.init(rail_width, 0, @max(0, size.width - rail_width), size.height);
}

pub fn frame(size: geometry.SizeF) geometry.RectF {
    return paneFrame(size).deflate(geometry.InsetsF.init(inset, inset, inset, inset));
}

/// The closed outline of `rect` with G4 corners of the given extent, as a
/// path in the builder's element store. Clockwise from the top edge.
pub fn outline(builder: *canvas.Builder, rect: geometry.RectF, extent: f32) ![]const canvas.PathElement {
    const elements = try builder.allocPathElements(6 + 4 * samples);
    const scale = extent / unit.extent;
    var n: usize = 0;
    elements[n] = element(.move_to, geometry.PointF.init(rect.x + extent, rect.y));
    n += 1;
    elements[n] = element(.line_to, geometry.PointF.init(rect.maxX() - extent, rect.y));
    n += 1;
    // Each corner maps the unit curve by a quarter turn: the unit curve
    // heads +x and turns towards +y, which on a y-down screen is the top-right
    // corner as drawn; the other three are that curve rotated.
    for (unit.points[1..]) |p| {
        elements[n] = element(.line_to, geometry.PointF.init(rect.maxX() - extent + p.x * scale, rect.y + p.y * scale));
        n += 1;
    }
    elements[n] = element(.line_to, geometry.PointF.init(rect.maxX(), rect.maxY() - extent));
    n += 1;
    for (unit.points[1..]) |p| {
        elements[n] = element(.line_to, geometry.PointF.init(rect.maxX() - p.y * scale, rect.maxY() - extent + p.x * scale));
        n += 1;
    }
    elements[n] = element(.line_to, geometry.PointF.init(rect.x + extent, rect.maxY()));
    n += 1;
    for (unit.points[1..]) |p| {
        elements[n] = element(.line_to, geometry.PointF.init(rect.x + extent - p.x * scale, rect.maxY() - p.y * scale));
        n += 1;
    }
    elements[n] = element(.line_to, geometry.PointF.init(rect.x, rect.y + extent));
    n += 1;
    for (unit.points[1..]) |p| {
        elements[n] = element(.line_to, geometry.PointF.init(rect.x + p.y * scale, rect.y + extent - p.x * scale));
        n += 1;
    }
    elements[n] = element(.close, geometry.PointF.init(0, 0));
    n += 1;
    return elements[0..n];
}

fn element(verb: canvas.PathVerb, point: geometry.PointF) canvas.PathElement {
    return .{ .verb = verb, .points = .{ point, point, point } };
}

/// The chrome prefix: a soft shadow, the plate, and its hairline. Exactly
/// `prefix_commands` commands, whatever the model holds.
pub fn build(m: *const Model, builder: *canvas.Builder, size: geometry.SizeF, tokens: canvas.DesignTokens) anyerror!void {
    _ = m;
    const rect = frame(size);
    const extent = @min(corner, rect.width / 2, rect.height / 2);
    try builder.shadow(.{
        .rect = rect,
        .radius = canvas.Radius.all(extent / 2),
        .offset = geometry.OffsetF.init(0, 6),
        .blur = 18,
        .color = tokens.colors.shadow,
    });
    const path = try outline(builder, rect, extent);
    try builder.fillPath(.{ .elements = path, .fill = .{ .color = tokens.colors.surface } });
    try builder.strokePath(.{ .elements = path, .stroke = .{ .fill = .{ .color = tokens.colors.border }, .width = tokens.stroke.hairline } });
}

fn heading(from: geometry.PointF, to: geometry.PointF) f64 {
    return std.math.atan2(@as(f64, to.y - from.y), @as(f64, to.x - from.x));
}

test "the unit corner turns through a right angle and ends on the diagonal" {
    const points = unit.points;
    try std.testing.expectApproxEqAbs(@as(f64, 0), heading(points[0], points[1]), 1e-3);
    try std.testing.expectApproxEqAbs(std.math.pi / 2.0, heading(points[samples - 1], points[samples]), 1e-3);
    var turned: f64 = 0;
    var i: usize = 1;
    while (i < samples) : (i += 1) {
        turned += heading(points[i], points[i + 1]) - heading(points[i - 1], points[i]);
    }
    try std.testing.expectApproxEqAbs(std.math.pi / 2.0, turned, 1e-3);
    try std.testing.expectApproxEqAbs(unit.extent, points[samples].x, 1e-6);
    try std.testing.expectApproxEqAbs(unit.extent, points[samples].y, 1e-6);
}

test "the curvature bump has a triple zero at both ends" {
    const h = 1e-3;
    inline for (.{ 0.0, 1.0 }) |end| {
        const inside: f64 = if (end == 0.0) h else 1 - h;
        try std.testing.expectApproxEqAbs(@as(f64, 0), bump(end), 1e-12);
        // A triple zero: the value h from the end is O(h³), far below O(h).
        try std.testing.expect(bump(inside) < h * h);
    }
}

test "the plate sits inside its pane, one inset from every edge" {
    const size = geometry.SizeF.init(1080, 720);
    const pane = paneFrame(size);
    const drawn = frame(size);
    try std.testing.expectEqual(rail_width, pane.x);
    try std.testing.expect(pane.containsRect(drawn));
    try std.testing.expectEqual(@as(f32, 720 - 2 * inset), drawn.height);
    try std.testing.expectEqual(@as(f32, 1080 - rail_width - 2 * inset), drawn.width);
}

test "build emits exactly the declared prefix" {
    var commands: [16]canvas.CanvasCommand = undefined;
    var builder = canvas.Builder.init(&commands);
    const model = try std.heap.page_allocator.create(Model);
    defer std.heap.page_allocator.destroy(model);
    model.* = .{};
    try build(model, &builder, geometry.SizeF.init(1080, 720), canvas.DesignTokens.theme(.{}));
    try std.testing.expectEqual(prefix_commands, builder.displayList().commands.len);
}
