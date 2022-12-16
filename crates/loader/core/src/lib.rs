#![no_std]
#![no_main]
#![feature(atomic_from_mut)]
#![feature(exclusive_wrapper)]
#![feature(ptr_to_from_bits)]
#![feature(pointer_byte_offsets)]
#![feature(const_pointer_byte_offsets)]
#![allow(unreachable_code)]
#![allow(dead_code)]

use core::arch::asm;
use core::ops::Range;
use core::panic::PanicInfo;

use log::LevelFilter;

use loader_payload_types::{Payload, PayloadInfo};
use sel4_platform_info::PLATFORM_INFO;

mod barrier;
mod copy_payload_data;
mod debug;
mod drivers;
mod enter_kernel;
mod exception_handler;
mod fmt;
mod init_platform_state;
mod logging;
mod plat;
mod sanity_check;
mod smp;
mod stacks;

use barrier::Barrier;
use logging::Logger;

const LOG_LEVEL: LevelFilter = LevelFilter::Debug;

static LOGGER: Logger = Logger::new(LOG_LEVEL);

const MAX_NUM_NODES: usize = sel4_config::sel4_cfg_usize!(MAX_NUM_NODES);
const NUM_SECONDARY_CORES: usize = MAX_NUM_NODES - 1;

static KERNEL_ENTRY_BARRIER: Barrier = Barrier::new(MAX_NUM_NODES);

pub fn main<'a>(payload: &Payload<'a>, own_footprint: &Range<usize>) -> ! {
    debug::init();

    LOGGER.set().unwrap();

    log::info!("Starting loader");

    log::debug!("Platform info: {:#x?}", PLATFORM_INFO);
    log::debug!("Loader footprint: {:#x?}", own_footprint);
    log::debug!("Payload info: {:#x?}", payload.info);
    log::debug!("Payload regions:");
    for content in payload.data.iter() {
        log::debug!(
            "    0x{:x?} {:?}",
            content.phys_addr_range,
            content.content.is_some()
        );
    }

    {
        let own_footprint =
            own_footprint.start.try_into().unwrap()..own_footprint.end.try_into().unwrap();
        sanity_check::sanity_check(&own_footprint, &payload.data);
    }

    log::debug!("Copying payload data");
    copy_payload_data::copy_payload_data(&payload.data);

    smp::start_secondary_cores(&payload.info);

    common_epilogue(0, &payload.info)
}

fn secondary_main(core_id: usize, payload_info: &PayloadInfo) -> ! {
    common_epilogue(core_id, payload_info)
}

fn common_epilogue(core_id: usize, payload_info: &PayloadInfo) -> ! {
    if core_id == 0 {
        log::info!("Entering kernel");
    }
    KERNEL_ENTRY_BARRIER.wait();
    init_platform_state::init_platform_state_per_core(core_id);
    init_platform_state::init_platform_state_per_core_after_which_no_syncronization(core_id);
    enter_kernel::enter_kernel(&payload_info);
    fmt::debug_println_without_synchronization!("Core {}: failed to enter kernel", core_id);
    idle()
}

//

#[panic_handler]
extern "C" fn panic_handler(info: &PanicInfo) -> ! {
    log::error!("{}", info);
    idle()
}

fn idle() -> ! {
    loop {
        unsafe {
            asm!("wfe");
        }
    }
}

//

mod translation_tables {
    include!(concat!(env!("OUT_DIR"), "/translation_tables.rs"));
}
