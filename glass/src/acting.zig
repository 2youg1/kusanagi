// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! What the sheets and composers do when their button is pressed: each one
//! guards on `busy` and readiness, marks itself busy, and spawns one verb.
//!
//! Apart from `update.zig` because that dispatch is at its line limit, and
//! because every function here has one shape — check, mark, spawn — that the
//! dispatch only needs to name.

const std = @import("std");
const native_sdk = @import("native_sdk");
const model_mod = @import("model.zig");
const update = @import("update.zig");
const verbs = @import("verbs.zig");

const Model = model_mod.Model;
const Effects = update.Effects;

pub fn showInvite(m: *Model) void {
    m.sheet = .invite;
    m.invite.line.clear();
    m.check.clear();
    if (m.invite.waypoint.text().len == 0 and m.onThread()) m.invite.waypoint.set(m.currentWaypoint());
}

pub fn showDoctor(m: *Model) void {
    m.sheet = .doctor;
    m.doctor.clear();
    if (m.doctor.waypoint.text().len == 0 and m.onThread()) m.doctor.waypoint.set(m.currentWaypoint());
}

pub fn sendDraft(m: *Model, fx: *Effects) void {
    if (!m.canSend()) return;
    m.busy = verbs.key(.send);
    verbs.send(fx, m, m.currentName(), m.draft.text(), &m.scratch);
}

/// Mints an invitation. For a room: into the room of that name when this
/// endpoint holds one, otherwise the room is founded first and the invitation
/// follows on its exit (`update.exited`).
pub fn mint(m: *Model, fx: *Effects) void {
    if (m.isBusy() or !m.invite.ready()) return;
    if (!m.invite.room) {
        m.busy = verbs.key(.invite);
        return verbs.invite(fx, m, m.invite.nameText(), m.invite.waypointText(), .{ .every = m.invite.period(), .release = m.invite.release }, &m.scratch);
    }
    if (m.holdsRoom(m.invite.nameText())) {
        m.busy = verbs.key(.room_invite);
        return verbs.roomInvite(fx, m, m.invite.nameText(), &m.scratch);
    }
    m.busy = verbs.key(.room);
    verbs.room(fx, m, m.invite.nameText(), m.invite.waypointText(), &m.scratch);
}

pub fn accept(m: *Model, fx: *Effects) void {
    if (m.isBusy() or !m.join.ready()) return;
    if (m.join.room) {
        m.busy = verbs.key(.room_join);
        return verbs.roomJoin(fx, m, m.join.nameText(), m.join.invitationText(), &m.scratch);
    }
    m.busy = verbs.key(.join);
    verbs.join(fx, m, m.join.nameText(), m.join.invitationText(), m.join.release, &m.scratch);
}

pub fn exportSite(m: *Model, fx: *Effects) void {
    if (m.isBusy()) return;
    m.busy = verbs.key(.export_);
    verbs.exportSite(fx, m);
}

pub fn saveRoster(m: *Model, fx: *Effects) void {
    if (m.isBusy() or !m.roster.ready()) return;
    var names: [model_mod.max_channels][]const u8 = undefined;
    const members = m.roster.ticked(m.channel_count, &names);
    m.sheet = .none;
    m.busy = verbs.key(.group);
    verbs.group(fx, m, m.roster.nameText(), members, &m.scratch);
}

/// One sentence to the open group: a fan-out over its channels, or one
/// segment on my own stream when the row is a room.
pub fn broadcast(m: *Model, fx: *Effects) void {
    if (m.isBusy() or !m.onGroup() or m.draft.text().len == 0) return;
    if (m.groupIsRoom()) {
        m.busy = verbs.key(.room_send);
        return verbs.roomSend(fx, m, m.groupTitle(), m.draft.text(), &m.scratch);
    }
    m.busy = verbs.key(.fanout);
    verbs.fanout(fx, m, m.groupTitle(), m.draft.text(), &m.scratch);
}

pub fn examine(m: *Model, fx: *Effects) void {
    if (m.isBusy() or !m.doctor.ready()) return;
    m.busy = verbs.key(.doctor);
    verbs.doctor(fx, m, m.doctor.waypointText());
}

pub fn writeArchive(m: *Model, fx: *Effects) void {
    const path = std.fmt.bufPrint(&m.backup.path.buf, "{s}{c}kusanagi-backup-{d}.ksnb", .{ m.home.slice(), std.fs.path.sep, fx.wallMs() }) catch return;
    m.backup.path.len = path.len;
    fx.writeFile(.{ .key = verbs.key(.backup_file), .path = path, .bytes = m.backup.bytes(), .on_result = Effects.fileMsg(.filed) });
}

pub fn filed(m: *Model, result: native_sdk.EffectFileResult) void {
    m.backup.written = result.outcome == .ok;
    if (m.backup.written) return;
    m.status.code.set("glass.backup_unwritten");
    m.status.error_text.set(m.t.err_backup_unwritten);
    m.status.recover.set(m.t.rec_backup_unwritten);
}
