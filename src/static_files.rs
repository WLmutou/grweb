use crate::Response;
use std::fs;
use std::path::{Path, PathBuf};

pub fn get_mime_type(path: &str) -> &'static str {
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    match ext {
        "html" | "htm" => "text/html;charset=utf-8",
        "css" => "text/css;charset=utf-8",
        "js" => "application/javascript;charset=utf-8",
        "json" => "application/json",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "webp" => "image/webp",
        "txt" => "text/plain;charset=utf-8",
        "xml" => "application/xml",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "wasm" => "application/wasm",
        _ => "application/octet-stream",
    }
}

pub fn serve_file(root_dir: &str, url_path: &str) -> Response {
    let relative_path = url_path.trim_start_matches('/');

    let file_path = PathBuf::from(root_dir).join(relative_path);

    let canonical_root = match fs::canonicalize(root_dir) {
        Ok(p) => p,
        Err(_) => return Response::internal_error(),
    };

    let canonical_path = match fs::canonicalize(&file_path) {
        Ok(p) => p,
        Err(_) => {
            let index_path = file_path.join("index.html");
            match fs::canonicalize(&index_path) {
                Ok(p) => p,
                Err(_) => return Response::not_found(),
            }
        }
    };

    if !canonical_path.starts_with(&canonical_root) {
        return Response::not_found();
    }

    if canonical_path.is_dir() {
        let index_path = canonical_path.join("index.html");
        if index_path.is_file() {
            match fs::read(&index_path) {
                Ok(data) => {
                    let mut resp = Response::new(200, data);
                    resp.headers = vec![(
                        "Content-Type".to_string(),
                        "text/html;charset=utf-8".to_string(),
                    )];
                    return resp;
                }
                Err(_) => return Response::not_found(),
            }
        }
        return Response::not_found();
    }

    match fs::read(&canonical_path) {
        Ok(data) => {
            let mime = get_mime_type(canonical_path.to_str().unwrap_or(""));
            let mut resp = Response::new(200, data);
            resp.headers = vec![("Content-Type".to_string(), mime.to_string())];
            resp
        }
        Err(_) => Response::not_found(),
    }
}
