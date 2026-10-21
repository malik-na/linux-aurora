// SPDX-License-Identifier: GPL-2.0-only OR MIT

//! The register, engine, channel and VM state recovery reads, as [`super::logic`]'s types, and
//! failing every VM.
//!
//! The other operations recovery, PB growth and the idle policy use are
//! `Firmware::{transport_error, drain_rings, devctl}` (channel.rs), `write_c1` (initdata.rs),
//! `Firmware::{progress, retire_oldest, report}` (poll.rs) and `job_partly_published`
//! (engine.rs).

use kernel::prelude::*;

use super::logic::{EngineState, Observed, Receipts, Tail};
use crate::g15::channel::Channel;
use crate::g15::fw::graph::qstate;
use crate::g15::Firmware;

impl Firmware {
    // --- MMIO ---

    /// Engine busy and fault status (`Regs::engine_state`): busy = bit 0 of C020 | C120, fault
    /// = D8C0 of fault banks 0 and 1 with the bank selector D800 saved and restored. Every
    /// nonzero bank is logged with its decoded status (`log_fault`).
    pub(crate) fn engine_state(&mut self) -> Result<EngineState> {
        let state = self.session.regs.engine_state()?;
        if state.fault() != 0 {
            self.log_fault(&state);
        }
        Ok(EngineState {
            busy: state.busy & 1 != 0,
            fault: state.fault(),
        })
    }

    // --- Engine ---

    /// Receipts of the published prefix `index` (0 = oldest): the `engine::Flight` fields
    /// filled at publication.
    pub(crate) fn receipts(&self, index: usize) -> Receipts {
        let Some(f) = self.engine.flights.get(index) else {
            return Receipts::default();
        };
        Receipts {
            computes: f.computes,
            renders: f.renders,
            target: f.target,
            head: f.head,
            c_start: f.c_start,
            ta_target: f.ta_target,
            fs_target: f.fs_target,
            ta_head: f.ta_head,
            fs_head: f.fs_head,
            ta_start: f.ta_start,
            fs_start: f.fs_start,
            rec: f.rec,
            cstamp: f.cstamp,
            cwait: f.cwait,
        }
    }

    /// The render queue recovery observes: that of the published prefixes (one VM's, so one
    /// queue's), else that of the newest render prefix.
    pub(crate) fn active_rq(&self) -> usize {
        self.engine
            .flights
            .first()
            .map_or(self.engine.last_rq, |f| f.rq)
    }

    /// The newest published receipts: the `engine::Engine` fields of the compute queue and the
    /// active render queue ([`Self::active_rq`]) and the channel producers.
    pub(crate) fn tail(&self) -> Tail {
        let e = &self.engine;
        let t = &e.rq[self.active_rq()];
        Tail {
            target: e.target,
            head: e.head,
            ta_target: t.ta_target,
            fs_target: t.fs_target,
            ta_head: t.ta_head,
            fs_head: t.fs_head,
            producer: [
                self.channels.producer(Channel::Ta),
                self.channels.producer(Channel::Fs),
                self.channels.producer(Channel::Compute),
            ],
        }
    }

    /// One observation of the firmware-written receipt words: the compute/TA/FS FW stamps, the
    /// compute/TA/FS work-ring DONE and READ cursors (TA/FS of the active render queue) and the
    /// three submit channels' consumers.
    pub(crate) fn observe(&self) -> Result<Observed> {
        let (g, ch) = (&self.graph.compute, &self.channels);
        let r = &self.graph.render[self.active_rq()];
        Ok(Observed {
            compute_stamp: g.fw_stamp.read()?,
            ta_stamp: r.fw_stamp[0].read()?,
            fs_stamp: r.fw_stamp[1].read()?,
            compute_done: g.qstate.r32(qstate::DONE)?,
            compute_read: g.qstate.r32(qstate::READ)?,
            ta_done: r.qstate[0].r32(qstate::DONE)?,
            ta_read: r.qstate[0].r32(qstate::READ)?,
            fs_done: r.qstate[1].r32(qstate::DONE)?,
            fs_read: r.qstate[1].r32(qstate::READ)?,
            consumer: [
                ch.read(Channel::Ta)?,
                ch.read(Channel::Fs)?,
                ch.read(Channel::Compute)?,
            ],
        })
    }

    // --- VMs ---

    /// Latches `error` on every live VM (`Registry::fail_all`).
    pub(crate) fn fail_all_vms(&mut self, error: Error) {
        self.vms.fail_all(error)
    }
}
