// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: 2026 Krish Agarwal
// Trusted security test fixture, not shipped or used for archive processing.
use std::fs::File;
fn main() {
    let a:Vec<_>=std::env::args().collect();
    if a.get(1).is_some_and(|s|s=="memory") {
        let mut blocks=Vec::new();
        loop { let mut block=Vec::<u8>::new(); if block.try_reserve_exact(16<<20).is_err(){println!("memory_limited");return;} block.resize(16<<20,42); blocks.push(block); }
    }
    let read=|p:&str|std::fs::read(p).is_ok();
    let write=|p:&str|File::options().write(true).open(p).is_ok();
    println!("read={} outsider={} input_write={} outside_write={} scratch_write={} code_write={} shared_read={}",read(&a[1]),read(&a[2]),write(&a[1]),std::fs::write(&a[3],b"bad").is_ok(),std::fs::write(&a[4],b"scratch").is_ok(),write(&a[5]),read(&a[6]));
    println!("argument={}",a[7]);
}
