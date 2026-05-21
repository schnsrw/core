#![no_main]
//! Hostile-input fuzzer for the OOXML preservation layer.
//!
//! `s1_ooxml::Package::parse` is the lowest tier of the DOCX preservation
//! bridge. Truncated zips, malformed XML, missing `[Content_Types].xml`,
//! circular relationships, and oversized parts must all fail with a
//! typed error — never panic.
//!
//! Run: `cargo +nightly fuzz run fuzz_ooxml_package`

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(pkg) = s1_ooxml::Package::parse(data) {
        // Round-trip must also be panic-free for any package we
        // successfully parsed.
        let _ = pkg.write();
    }
});
