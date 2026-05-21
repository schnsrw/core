#![no_main]
//! Hostile-input fuzzer for the Phase 2b edit-and-export splice path.
//!
//! Exercises `read_with_package_and_origin` (origin-table build) +
//! `update_toc` (dirty-tracking trigger) + `export(Docx)` (per-NodeId
//! splice). Any panic on a malformed input is a bug — the parser is
//! supposed to be lenient and the splice is supposed to fall back to
//! wholesale regenerate on structural mismatch.
//!
//! Run: `cargo +nightly fuzz run fuzz_docx_phase2b`

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let engine = s1engine::Engine::new();
    if let Ok(mut doc) = engine.open_as(data, s1engine::Format::Docx) {
        // Fire the dirty-tracking path used by docx_edit_coverage.
        doc.update_toc();
        let _ = doc.export(s1engine::Format::Docx);
    }
});
