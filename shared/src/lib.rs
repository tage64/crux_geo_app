mod capabilities;
pub mod geo_app;
#[allow(unused)]
mod numbers;

use std::sync::LazyLock;

pub use capabilities::*;
pub use crux_core::{Core, Request, bridge::Bridge};
pub use geo_app::*;
use wasm_bindgen::prelude::wasm_bindgen;

uniffi::include_scaffolding!("shared");

static CORE: LazyLock<Bridge<GeoApp>> = LazyLock::new(|| Bridge::new(Core::new()));

#[wasm_bindgen]
pub fn process_event(data: &[u8]) -> Vec<u8> {
    CORE.process_event(data).unwrap()
}

#[wasm_bindgen]
pub fn handle_response(id: u32, data: &[u8]) -> Vec<u8> {
    CORE.handle_response(id, data).unwrap()
}

#[wasm_bindgen]
pub fn view() -> Vec<u8> {
    CORE.view().unwrap()
}
