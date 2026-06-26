//! Thin platform abstraction for opening and saving files, so the editor logic
//! stays identical on native and web targets.

/// Trigger a "save" of `contents` under `filename`.
///
/// On native this opens a save dialog; on web it triggers a browser download.
#[cfg(not(target_arch = "wasm32"))]
pub fn save_file(filename: &str, contents: &str) -> Result<bool, String> {
    if let Some(path) = rfd::FileDialog::new()
        .set_file_name(filename)
        .add_filter("DBC", &["dbc"])
        .save_file()
    {
        std::fs::write(&path, contents).map_err(|e| e.to_string())?;
        Ok(true)
    } else {
        Ok(false)
    }
}

/// Open a native file picker and return the chosen file as (name, contents).
#[cfg(not(target_arch = "wasm32"))]
pub fn open_file() -> Option<(String, String)> {
    let path = rfd::FileDialog::new()
        .add_filter("DBC", &["dbc"])
        .pick_file()?;
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "untitled.dbc".to_string());
    let contents = std::fs::read_to_string(&path).ok()?;
    Some((name, contents))
}

#[cfg(target_arch = "wasm32")]
pub fn save_file(filename: &str, contents: &str) -> Result<bool, String> {
    use wasm_bindgen::JsCast;

    let window = web_sys::window().ok_or("no window")?;
    let document = window.document().ok_or("no document")?;

    // Build a Blob from the text and a temporary object URL.
    let parts = js_sys::Array::new();
    parts.push(&wasm_bindgen::JsValue::from_str(contents));
    let opts = web_sys::BlobPropertyBag::new();
    opts.set_type("text/plain");
    let blob = web_sys::Blob::new_with_str_sequence_and_options(&parts, &opts)
        .map_err(|_| "blob creation failed")?;
    let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(|_| "url failed")?;

    // Click a transient anchor to start the download.
    let anchor = document
        .create_element("a")
        .map_err(|_| "anchor failed")?
        .dyn_into::<web_sys::HtmlAnchorElement>()
        .map_err(|_| "anchor cast failed")?;
    anchor.set_href(&url);
    anchor.set_download(filename);
    anchor.click();
    let _ = web_sys::Url::revoke_object_url(&url);
    Ok(true)
}

/// On web, file opening is handled by drag-and-drop (see the app), so the
/// picker button is native-only. This stub keeps call sites uniform.
#[cfg(target_arch = "wasm32")]
pub fn open_file() -> Option<(String, String)> {
    None
}
