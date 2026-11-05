// SPDX-License-Identifier: GPL-3.0-only
// SPDX-FileCopyrightText: 2026 Krish Agarwal
fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!(
            "cargo:rustc-link-arg-cdylib=/DEF:{}",
            std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
                .join("exports.def")
                .display()
        );
        println!("cargo:rerun-if-changed=exports.def");
    }
}
