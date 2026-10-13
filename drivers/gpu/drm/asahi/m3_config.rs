// SPDX-License-Identifier: GPL-2.0-only OR MIT
//! Owned RTKit 2419.140.12 configuration with named initialization references.
use kernel::prelude::*;
use crate::m3_init_layout as init;
use crate::m3_init_storage as storage;
use storage::{REGION_A,RUNTIME_POINTERS,HARDWARE_DATA,UNKNOWN_PAIR,FWLOG_PAYLOAD,
    UNKNOWN_C0,UNKNOWN_C1,UNKNOWN_C3,GLOBALS,GLOBALS_POWER,CONTROL_REGION,RUNTIME_FLAGS,
    FW_CONTROL_STATE,FW_CONTROL_RING};
use crate::{driver,agx_memory::{self},m3_memory::Buffer,mmu,pgtable::prot};
use kernel::time::{Instant, Monotonic, Delta, delay::fsleep};
kernel::static_assert!(core::mem::size_of::<crate::fw::channels::RawFwLogMsg>() == init::fwlog::ENTRY_SIZE);
kernel::static_assert!(core::mem::size_of::<crate::fw::channels::RawFwLogPayloadMsg>() == init::fwlog::PAYLOAD_SIZE);
kernel::static_assert!(<crate::fw::channels::FwLogChannelState as crate::fw::channels::RxChannelState>::SUB_CHANNELS == init::fwlog::CHANNELS);

/// The root, runtime-pointer and firmware-control records, encoded for the owners
/// `region` describes. Config uploads them for its own allocations; the device-tree
/// InitData checks encode them for the fixed allocation table before GPU access.
pub(crate) struct Records {
    pub(crate) root: [u8; init::ROOT_SIZE],
    pub(crate) runtime: [u8; init::RUNTIME_SIZE],
    pub(crate) control: KVec<u8>,
}
impl Records {
    /// `hardware_view_4` is the fourth hardware view's offset into the hardware record
    /// (`init::HARDWARE_VIEW_4` unless an armed T6031 uses the J516C record).
    pub(crate) fn encode(region: impl Fn(usize) -> Result<init::Region>, hardware_view_4: u64) -> Result<Self> {
        let root = init::Root { region_a: region(REGION_A)?, runtime: region(RUNTIME_POINTERS)?,
            globals: region(GLOBALS)?, new_a: region(RUNTIME_FLAGS)?,
            control: region(CONTROL_REGION)?, power: region(GLOBALS_POWER)? };
        let mut channels = [init::Channel { state: region(0)?, ring: region(1)? }; 17];
        for (index, channel) in channels.iter_mut().enumerate() {
            *channel = init::Channel { state: region(index * 2)?, ring: region(index * 2 + 1)? };
        }
        let runtime = init::RuntimePointers { hardware: region(HARDWARE_DATA)?,
            unknown_pair: region(UNKNOWN_PAIR)?, fwlog_payload: region(FWLOG_PAYLOAD)?,
            unknown_c0: region(UNKNOWN_C0)?, unknown_c1: region(UNKNOWN_C1)?,
            unknown_c3: region(UNKNOWN_C3)?, channels, hardware_view_4 };
        let control = init::ControlRegion { channel: init::Channel {
            state: region(FW_CONTROL_STATE)?, ring: region(FW_CONTROL_RING)? } };
        let mut root_bytes = [0; init::ROOT_SIZE];
        root.encode(&mut root_bytes).map_err(|_| EINVAL)?;
        let mut runtime_bytes = [0; init::RUNTIME_SIZE];
        runtime.encode(&mut runtime_bytes).map_err(|_| EINVAL)?;
        // The control/status region is 50 KiB; use temporary heap storage,
        // never a kernel-stack array. All three owners precede firmware startup.
        let mut control_bytes = KVec::new();
        control_bytes.resize(init::CONTROL_SIZE, 0u8, GFP_KERNEL)?;
        control.encode(&mut control_bytes).map_err(|_| EINVAL)?;
        Ok(Self { root: root_bytes, runtime: runtime_bytes, control: control_bytes })
    }
}

pub(crate) struct Config {
    objects: KVec<Buffer>,
    #[cfg(CONFIG_DEV_COREDUMP)]
    fault_reserve: Option<crate::agx_fault::Dump>,
    _iomaps: KVec<mmu::KernelMapping>,
    pub(crate) completed_events: u64,
    /// Recovery events answered by a resume (`asahi.t6031_resume_halt=1`).
    pub(crate) resumed_halts: u64,
    pstates: crate::m3_adt_config::PstateWatch,
    thermal: crate::m3_thermal::Governor,
}
impl Config {
    /// Snapshot owned RAM only. Firmware can still update these buffers;
    /// per-record time intervals explicitly describe sequential observations.
    /// `crashlog` is the firmware crash log, when RTKit reported one first (armed T6031 start).
    #[cfg(CONFIG_DEV_COREDUMP)]
    pub(crate) fn fault_snapshot(&mut self, dev: &driver::AsahiDevice, primary: Error, gpu_pending: bool,
        crashlog: Option<&[u8]>) -> Result {
        let mut dump = self.fault_reserve.take().ok_or(ENOMEM)?;
        dump.begin(0);
        Self::record_cause(&mut dump, 1, primary.to_errno(), gpu_pending)?;
        if let Some(crashlog) = crashlog {
            Self::record_crashlog(dev, &mut dump, crashlog);
        }
        self.initdata_records(dev, &mut dump);
        dump.publish(dev.as_ref())
    }

    /// The firmware crashed and no host step failed: one snapshot of the crash log and the
    /// InitData records, for comparing the records offline. The cause record has kind 2
    /// (firmware crash), errno 0.
    ///
    /// The devcoredump is the `agx_fault::Dump` format ("M3FWD001"): a 40-byte header (magic[8],
    /// u32 record count, u32 version 1, u64 job 0, u64 begin ns, u64 total length), then the
    /// records back to back, each name[32] (NUL padded), u64 firmware VA, u64 length, u64 begin
    /// ns, u64 end ns, then `length` bytes. All integers are little-endian. Records, in order:
    /// host-first-error, firmware-crashlog, then those of `initdata_records`.
    #[cfg(CONFIG_DEV_COREDUMP)]
    pub(crate) fn crash_snapshot(&mut self, dev: &driver::AsahiDevice, gpu_pending: bool,
        crashlog: &[u8]) -> Result {
        let mut dump = self.fault_reserve.take().ok_or(ENOMEM)?;
        dump.begin(0);
        Self::record_cause(&mut dump, 2, 0, gpu_pending)?;
        Self::record_crashlog(dev, &mut dump, crashlog);
        self.initdata_records(dev, &mut dump);
        dump.publish(dev.as_ref())
    }

    /// "host-first-error": u32 kind (1 host fault, 2 firmware crash), i32 errno, u64 whether
    /// a GPU job was pending.
    #[cfg(CONFIG_DEV_COREDUMP)]
    fn record_cause(dump: &mut crate::agx_fault::Dump, kind: u32, errno: i32, gpu_pending: bool) -> Result {
        let mut cause = [0u8; 16];
        cause[..4].copy_from_slice(&kind.to_le_bytes());
        cause[4..8].copy_from_slice(&errno.to_le_bytes());
        cause[8..16].copy_from_slice(&u64::from(gpu_pending).to_le_bytes());
        dump.record("host-first-error", 0, cause.len(), |out| {
            out.copy_from_slice(&cause); Ok(())
        })
    }

    /// "firmware-crashlog": the log bytes RTKit retained, as many as `capture_crashlog` kept.
    /// A record that does not fit is logged and skipped, never fatal to the rest.
    #[cfg(CONFIG_DEV_COREDUMP)]
    fn record_crashlog(dev: &driver::AsahiDevice, dump: &mut crate::agx_fault::Dump, crashlog: &[u8]) {
        if crashlog.is_empty() { return; }
        if let Err(error) = dump.record("firmware-crashlog", 0, crashlog.len(), |out| {
            out.copy_from_slice(crashlog); Ok(())
        }) {
            dev_warn!(dev.as_ref(), "M3: crash log record skipped: {:?}\n", error);
        }
    }

    /// Record the host-written InitData records and the channel storage, in this order: root,
    /// runtime pointers, hardware record (its live size, which may exceed the allocation
    /// table's), globals, control region, flags, power block, then the device-control,
    /// firmware-control, event, firmware-log and trace queues. Reads are of the live buffers,
    /// so after the firmware ran they show what it wrote too. A record that cannot be read or
    /// does not fit is logged and skipped.
    #[cfg(CONFIG_DEV_COREDUMP)]
    fn initdata_records(&mut self, dev: &driver::AsahiDevice, dump: &mut crate::agx_fault::Dump) {
        for (name, index) in [
            ("root", storage::ROOT), ("runtime", RUNTIME_POINTERS),
            ("hardware", HARDWARE_DATA), ("globals", GLOBALS),
            ("control", CONTROL_REGION), ("flags", RUNTIME_FLAGS),
            // The power-controller block, and the host-to-firmware device-control and
            // firmware-control queues: their state words show whether the firmware read a
            // queued control (read index) and what it was sent.
            ("power", GLOBALS_POWER),
            ("devctl-state", storage::DEVICE_CONTROL * 2),
            ("devctl-ring", storage::DEVICE_CONTROL * 2 + 1),
            ("fwctl-state", storage::FW_CONTROL_STATE),
            ("fwctl-ring", storage::FW_CONTROL_RING),
            ("event-state", storage::EVENT * 2),
            ("event-ring", storage::EVENT * 2 + 1),
            ("fwlog-state", storage::FIRMWARE_LOG * 2),
            ("fwlog-ring", storage::FIRMWARE_LOG * 2 + 1),
            ("fwlog-payload", FWLOG_PAYLOAD),
            ("trace-state", storage::TRACE * 2),
            ("trace-ring", storage::TRACE * 2 + 1),
        ] {
            let buffer = &mut self.objects[index];
            let (va, size) = (buffer.va(), buffer.size());
            if let Err(error) = dump.record(name, va, size, |out| buffer.read(0, out)) {
                dev_warn!(dev.as_ref(), "M3: snapshot record {} skipped: {:?}\n", name, error);
            }
        }
    }

    pub(crate) fn new(dev: &driver::AsahiDevice, uat: &mmu::Uat, firmware: &crate::m3_firmware::Firmware,
        contents: &crate::m3_adt_config::Contents) -> Result<Self> {
        contents.check_for_upload()?;
        let mut objects = KVec::new();
        let mut iomaps = KVec::new();
        for index in 0..storage::COUNT {
            let mut a=storage::allocation(index).map_err(|_|EINVAL)?;
            // An armed T6031 may give the firmware the J516C's longer hardware record.
            if index==HARDWARE_DATA {a.size=contents.hardware_size;}
            let image=contents.initial(index,&a)?;
            if a.initial!=storage::Initial::Zero && image.len()!=a.size {return Err(EINVAL);}
            // The SoC may place the HwData object elsewhere and have it mapped cacheable
            // (`Soc::hwdata_object`); the records and the upload check use the live address.
            let object=if index==HARDWARE_DATA {contents.hwdata_object} else {None};
            if let Some(o)=object {a.address=o.address;}
            let mut b=match object {
                Some(o) if o.cached=>Buffer::at_prot(dev,uat.kernel_vm(),None,Some(a.address),a.size,
                    prot::PROT_FW_PRIV_RW,prot::PROT_GPU_SHARED_RW)?,
                _=>Buffer::at(dev,uat.kernel_vm(),None,Some(a.address),a.size,a.gpu_shared)?,
            };
            b.write(0,image)?;objects.push(b,GFP_KERNEL)?;
        }
        for io in contents.iomaps {
            // Read-write unless the contents say otherwise (only the T8122 start experiment).
            let protection=if contents.read_only_slots==0 {prot::PROT_FW_MMIO_RW}
                else {crate::t8122_start::mmio_prot(contents.read_only_slots,io.slot)};
            let map=uat.kernel_vm().map_io(io.address,io.physical.try_into()?,io.size,protection)?;
            let owned=init::Region::new(map.iova(),io.size).map_err(|_|EINVAL)?;
            let field=io.pointer_field().map_err(|_|EINVAL)?;
            objects[HARDWARE_DATA].u64(field,io.pointer(owned).map_err(|_|EINVAL)?)?;
            iomaps.push(map,GFP_KERNEL)?;
        }
        // This field is a physical address owned by the reserved GPU region,
        // unlike the firmware virtual references in the IOMapping records.
        objects[HARDWARE_DATA].u64(storage::GPU_REGION_PHYSICAL,firmware.resources.regions[0].base)?;
        Self::initialize_records(&mut objects,contents.hardware_view_4)?;
        // The root record carries the G15 14.8.3 InitData version, which is every T6030
        // image's. Only the T8122 start experiment gives the firmware another one
        // (`asahi.t8122_initdata_version`).
        if firmware.initdata_magic!=crate::m3_firmware::G15_V14_8_3_INITDATA {
            objects[storage::ROOT].u64(0,firmware.initdata_magic)?;
        }
        // Vertex/tessellation loops need not retire a primitive at every
        // firmware progress poll. The inherited interval of 10 falsely
        // declares a valid 35.7 ms TA job stuck on J514S. Keep progress
        // detection enabled, with a finite interval of 100 firmware polls.
        // This is independent of the host job bound and 60-second watchdog.
        // RTKit 2419 copies Globals+0x980 into its TA poll threshold at
        // text+0x6514; text+0xd710 checks it before sampling engine progress.
        const TA_PROGRESS_INTERVAL: usize = 0x980;
        if objects[GLOBALS].read_u32(TA_PROGRESS_INTERVAL)? != 10 { return Err(EINVAL); }
        objects[GLOBALS].u32(TA_PROGRESS_INTERVAL, 100)?;
        dev_info!(dev.as_ref(), "M3: firmware TA progress-check interval=100\n");
        // RTKit 2419 +2de48..2dec8 accepts user timestamp stores only
        // within the 64 MiB arena at HwDataB+28. Match map_timestamp().
        objects[HARDWARE_DATA].u64(0x28,agx_memory::TIMESTAMP_RANGE.start)?;
        // RTKit 2419 HwDataA follows the 0x4580-byte HwDataB. Request the
        // boot performance state through firmware (asahi.m3_boot_pstate) and cap
        // the states it may use (asahi.m3_max_pstate), retaining the voltage
        // tables, power/thermal ceilings and minimum operating state.
        // The bring-up image initialized actual/target and every base to 1
        // (338 MHz), so short desktop jobs never left that startup state.
        // The uploaded table is the SoC's (`Soc::pstates`): its highest state at +0xb58 and that
        // state's frequency in the table at +0xb5c (8 and 1380 MHz on T6030).
        let (states, top_mhz) = contents.table;
        if objects[HARDWARE_DATA].read_u32(0xb58)? != states
            || objects[HARDWARE_DATA].read_u32(0xb5c + states as usize * 4)? != top_mhz
        {
            return Err(EINVAL);
        }
        let (hwdata, globals) = objects.split_at_mut(GLOBALS);
        crate::m3_adt_config::publish_pstates(dev.as_ref(), &mut hwdata[HARDWARE_DATA], &mut globals[0], &contents.pstates)?;
        // The J516C word overlay, last, so the firmware sees the J516C driver's words where it
        // covers the generator's (asahi.t6031_overlay=j516c).
        if contents.overlay {
            crate::m3_adt_config::apply_overlay(dev.as_ref(),&mut objects)?;
            // The overlay carries the J516C driver's uncapped runtime-cap words; put the armed cap
            // back (boots t42-t58 ran at the firmware's highest state because of this).
            crate::m3_adt_config::publish_runtime_cap(&mut objects[GLOBALS], contents.pstates.safe)?;
            dev_info!(dev.as_ref(), "M3 G15C start: runtime cap republished after the overlay: state {}\n", contents.pstates.safe);
        }
        // An armed T6031 may stretch the firmware's compute progress check (asahi.t6031_cl_progress_ms):
        // boot t56 showed the halted first job completing 2.5 ms after a resume.
        if let Some(ms) = crate::m3_params::t6031_cl_progress_ms() {
            dev_info!(dev.as_ref(), "M3 G15C start: compute progress check interval {} ms (asahi.t6031_cl_progress_ms)\n", ms);
            objects[GLOBALS].u32(0x984, ms)?;
        }
        // Firmware ktrace is for explicit detailed diagnostics. Normal games
        // still drain error/events, but need not generate trace records for
        // every firmware operation on the command-processing core.
        objects[RUNTIME_FLAGS].u32(0,u32::from(crate::debug::debug_enabled(
            crate::debug::DebugFlags::SubmitTiming)))?;
        crate::m3_adt_config::check_upload(dev.as_ref(), firmware, contents.iomaps, &mut objects)?;
        agx_memory::publish();
        let thermal = crate::m3_thermal::Governor::new(dev.as_ref(), &contents.pstates)?;
        #[cfg(CONFIG_DEV_COREDUMP)]
        let fault_reserve = match crate::agx_fault::Dump::new_m3() {
            Ok(dump) => Some(dump),
            Err(error) => {
                dev_warn!(dev.as_ref(), "M3: fault snapshot reserve unavailable: {:?}\n", error);
                None
            }
        };
        Ok(Self { objects, _iomaps: iomaps, completed_events: 0, resumed_halts: 0,
            #[cfg(CONFIG_DEV_COREDUMP)] fault_reserve,
            pstates: crate::m3_adt_config::PstateWatch::new(contents.pstates), thermal })
    }
    fn initialize_records(objects: &mut [Buffer], hardware_view_4: u64) -> Result {
        let records = Records::encode(|index: usize| {
            init::Region::new(objects[index].va(), objects[index].size()).map_err(|_| EINVAL)
        }, hardware_view_4)?;
        objects[storage::ROOT].write(0, &records.root)?;
        objects[RUNTIME_POINTERS].write(0, &records.runtime)?;
        objects[CONTROL_REGION].write(0, &records.control)
    }
    pub(crate) fn root(&self) -> u64 { self.objects[storage::ROOT].va() }
    /// Move the runtime cap by temperature (the thermal limit), then check the performance
    /// state the firmware reports against the ceiling and the runtime cap. Called after boot and
    /// after every job, by the job thread.
    pub(crate) fn check_pstate(&mut self, dev: &driver::AsahiDevice, device: &crate::m3_device::Device, what: &str) -> Result {
        let now = kernel::time::Instant::<kernel::time::Monotonic>::now();
        if let Some(cap) = self.thermal.update(dev.as_ref(), now) {
            crate::m3_adt_config::publish_runtime_cap(&mut self.objects[GLOBALS], cap)?;
        }
        let allowed = self.thermal.allowed(now);
        let (over, peak) = self.pstates.check(dev.as_ref(), &mut self.objects[HARDWARE_DATA], || device.pstate_register(), what, allowed)?;
        self.thermal.note_check(dev.as_ref(), now, over, peak)?;
        self.thermal.busy(now);
        Ok(())
    }
    pub(crate) fn stats_region(&self) -> Result<init::Region> {
        let owner = &self.objects[HARDWARE_DATA];
        init::Region::new(owner.va(), owner.size()).map_err(|_| EINVAL)
    }

    pub(crate) fn render_pb(&mut self) -> Result {
        self.objects[UNKNOWN_C1].write(0,&storage::parameter_buffer(crate::m3_render::PB_PAGES).map_err(|_|EINVAL)?)
    }
    /// `timestamp`: the first eight bytes of the entry; the qualified graph sends 0, the J516C's
    /// own driver the 24 MHz system counter at submission (`asahi.t6031_run_timestamp`).
    pub(crate) fn submit_queue(&mut self, kind:usize, queue:u64, head:u16,event:u8,new:bool,timestamp:u64)->Result {
        if kind>2 {return Err(EINVAL);}
        let state=kind*2;let ring=state+1;
        let w=self.objects[state].read_u32(0x20)?;
        if w>=256 || self.objects[state].read_u32(0)?!=w { return Err(EBUSY); }
        let mut message=[0u8;0x18];
        message[0..8].copy_from_slice(&timestamp.to_le_bytes());
        message[8..16].copy_from_slice(&queue.to_le_bytes());
        message[16..20].copy_from_slice(&(kind as u32).to_le_bytes());
        message[20..22].copy_from_slice(&head.to_le_bytes());
        message[22]=event;message[23]=u8::from(new); // New queue; caller selected the event slot.
        self.objects[ring].write(w as usize*0x18,&message)?;
        agx_memory::publish();
        self.objects[state].u32(0x20,(w+1)%256)?;
        agx_memory::publish();
        Ok(())
    }
    /// Whether the firmware has consumed every message on pipe `kind` (0 TA, 1 3D, 2 compute).
    pub(crate) fn pipe_free(&mut self, kind:usize) -> Result<bool> {
        if kind>2 {return Err(EINVAL);}
        Ok(self.objects[kind*2].read_u32(0)? == self.objects[kind*2].read_u32(0x20)?)
    }
    pub(crate) fn pipes_idle(&mut self) -> Result<bool> {
        for state in [0,2,4] {
            if self.objects[state].read_u32(0)? != self.objects[state].read_u32(0x20)? {
                return Ok(false);
            }
        }
        Ok(true)
    }
    pub(crate) fn ready(&mut self) -> Result<bool> {
        // NewRegionB+4 is an input initialized to 1, not an acknowledgement.
        // These are firmware-written transitions present in compute-15's
        // pre-submit snapshot; both start at zero in the exported schema.
        Ok(self.objects[RUNTIME_POINTERS].read_u32(0x314)? == 0xabcdabcd
            && self.objects[RUNTIME_FLAGS].read_u32(4)? == 1)
    }
    pub(crate) fn log_ready(&mut self, dev: &driver::AsahiDevice) -> Result {
        let magic=self.objects[RUNTIME_POINTERS].read_u32(0x314)?;
        let state=self.objects[RUNTIME_POINTERS].read_u32(0x2fc)?;
        let a=self.objects[RUNTIME_FLAGS].read_u32(4)?;
        let b=self.objects[RUNTIME_FLAGS].read_u32(0x14)?;
        let c=self.objects[CONTROL_REGION].read_u32(0x45c4)?;
        dev_info!(dev.as_ref(),"M3 firmware readiness: magic={:#x} runtime={} newA={}/{} newB={}\n",magic,state,a,b,c);
        Ok(())
    }
    /// The firmware's halt counter and its halted and resume flags.
    pub(crate) fn halt_state(&mut self) -> Result<(u64,u32,u32)> {
        let control=&mut self.objects[CONTROL_REGION];
        Ok((control.read_u64(0x4580)?,control.read_u32(0x4590)?,control.read_u32(0x45a0)?))
    }
    /// Let a halted firmware continue: clear halted, then set resume. These
    /// words keep the halt/resume layout of earlier Apple GPU firmware.
    pub(crate) fn resume_halted(&mut self) -> Result {
        let control=&mut self.objects[CONTROL_REGION];
        control.u32(0x4590,0)?;
        agx_memory::publish();
        control.u32(0x45a0,1)?;
        agx_memory::publish();
        Ok(())
    }
    /// Read-only snapshot of the firmware's recovery words in the control
    /// region (InitData+0xb0): the halt counter and the halted/resume flags
    /// at +0x4580/+0x4590/+0x45a0, the firmware's own recovery information
    /// at +0x40a0, and the details it leaves for a recovery event at +0x44a0.
    /// Nothing is acknowledged, cleared or resumed here.
    pub(crate) fn log_recovery_state(&mut self, dev: &driver::AsahiDevice, events: u64) -> Result {
        let control=&mut self.objects[CONTROL_REGION];
        let halt_count=control.read_u64(0x4580)?;
        let halted=control.read_u32(0x4590)?;
        let resume=control.read_u32(0x45a0)?;
        let info=[control.read_u32(0x40a0)?,control.read_u32(0x40a4)?];
        let mut packet=[0u8;0x14];
        control.read(0x44a0,&mut packet)?;
        dev_info!(dev.as_ref(),"M3 firmware recovery state: halt_count={} halted={} resume={} info={:#x}/{:#x} packet={:02x?} completed_events={} event_messages={}\n",
            halt_count,halted,resume,info[0],info[1],packet,self.completed_events,events);
        Ok(())
    }
    /// Queue a firmware-control ("MemDesc") entry asking the firmware to flush its view of `pages`
    /// pages at `addr` (14.x layout: u64 addr, u32 0, u32 slot 0x40, u16 pages, u16 2, u32 pad; the
    /// write index at state +0x10). The J516C's own driver sends one for each pool manager record
    /// before the record's first job. Returns the index the firmware's read index must reach.
    pub(crate) fn fwctl_flush(&mut self, addr: u64, pages: u16) -> Result<u32> {
        let count = (self.objects[FW_CONTROL_RING].size() / 0x18) as u32;
        let w = self.objects[FW_CONTROL_STATE].read_u32(0x10)?;
        let r = self.objects[FW_CONTROL_STATE].read_u32(0)?;
        if count == 0 || w >= count || r != w { return Err(EBUSY); }
        let mut bytes = [0u8; 0x18];
        bytes[..8].copy_from_slice(&addr.to_le_bytes());
        bytes[0xc..0x10].copy_from_slice(&0x40u32.to_le_bytes());
        bytes[0x10..0x12].copy_from_slice(&pages.to_le_bytes());
        bytes[0x12..0x14].copy_from_slice(&2u16.to_le_bytes());
        self.objects[FW_CONTROL_RING].write(w as usize * 0x18, &bytes)?;
        agx_memory::publish();
        let next = (w + 1) % count;
        self.objects[FW_CONTROL_STATE].u32(0x10, next)?;
        agx_memory::publish();
        Ok(next)
    }
    pub(crate) fn fwctl_done(&mut self, next: u32) -> Result<bool> {
        Ok(self.objects[FW_CONTROL_STATE].read_u32(0)? == next)
    }
    pub(crate) fn control(&mut self, opcode: u32) -> Result<u32> {
        if !matches!(opcode, 0x13 | 9 | 0x1c) { return Err(EINVAL); }
        let w = self.objects[storage::DEVICE_CONTROL*2].read_u32(0x20)?;
        let r = self.objects[storage::DEVICE_CONTROL*2].read_u32(0)?;
        if w >= 256 || r != w { return Err(EBUSY); }
        let mut bytes = [0; 0x38];
        bytes[..4].copy_from_slice(&opcode.to_le_bytes());
        self.objects[storage::DEVICE_CONTROL*2+1].write(w as usize * 0x38, &bytes)?;
        agx_memory::publish();
        let next = (w + 1) % 256;
        self.objects[storage::DEVICE_CONTROL*2].u32(0x20, next)?;
        agx_memory::publish();
        Ok(next)
    }
    pub(crate) fn control_done(&mut self, next: u32) -> Result<bool> {
        Ok(self.objects[storage::DEVICE_CONTROL*2].read_u32(0)? == next && self.objects[storage::DEVICE_CONTROL*2].read_u32(0x10)? == next)
    }
    pub(crate) fn drain(&mut self, dev: &driver::AsahiDevice) -> Result {
        // Event, trace, stats and six independent firmware log rings.
        for (state, ring, size, count, channels) in [(storage::EVENT*2,storage::EVENT*2+1,0x38,256,1),
            (storage::TRACE*2,storage::TRACE*2+1,0x38,512,1),
            (storage::STATS*2,storage::STATS*2+1,0x40,256,1),
            (storage::FIRMWARE_LOG*2,storage::FIRMWARE_LOG*2+1,init::fwlog::ENTRY_SIZE,init::fwlog::SLOTS,init::fwlog::CHANNELS)] {
            for channel in 0..channels {
                let base = channel * 0x30;
                let mut r = self.objects[state].read_u32(base)? as usize;
                let w = self.objects[state].read_u32(base+0x20)? as usize;
                if r >= count || w >= count { return Err(EIO); }
                let previous_read = r;
                while r != w {
                    if (state==storage::TRACE*2 && self.completed_events<4) || state==storage::FIRMWARE_LOG*2 {
                        let mut raw=[0u8;0xd8];
                        self.objects[ring].read((channel*count+r)*size,&mut raw[..size])?;
                        dev_info!(dev.as_ref(),"M3 fw-channel {} {:02x?}\n",state,&raw[..size]);
                        if state == storage::FIRMWARE_LOG*2 && self.objects[ring].read_u32((channel*count+r)*size)? == 2 {
                            let index = self.objects[ring].read_u64((channel*count+r)*size+8)?;
                            let offset = init::fwlog::payload_offset(channel,index).map_err(|_| EIO)?;
                            let mut payload = [0u8;init::fwlog::PAYLOAD_SIZE];
                            self.objects[FWLOG_PAYLOAD].read(offset,&mut payload)?;
                            dev_info!(dev.as_ref(),"M3 fwlog payload {} {:02x?}\n",channel,payload);
                        }
                    }
                    // With the recovery-resume knob on, the firmware's stats entries (power on/off,
                    // power state, utilization) are logged raw as well: the only per-cluster power
                    // signal the firmware gives the host.
                    if state == storage::STATS*2 && crate::m3_params::t6031_resume_halt() {
                        let mut raw=[0u8;0x40];self.objects[ring].read(r*size,&mut raw)?;
                        dev_info!(dev.as_ref(), "M3 firmware stats {} bytes={:02x?}\n", self.objects[ring].read_u32(r*size)?, &raw[4..]);
                    }
                    if state == storage::EVENT*2 {
                        let kind = self.objects[ring].read_u32(r*size)?;
                        let resume = crate::m3_params::t6031_resume_halt();
                        if resume {
                            let mut raw=[0u8;0x38];self.objects[ring].read(r*size,&mut raw)?;
                            dev_info!(dev.as_ref(), "M3 firmware event {} bytes={:02x?}\n", kind, raw);
                        }
                        if kind == 1 {
                            let flags=self.objects[ring].read_u64(r*size+4)?;
                            self.completed_events+=u64::from((flags&7).count_ones());
                        }
                        // An armed T6031 may answer a recovery event by resuming the halted firmware
                        // and letting the job retire on its own merits (asahi.t6031_resume_halt=1).
                        if kind == 4 && resume {
                            let start=Instant::<Monotonic>::now();
                            let mut halt=self.halt_state()?;
                            while halt.1==0 && start.elapsed()<Delta::from_millis(100) {
                                fsleep(Delta::from_millis(1));
                                halt=self.halt_state()?;
                            }
                            if halt.1!=0 {
                                self.resume_halted()?;
                                self.resumed_halts+=1;
                                dev_warn!(dev.as_ref(), "M3 firmware recovery event answered by a resume (halt_count={}, resumes={}); the job stays live\n", halt.0, self.resumed_halts);
                            } else {
                                dev_err!(dev.as_ref(), "M3 firmware recovery event but no halt within 100 ms (halt_count={} resume={})\n", halt.0, halt.2);
                                return Err(EIO);
                            }
                            r = (r+1) % count;
                            continue;
                        }
                        if kind == 0 || kind == 4 {
                            let mut raw=[0u8;0x38];self.objects[ring].read(r*size,&mut raw)?;
                            dev_err!(dev.as_ref(), "M3 firmware error event {} bytes={:02x?}\n", kind,raw);
                            return Err(EIO);
                        }
                    }
                    r = (r+1) % count;
                }
                // Publish actual consumption only, as RxChannel::get does.
                // Empty polls need no write to firmware's shared state line.
                if r != previous_read { self.objects[state].u32(base, r as u32)?; }
            }
        }
        Ok(())
    }
}
