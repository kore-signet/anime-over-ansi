pub mod codec;
#[cfg(feature = "midi")]
pub mod midi;
pub mod player;
pub mod subtitles;

pub struct TerminalCleanup;

impl Drop for TerminalCleanup {
    fn drop(&mut self) {
        println!("\x1b[?25h");
    }
}
