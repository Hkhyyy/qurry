//! Thin wasm-bindgen layer over `qurry-shared`: each function takes and returns plain JS values
//! (converted with serde-wasm-bindgen). No logic here. Built by `pnpm wasm` into `src/wasm/pkg`.

use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn ping(req: JsValue) -> Result<JsValue, JsError> {
    let req = serde_wasm_bindgen::from_value(req)?;
    Ok(serde_wasm_bindgen::to_value(&qurry_shared::ping(req))?)
}
