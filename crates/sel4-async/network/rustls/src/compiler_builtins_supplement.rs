//
// Copyright 2023, Colias Group, LLC
//
// SPDX-License-Identifier: Apache-2.0 OR ISC OR MIT
//

// https://github.com/rust-lang/compiler-builtins/pull/563
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
#[no_mangle]
pub extern "C" fn __bswapsi2(u: u32) -> u32 {
    u.swap_bytes()
}
