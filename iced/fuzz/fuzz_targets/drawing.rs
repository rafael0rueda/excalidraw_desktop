//! Any bytes as a drawing file. How to run it: `iced/fuzz/Cargo.toml`.
#![no_main]

#[path = "../../crates/export/tests/common/exercise.rs"]
mod exercise;

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        // PNG export is slow; one input in sixteen, chosen by its length.
        exercise::exercise(text, data.len() % 16 == 0);
    }
});
