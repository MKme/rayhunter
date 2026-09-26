// Run the same pure model/renderer tests on a host without Linux framebuffer APIs:
// rustc --edition 2024 --test tools/test-tactical.rs -o target/tactical-tests.exe
#[path = "../daemon/src/display/bitmap_font.rs"]
mod bitmap_font;
#[path = "../daemon/src/display/tactical.rs"]
mod tactical;
