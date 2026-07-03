use super::components::Example;
use iocraft::prelude::*;

pub fn run() {
    if let Err(e) = smol::block_on(element!(Example).fullscreen().disable_mouse_capture()) {
        eprintln!("App error: {e}");
    }
}
