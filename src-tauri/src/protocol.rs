//! `svsm://localhost/icon/<key>` serves mod icons from memory, so they never travel as JSON.

use svsm_core::sync::RwLockExt as _;
use tauri::http::{header, Request, Response, StatusCode};
use tauri::{Manager, UriSchemeContext, Wry};

use crate::state::AppState;

pub fn handle(ctx: UriSchemeContext<'_, Wry>, request: Request<Vec<u8>>) -> Response<Vec<u8>> {
    let path = request.uri().path().trim_start_matches('/');
    let Some(key) = path.strip_prefix("icon/") else {
        return not_found();
    };
    let state = ctx.app_handle().state::<AppState>();
    let bytes = state.mods.read_safe().icons.get(key).cloned();
    match bytes {
        Some(b) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, sniff(&b))
            .header(header::CACHE_CONTROL, "max-age=3600")
            .body(b.as_ref().clone())
            .expect("valid response"),
        None => not_found(),
    }
}

fn sniff(b: &[u8]) -> &'static str {
    if b.starts_with(b"\x89PNG") {
        "image/png"
    } else if b.starts_with(&[0xFF, 0xD8]) {
        "image/jpeg"
    } else if b.starts_with(b"GIF8") {
        "image/gif"
    } else if b.len() > 12 && &b[8..12] == b"WEBP" {
        "image/webp"
    } else {
        "application/octet-stream"
    }
}

fn not_found() -> Response<Vec<u8>> {
    Response::builder().status(StatusCode::NOT_FOUND).body(Vec::new()).expect("valid response")
}
