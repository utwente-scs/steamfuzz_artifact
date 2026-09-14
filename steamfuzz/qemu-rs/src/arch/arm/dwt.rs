use std::os;

use common::config::emulator::ENABLE_POLL_LOOP_FAST_FORWARD;
use qemu_sys::{cstr, ARMv7MState};

use crate::{Event, QemuCallbackShared};

// DWT register offsets from 0xE0001000
const REG_OFF_DWT_CTRL: u64 = 0x0;
const REG_OFF_DWT_CYCCNT: u64 = 0x4;

// DWT_CTRL bit: enable cycle counter
const DWT_CTRL_CYCCNTENA: u32 = 1 << 0;

// Poll-loop sleep fast-forward:
//   If CYCCNT grows by <= this many ticks between two consecutive reads, the
//   firmware is probably spinning in a busy-wait delay loop.
const DWT_POLL_LOOP_THRESHOLD: u32 = 5;
//   After this many "tiny increment" reads in a row, start boosting CYCCNT.
const DWT_POLL_LOOP_LIMIT: u32 = 2;
//   Amount to add to CYCCNT on each stuck read (~1 ms at 168 MHz).
//   With this, a 100 ms delay exits in ~17 fast-forward reads instead of
//   ~16 800 000 loop iterations.
const DWT_FAST_FORWARD_CYCLES: u32 = 1_000_000;

static mut DWT: Option<Dwt> = None;

pub struct Dwt {
    region: qemu_sys::MemoryRegion,
    ops: qemu_sys::MemoryRegionOps,
    callback: QemuCallbackShared,
    ctrl: u32,
    cyccnt: u32,
    // Poll-loop detection (not firmware-visible, not saved in snapshots)
    last_read_cyccnt: u32,
    poll_loop_count: u32,
}

#[derive(Clone, Debug)]
pub(crate) struct DwtSnapshot {
    ctrl: u32,
    cyccnt: u32,
}

pub fn dwt() -> &'static mut Dwt {
    unsafe { DWT.as_mut().unwrap() }
}


extern "C" fn dwt_read(
    opaque: *mut os::raw::c_void,
    addr: qemu_sys::hwaddr,
    size: os::raw::c_uint,
) -> u64 {
    if size != 4 {
        log::warn!(
            "Invalid DWT read with size {} at offset {:#x?}",
            size,
            addr
        );
        return 0;
    }

    let value = dwt_ptr(opaque).read(addr);

    log::trace!(
        "dwt_read(addr = {:#x?}, size = {:#x?}) -> value = {:#x?}",
        addr,
        size,
        value
    );

    value as u64
}

fn dwt_ptr<'a>(opaque: *mut os::raw::c_void) -> &'a mut Dwt {
    unsafe { (opaque as *mut Dwt).as_mut() }.expect("DWT opaque pointer is null")
}

extern "C" fn dwt_write(
    opaque: *mut os::raw::c_void,
    addr: qemu_sys::hwaddr,
    data: u64,
    size: os::raw::c_uint,
) {
    if size != 4 {
        log::warn!(
            "Invalid DWT write with size {} at offset {:#x?} with value {:#x?}",
            size,
            addr,
            data
        );
    }

    log::trace!(
        "dwt_write(addr = {:#x?}, data = {:#x?}, size = {:#x?})",
        addr,
        data,
        size
    );

    dwt_ptr(opaque).write(addr, data as u32);
}

impl Dwt {
    fn new(
        region: qemu_sys::MemoryRegion,
        ops: qemu_sys::MemoryRegionOps,
        callback: QemuCallbackShared,
    ) -> Self {
        Dwt {
            region,
            ops,
            callback,
            ctrl: 0,
            cyccnt: 0,
            last_read_cyccnt: 0,
            poll_loop_count: 0,
        }
    }

    /// Increment CYCCNT by `count` basic-block ticks if CYCCNTENA is set.
    pub fn tick(&mut self, count: u32) {
        if self.ctrl & DWT_CTRL_CYCCNTENA != 0 {
            self.cyccnt = self.cyccnt.wrapping_add(count);
        }
    }

    fn read(&mut self, offset: u64) -> u32 {
        match offset {
            REG_OFF_DWT_CTRL => self.ctrl,
            REG_OFF_DWT_CYCCNT => {
                // Sync tick count before returning (same trick SysTick uses).
                self.callback
                    .borrow_mut()
                    .on_update(Event::DwtGetCyccnt)
                    .expect("DwtGetCyccnt update hook failed");

                if ENABLE_POLL_LOOP_FAST_FORWARD {
                    let delta = self.cyccnt.wrapping_sub(self.last_read_cyccnt);
                    if delta <= DWT_POLL_LOOP_THRESHOLD {
                        self.poll_loop_count += 1;
                        if self.poll_loop_count >= DWT_POLL_LOOP_LIMIT {
                            log::trace!(
                                "DWT poll-loop detected (count={}), fast-forwarding CYCCNT by {}",
                                self.poll_loop_count,
                                DWT_FAST_FORWARD_CYCLES,
                            );
                            self.cyccnt = self.cyccnt.wrapping_add(DWT_FAST_FORWARD_CYCLES);
                        }
                    } else {
                        // Real forward progress — reset the stuck counter.
                        self.poll_loop_count = 0;
                    }
                    self.last_read_cyccnt = self.cyccnt;
                }

                self.cyccnt
            }
            _ => {
                log::trace!("DWT read from unimplemented offset {:#x?}", offset);
                0
            }
        }
    }

    fn write(&mut self, offset: u64, value: u32) {
        match offset {
            REG_OFF_DWT_CTRL => {
                self.ctrl = value;
            }
            REG_OFF_DWT_CYCCNT => {
                self.cyccnt = value;
            }
            _ => {
                log::trace!(
                    "DWT write to unimplemented offset {:#x?} with value {:#x?}",
                    offset,
                    value
                );
            }
        }
    }

    pub(crate) fn snapshot_create(&self) -> DwtSnapshot {
        DwtSnapshot {
            ctrl: self.ctrl,
            cyccnt: self.cyccnt,
        }
    }

    pub(crate) fn snapshot_restore(&mut self, snapshot: &DwtSnapshot) {
        self.ctrl = snapshot.ctrl;
        self.cyccnt = snapshot.cyccnt;
        // Reset detection state on restore so a replayed input doesn't carry
        // over stale poll-loop counters from the previous run.
        self.last_read_cyccnt = snapshot.cyccnt;
        self.poll_loop_count = 0;
    }
}

pub(crate) fn init_dwt(
    dev: *mut qemu_sys::DeviceState,
    armv7m: &mut ARMv7MState,
    callback: QemuCallbackShared,
) {
    log::trace!("init_dwt()");

    let region = qemu_sys::MemoryRegion::default();
    let ops = qemu_sys::MemoryRegionOps {
        read: Some(dwt_read),
        write: Some(dwt_write),
        endianness: qemu_sys::device_endian::DEVICE_NATIVE_ENDIAN,
        valid: qemu_sys::MemoryRegionOps__bindgen_ty_1 {
            min_access_size: 4,
            max_access_size: 4,
            ..qemu_sys::MemoryRegionOps__bindgen_ty_1::default()
        },
        ..qemu_sys::MemoryRegionOps::default()
    };

    unsafe {
        DWT = Some(Dwt::new(region, ops, callback));
        let dwt_ptr = DWT.as_mut().unwrap();

        qemu_sys::memory_region_init_io(
            &mut dwt_ptr.region,
            dev as _,
            &dwt_ptr.ops,
            dwt_ptr as *mut _ as _,
            cstr!("dwt-hoedur"),
            0x1000,
        );
        // Use i32::MAX priority to override the nvic-default RAZ/WI region
        qemu_sys::memory_region_add_subregion_overlap(
            &mut armv7m.container,
            0xe0001000,
            &mut dwt_ptr.region,
            i32::MAX,
        );
    }
}

pub(crate) fn drop_dwt() {
    unsafe {
        DWT.take();
    }
}
