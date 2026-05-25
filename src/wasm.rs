use crate::formatter::{Options, format_with_options};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn format_vdf(
    input: &str,
    reflow_comments: bool,
    bare_literals: bool,
) -> Result<String, JsValue> {
    format_with_options(
        input,
        Options {
            reflow_comments,
            bare_literals,
        },
    )
    .map_err(|err| JsValue::from_str(&err.to_string()))
}
