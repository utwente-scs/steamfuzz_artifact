use crate::{Exception,};

static mut INTERRUPT: Vec<Exception> = vec![];

/// Flush the pending interrupt queue into the NVIC immediately.
/// Call this whenever an interrupt was pushed to the queue outside of a BB hook
/// (e.g. during exception-exit processing) so QEMU sees it before the next TB.
pub fn flush_interrupt_queue() {
    inject_interrupt_fuzzer();
}

pub fn request_interrupt_injection(interrupt: Exception) {
    // log::info!("request_interrupt_injection(interrupt = {:?})", interrupt);

    unsafe {
        INTERRUPT.push(interrupt);
    }
}

pub(crate) fn inject_interrupt_fuzzer() -> bool {
    // not the systick irq
    while let Some(interrupt) = unsafe { INTERRUPT.pop() } {
        // log::info!("injecting fuzzer interrupt {}",  interrupt);

        let cpu = crate::qcontrol::cpu_mut();

        // let primask = cpu.env.v7m.primask[0];
        // log::info!("primask {:?}", primask);
        // let faultmask = cpu.env.v7m.faultmask[0];
        // log::info!("faultmask {:?}", faultmask);
        // let basepri = cpu.env.v7m.basepri[0];
        // log::info!("basepri {:?}", basepri);

        // enable cpu io
        // this should be safe as we are at the start of an execution block and manually set the PC
        let can_do_io = cpu.parent_obj.can_do_io;
        cpu.parent_obj.can_do_io = 1;

        pend_interrupt(interrupt);

        // kick cpu after interrupt injection
        cpu.parent_obj.halted = 0;

        // restore cpu io state
        cpu.parent_obj.can_do_io = can_do_io;
    }

    false

}


fn pend_interrupt(interrupt: Exception) {
    log::trace!("pend_interrupt(interrupt = {:?})", interrupt);

    #[cfg(feature = "arm")]
    {
        // NVIC needs special care
        if let Some(exception) = interrupt.as_nvic() {
            exception.pend();
            return;
        }

        if let Some(exception) = interrupt.as_cpu() {
            exception.pend();
            return;
        }

        unreachable!("invalid interrupt");
    }

    #[cfg(not(feature = "arm"))]
    interrupt.pend();
}
