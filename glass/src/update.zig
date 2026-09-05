// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! Every message the window can receive, and what each one changes.
//!
//! One arm per message; a side effect is only ever a spawn, a timer, a file
//! write or a clipboard write on the effects channel. The verbs run in a
//! subprocess and answer as JSON, so this function never touches the disk, the
//! network or a secret. Killing the window mid-anything loses the window.

const std = @import("std");
const native_sdk = @import("native_sdk");
const canvas = native_sdk.canvas;
const model_mod = @import("model.zig");
const polling = @import("polling.zig");
const verbs = @import("verbs.zig");
const answer = @import("answer.zig");
const font = @import("font.zig");
const strings = @import("strings.zig");

pub const Model = model_mod.Model;

pub const Msg = union(enum) {
    appearance: native_sdk.Appearance,
    show_settings,
    set_look: model_mod.Look,
    set_language: model_mod.Language,
    face_path_edit: canvas.TextInputEvent,
    try_face,
    /// A file dropped onto the window. The path is borrowed from the drop
    /// event: the runtime dispatches inside the callback, so `update` copies
    /// it into the model before the event goes away. A Msg is stored in every
    /// control slot of every widget, so it must stay small.
    dropped: []const u8,
    streamed: native_sdk.EffectFileResult,
    preferred: native_sdk.EffectFileResult,
    copy_handle,
    select: usize,
    select_group: usize,
    refresh,
    show_invite,
    show_join,
    show_backup,
    show_roster,
    show_doctor,
    show_forget,
    close_sheet,
    dismiss_status,
    search_edit: canvas.TextInputEvent,
    draft_edit: canvas.TextInputEvent,
    send,
    invite_name_edit: canvas.TextInputEvent,
    invite_waypoint_edit: canvas.TextInputEvent,
    invite_every_edit: canvas.TextInputEvent,
    toggle_invite_release,
    mint,
    copy_invite,
    copy_check,
    join_name_edit: canvas.TextInputEvent,
    join_text_edit: canvas.TextInputEvent,
    toggle_join_release,
    accept,
    export_now,
    copy_recovery,
    roster_name_edit: canvas.TextInputEvent,
    toggle_member: usize,
    save_roster,
    broadcast,
    doctor_edit: canvas.TextInputEvent,
    examine,
    confirm_forget,
    revoke,
    exited: native_sdk.EffectExit,
    filed: native_sdk.EffectFileResult,
    poll: native_sdk.EffectTimer,
    slot: native_sdk.EffectTimer,
    scrub: native_sdk.EffectTimer,
    scrubbing: native_sdk.EffectClipboardResult,

    pub const view_unbound = .{ "appearance", "exited", "filed", "poll", "slot", "scrub", "scrubbing", "dropped", "streamed", "preferred" };
};

pub const Effects = native_sdk.Effects(Msg);

/// How often an open on-demand thread asks the host for one address.
pub const poll_ms: u64 = 20_000;

/// How long something this window copied may stay on the clipboard.
///
/// The clipboard is a log: Windows keeps a history of it and offers to sync
/// it across devices, and every process on the machine may read it. An
/// invitation carries a channel key, so it is taken back once the person has
/// had a minute to paste it — and only if it is still what was copied, so a
/// later copy of theirs is never touched.
pub const scrub_ms: u64 = 60_000;

/// Boot: this machine's report, this endpoint's handle, and the channel list.
pub fn boot(m: *Model, fx: *Effects) void {
    if (m.noBinary()) return;
    verbs.here(fx, m);
    verbs.identity(fx, m);
    verbs.channels(fx, m);
}

pub fn update(m: *Model, msg: Msg, fx: *Effects) void {
    switch (msg) {
        .appearance => |appearance| m.appearance = appearance,
        .show_settings => m.sheet = .settings,
        .set_look => |look| m.look = look,
        .set_language => |language| {
            m.setLanguage(language);
            // The choice applies now whatever the disk says; the file only
            // spares the next start from guessing, so its result needs no arm.
            const where = std.fmt.bufPrint(&m.scratch, "{s}{c}{s}", .{ m.home.slice(), std.fs.path.sep, strings.preference_file }) catch return;
            // A second choice before the first write landed replaces it.
            fx.cancel(verbs.key(.language_file));
            fx.writeFile(.{ .key = verbs.key(.language_file), .path = where, .bytes = @tagName(m.language) });
        },
        .face_path_edit => |edit| m.face.path.apply(edit),
        .try_face => tryFace(m, fx),
        .dropped => |path| {
            m.face.path.set(path);
            m.sheet = .settings;
            tryFace(m, fx);
        },
        .streamed => |result| streamed(m, fx, result),
        .preferred => |result| {
            m.face.saved = result.outcome == .ok;
            if (!m.face.saved) m.face.verdict.set(m.t.face_unsaved);
        },
        .copy_handle => copy(m, fx, m.handle.slice()),
        .select => |slot| open(m, fx, slot),
        .select_group => |slot| polling.open(m, fx, slot),
        .refresh => {
            verbs.channels(fx, m);
            if (m.onThread()) fetch(m, fx);
        },
        .show_invite => showInvite(m),
        .show_join => {
            m.sheet = .join;
            m.check.clear();
        },
        .show_backup => m.sheet = .backup,
        .show_roster => {
            m.sheet = .roster;
            m.roster.prefill(m.channelRows(), if (m.onGroup()) m.currentGroup() else null);
        },
        .show_doctor => showDoctor(m),
        .show_forget => m.sheet = .forget,
        .close_sheet => m.sheet = .none,
        .dismiss_status => m.status.clear(),
        .search_edit => |edit| m.search.apply(edit),
        .draft_edit => |edit| m.draft.apply(edit),
        .send => sendDraft(m, fx),
        .invite_name_edit => |edit| m.invite.name.apply(edit),
        .invite_waypoint_edit => |edit| m.invite.waypoint.apply(edit),
        .invite_every_edit => |edit| m.invite.every.apply(edit),
        .toggle_invite_release => m.invite.release = !m.invite.release,
        .mint => mint(m, fx),
        .copy_invite => copy(m, fx, m.invite.lineText()),
        .copy_check => copy(m, fx, m.check.slice()),
        .join_name_edit => |edit| m.join.name.apply(edit),
        .join_text_edit => |edit| m.join.invitation.apply(edit),
        .toggle_join_release => m.join.release = !m.join.release,
        .accept => accept(m, fx),
        .export_now => exportSite(m, fx),
        .copy_recovery => copy(m, fx, m.backup.recoveryKey()),
        .roster_name_edit => |edit| m.roster.name.apply(edit),
        .toggle_member => |slot| m.roster.toggle(slot, m.channel_count),
        .save_roster => saveRoster(m, fx),
        .broadcast => broadcast(m, fx),
        .doctor_edit => |edit| m.doctor.waypoint.apply(edit),
        .examine => examine(m, fx),
        .confirm_forget => {
            if (!m.onThread()) return;
            m.sheet = .none;
            m.busy = verbs.key(.forget);
            verbs.forget(fx, m, m.currentName(), &m.name_scratch);
        },
        .revoke => {
            if (!m.onThread()) return;
            m.sheet = .none;
            m.busy = verbs.key(.revoke);
            verbs.revoke(fx, m, m.currentName(), &m.name_scratch);
        },
        .exited => |exit| exited(m, fx, exit),
        .filed => |result| filed(m, result),
        .poll => |timer| {
            if (timer.outcome != .fired) return;
            if (m.onGroup()) return polling.step(m, fx);
            if (!m.onThread() or m.current().slotted()) return;
            verbs.read(fx, m, .read_theirs, m.currentName(), m.theirs.height, &m.name_scratch);
        },
        .scrub => |timer| {
            if (timer.outcome != .fired or m.copied.isEmpty()) return;
            fx.readClipboard(.{ .key = verbs.key(.clipboard_read), .on_result = Effects.clipboardMsg(.scrubbing) });
        },
        .scrubbing => |result| {
            defer m.copied.clear();
            if (result.op != .read or result.outcome != .ok) return;
            if (!std.mem.eql(u8, result.text, m.copied.slice())) return;
            fx.writeClipboard(.{ .key = verbs.key(.clipboard_scrub), .text = "" });
        },
        .slot => |timer| {
            if (timer.outcome != .fired or !m.onThread() or !m.current().slotted()) return;
            verbs.tick(fx, m, m.currentName(), &m.name_scratch);
        },
    }
}

/// Keys the view never binds: `kusanagi`'s own shortcuts, when nothing focused
/// wants the key.
pub fn onKey(keyboard: canvas.WidgetKeyboardEvent) ?Msg {
    if (!keyboard.modifiers.hasNavigationModifier()) return null;
    if (std.ascii.eqlIgnoreCase(keyboard.key, "n")) return .show_invite;
    if (std.ascii.eqlIgnoreCase(keyboard.key, "j")) return .show_join;
    if (std.ascii.eqlIgnoreCase(keyboard.key, "r")) return .refresh;
    if (std.ascii.eqlIgnoreCase(keyboard.key, "b")) return .show_backup;
    if (std.ascii.eqlIgnoreCase(keyboard.key, ",") or std.ascii.eqlIgnoreCase(keyboard.key, "comma")) return .show_settings;
    return null;
}

pub fn onAppearance(appearance: native_sdk.Appearance) ?Msg {
    return .{ .appearance = appearance };
}

/// The first path dropped onto the window is offered as the body face; the
/// settings sheet then says what became of it.
pub fn onDrop(drop: native_sdk.platform.FileDropEvent) ?Msg {
    return if (drop.paths.len == 0) null else .{ .dropped = drop.paths[0] };
}

/// Judge the face at the path typed or dropped, by streaming its bytes into a
/// buffer the size of what the renderer will hold. The verdict is the same
/// function the start-up search runs, so what passes here passes there.
fn tryFace(m: *Model, fx: *Effects) void {
    const path = m.face.begin(font.max_face_bytes) orelse return;
    fx.readFileStream(.{ .key = verbs.key(.face_stream), .path = path, .on_result = Effects.fileMsg(.streamed) });
}

fn streamed(m: *Model, fx: *Effects, result: native_sdk.EffectFileResult) void {
    switch (result.event) {
        .chunk => if (!m.face.take(result.bytes)) {
            fx.cancel(verbs.key(.face_stream));
            m.face.verdict.set(m.t.face_too_large);
            m.face.finish();
        },
        .done => {
            defer m.face.finish();
            if (font.verdict(m.face.bytes())) |why| {
                m.face.verdict.set(if (std.mem.eql(u8, why, font.no_han)) m.t.face_no_han else why);
                return;
            }
            const where = std.fmt.bufPrint(&m.scratch, "{s}{c}{s}", .{ m.home.slice(), std.fs.path.sep, font.preference_file }) catch return;
            fx.writeFile(.{ .key = verbs.key(.face_file), .path = where, .bytes = m.face.path.text(), .on_result = Effects.fileMsg(.preferred) });
        },
        .terminal => {
            if (result.outcome != .cancelled) m.face.verdict.set(m.t.face_unreadable);
            m.face.finish();
        },
    }
}

fn open(m: *Model, fx: *Effects, slot: usize) void {
    if (slot >= m.channel_count) return;
    m.selected = slot;
    m.screen = .thread;
    // A sheet that is open stays open: the invitation minted on the welcome
    // page must still be on screen when the channel list answers and the
    // window walks over to read it (D3).
    m.mine.clear();
    m.theirs.clear();
    fetch(m, fx);
    stopTimers(fx);
    const row = m.current();
    if (row.slotted()) {
        fx.startTimer(.{ .key = verbs.key(.slot_timer), .interval_ms = @as(u64, row.period) * 1000, .mode = .repeating, .on_fire = Effects.timerMsg(.slot) });
    } else {
        fx.startTimer(.{ .key = verbs.key(.poll_timer), .interval_ms = poll_ms, .mode = .repeating, .on_fire = Effects.timerMsg(.poll) });
    }
}

fn fetch(m: *Model, fx: *Effects) void {
    // Nobody has joined yet, as far as this window knows. The only way to
    // learn otherwise is to read: the first read after they join is what
    // meets them (the CLI greets), so it is asked anyway, and `no_peer_yet`
    // is the one complaint the status line keeps quiet about — the banner
    // already says it. There is nothing of ours to read back until then.
    verbs.read(fx, m, .read_theirs, m.currentName(), m.theirs.height, &m.name_scratch);
    if (!m.current().hasPeer()) return;
    verbs.read(fx, m, .read_mine, m.currentName(), m.mine.height, &m.name_scratch);
}

pub fn stopTimers(fx: *Effects) void {
    fx.cancelTimer(verbs.key(.poll_timer));
    fx.cancelTimer(verbs.key(.slot_timer));
}

fn showInvite(m: *Model) void {
    m.sheet = .invite;
    m.invite.line.clear();
    m.check.clear();
    if (m.invite.waypoint.text().len == 0 and m.onThread()) m.invite.waypoint.set(m.currentWaypoint());
}

fn showDoctor(m: *Model) void {
    m.sheet = .doctor;
    m.doctor.clear();
    if (m.doctor.waypoint.text().len == 0 and m.onThread()) m.doctor.waypoint.set(m.currentWaypoint());
}

fn sendDraft(m: *Model, fx: *Effects) void {
    if (!m.canSend()) return;
    m.busy = verbs.key(.send);
    verbs.send(fx, m, m.currentName(), m.draft.text(), &m.scratch);
}

fn mint(m: *Model, fx: *Effects) void {
    if (m.isBusy() or !m.invite.ready()) return;
    m.busy = verbs.key(.invite);
    verbs.invite(fx, m, m.invite.nameText(), m.invite.waypointText(), .{ .every = m.invite.period(), .release = m.invite.release }, &m.scratch);
}

fn accept(m: *Model, fx: *Effects) void {
    if (m.isBusy() or !m.join.ready()) return;
    m.busy = verbs.key(.join);
    verbs.join(fx, m, m.join.nameText(), m.join.invitationText(), m.join.release, &m.scratch);
}

fn exportSite(m: *Model, fx: *Effects) void {
    if (m.isBusy()) return;
    m.busy = verbs.key(.export_);
    verbs.exportSite(fx, m);
}

fn saveRoster(m: *Model, fx: *Effects) void {
    if (m.isBusy() or !m.roster.ready()) return;
    var names: [model_mod.max_channels][]const u8 = undefined;
    const members = m.roster.ticked(m.channel_count, &names);
    m.sheet = .none;
    m.busy = verbs.key(.group);
    verbs.group(fx, m, m.roster.nameText(), members, &m.scratch);
}

fn broadcast(m: *Model, fx: *Effects) void {
    if (m.isBusy() or !m.onGroup() or m.draft.text().len == 0) return;
    m.busy = verbs.key(.fanout);
    verbs.fanout(fx, m, m.groupTitle(), m.draft.text(), &m.scratch);
}

fn examine(m: *Model, fx: *Effects) void {
    if (m.isBusy() or !m.doctor.ready()) return;
    m.busy = verbs.key(.doctor);
    verbs.doctor(fx, m, m.doctor.waypointText());
}

fn copy(m: *Model, fx: *Effects, text: []const u8) void {
    if (text.len == 0) return;
    fx.writeClipboard(.{ .key = verbs.key(.clipboard), .text = text });
    m.copied.set(text);
    fx.startTimer(.{ .key = verbs.key(.scrub_timer), .interval_ms = scrub_ms, .mode = .one_shot, .on_fire = Effects.timerMsg(.scrub) });
}

fn filed(m: *Model, result: native_sdk.EffectFileResult) void {
    m.backup.written = result.outcome == .ok;
    if (m.backup.written) return;
    m.status.code.set("glass.backup_unwritten");
    m.status.error_text.set(m.t.err_backup_unwritten);
    m.status.recover.set(m.t.rec_backup_unwritten);
}

fn exited(m: *Model, fx: *Effects, exit: native_sdk.EffectExit) void {
    if (m.busy == exit.key) m.busy = 0;
    answer.apply(m, exit);
    const key = verbs.keyOf(exit.key) orelse return;
    const failed = exit.reason != .exited or exit.code != 0;
    if (polling.exited(m, fx, key, failed)) return;
    if (failed) return;
    switch (key) {
        .channels => if (m.screen == .welcome and m.channel_count > 0) open(m, fx, 0),
        // A read that succeeded on a row still marked as waiting has just met
        // the peer; the row learns that from the channel list.
        .read_theirs => if (m.onThread() and !m.current().hasPeer()) verbs.channels(fx, m),
        .invite, .join, .forget, .group, .revoke => verbs.channels(fx, m),
        .fanout => polling.round(m, fx),
        .tick => verbs.read(fx, m, .read_theirs, m.currentName(), m.theirs.height, &m.name_scratch),
        .export_ => writeArchive(m, fx),
        else => {},
    }
}

fn writeArchive(m: *Model, fx: *Effects) void {
    const path = std.fmt.bufPrint(&m.backup.path.buf, "{s}{c}kusanagi-backup-{d}.ksnb", .{ m.home.slice(), std.fs.path.sep, fx.wallMs() }) catch return;
    m.backup.path.len = path.len;
    fx.writeFile(.{ .key = verbs.key(.backup_file), .path = path, .bytes = m.backup.bytes(), .on_result = Effects.fileMsg(.filed) });
}
