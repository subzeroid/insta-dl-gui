use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use tauri::http::{header, Method, Request, Response, StatusCode};

use crate::catalog::{Catalog, MediaFileKind};
use crate::library_commands::resolve_validated_catalog_file;

pub const MAX_PROTOCOL_BODY_BYTES: u64 = 32 * 1024 * 1024;
const DOCUMENT_CSP: &str = "default-src 'none'; sandbox";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ByteRange {
    start: u64,
    end: u64,
}

impl ByteRange {
    const fn len(self) -> u64 {
        self.end - self.start + 1
    }
}

pub async fn handle_library_protocol(
    catalog: Catalog,
    webview_label: &str,
    request: Request<Vec<u8>>,
) -> Response<Vec<u8>> {
    let is_head = request.method() == Method::HEAD;
    if webview_label != "main" {
        return empty_body_for_head(plain_response(StatusCode::NOT_FOUND, b"not found"), is_head);
    }
    if request.method() != Method::GET && request.method() != Method::HEAD {
        return Response::builder()
            .status(StatusCode::METHOD_NOT_ALLOWED)
            .header(header::ALLOW, "GET, HEAD")
            .header(header::CACHE_CONTROL, "no-store")
            .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
            .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
            .header(header::CONTENT_SECURITY_POLICY, DOCUMENT_CSP)
            .header(header::CONTENT_LENGTH, "18")
            .body(b"method not allowed".to_vec())
            .expect("static protocol response is valid");
    }
    let Some(file_id) = parse_media_file_id(request.uri()) else {
        return empty_body_for_head(plain_response(StatusCode::NOT_FOUND, b"not found"), is_head);
    };
    // RFC 7233 applies Range to GET. HEAD reports the full representation
    // headers and never attempts range parsing or body reads.
    let range_headers = if is_head {
        Ok(Vec::new())
    } else {
        request
            .headers()
            .get_all(header::RANGE)
            .iter()
            .map(|value| value.to_str().map(str::to_owned).map_err(|_| ()))
            .collect::<Result<Vec<_>, _>>()
    };

    let response = tauri::async_runtime::spawn_blocking(move || {
        serve_catalog_file(&catalog, file_id, is_head, range_headers)
    })
    .await
    .unwrap_or_else(|_| plain_response(StatusCode::NOT_FOUND, b"not found"));
    empty_body_for_head(response, is_head)
}

fn parse_media_file_id(uri: &tauri::http::Uri) -> Option<i64> {
    // Wry reverses its Windows/Android `http://library.localhost` workaround
    // before invoking custom protocol handlers, so every platform arrives as
    // the same canonical authority.
    if uri.scheme_str() != Some("library")
        || uri.authority().map(|authority| authority.as_str()) != Some("localhost")
        || uri.query().is_some()
    {
        return None;
    }
    let mut segments = uri.path().split('/');
    if segments.next() != Some("")
        || segments.next() != Some("media")
        || segments.clone().count() != 1
    {
        return None;
    }
    let segment = segments.next()?;
    if segment.is_empty() || !segment.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let id = segment.parse::<i64>().ok()?;
    (id > 0 && id.to_string() == segment).then_some(id)
}

fn serve_catalog_file(
    catalog: &Catalog,
    file_id: i64,
    is_head: bool,
    range_headers: Result<Vec<String>, ()>,
) -> Response<Vec<u8>> {
    let Ok(file) = resolve_validated_catalog_file(catalog, file_id) else {
        return plain_response(StatusCode::NOT_FOUND, b"not found");
    };
    let Some(relative_media) = protocol_media(&file.relative_path) else {
        return plain_response(StatusCode::NOT_FOUND, b"not found");
    };
    let Some(canonical_media) = protocol_media(&file.canonical_path) else {
        return plain_response(StatusCode::NOT_FOUND, b"not found");
    };
    if relative_media.kind != file.kind || canonical_media.kind != file.kind {
        return plain_response(StatusCode::NOT_FOUND, b"not found");
    }
    let Ok(metadata) = std::fs::metadata(&file.canonical_path) else {
        return plain_response(StatusCode::NOT_FOUND, b"not found");
    };
    let full_length = metadata.len();
    let range = match range_headers {
        Ok(headers) if headers.is_empty() => None,
        Ok(headers) if headers.len() == 1 => match parse_range(&headers[0], full_length) {
            Ok(range) => Some(range),
            Err(()) => return range_not_satisfiable(full_length),
        },
        _ => return range_not_satisfiable(full_length),
    };
    let selected = range.unwrap_or(ByteRange {
        start: 0,
        end: full_length.saturating_sub(1),
    });
    let response_length = range.map_or(full_length, ByteRange::len);
    // The Tauri responder owns a Vec body. Return 416 before opening the file
    // when one response would exceed the preview budget so media clients can
    // retry with a smaller byte range.
    if !is_head && response_length > MAX_PROTOCOL_BODY_BYTES {
        return range_not_satisfiable(full_length);
    }
    let body = if is_head {
        Vec::new()
    } else {
        let Ok(mut file) = std::fs::File::open(&file.canonical_path) else {
            return plain_response(StatusCode::NOT_FOUND, b"not found");
        };
        match read_bounded(&mut file, selected.start, response_length) {
            Ok(body) => body,
            Err(_) => return plain_response(StatusCode::NOT_FOUND, b"not found"),
        }
    };
    let mut builder = Response::builder()
        .status(if range.is_some() {
            StatusCode::PARTIAL_CONTENT
        } else {
            StatusCode::OK
        })
        .header(header::CONTENT_TYPE, canonical_media.content_type)
        .header(header::CONTENT_LENGTH, response_length.to_string())
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CACHE_CONTROL, "no-store")
        .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
        .header(header::CONTENT_SECURITY_POLICY, DOCUMENT_CSP);
    if let Some(range) = range {
        builder = builder.header(
            header::CONTENT_RANGE,
            format!("bytes {}-{}/{}", range.start, range.end, full_length),
        );
    }
    builder
        .body(body)
        .expect("validated protocol headers produce a response")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ProtocolMedia {
    kind: MediaFileKind,
    content_type: &'static str,
}

fn protocol_media(path: &Path) -> Option<ProtocolMedia> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    let (kind, content_type) = match extension.as_str() {
        "jpg" | "jpeg" => (MediaFileKind::Photo, "image/jpeg"),
        "png" => (MediaFileKind::Photo, "image/png"),
        "webp" => (MediaFileKind::Photo, "image/webp"),
        "mp4" => (MediaFileKind::Video, "video/mp4"),
        "mov" => (MediaFileKind::Video, "video/quicktime"),
        _ => return None,
    };
    Some(ProtocolMedia { kind, content_type })
}

fn parse_range(value: &str, full_length: u64) -> Result<ByteRange, ()> {
    let (unit, requested) = value.split_once('=').ok_or(())?;
    if !unit.eq_ignore_ascii_case("bytes")
        || requested.is_empty()
        || requested.contains(',')
        || full_length == 0
    {
        return Err(());
    }
    let (start, end) = requested.split_once('-').ok_or(())?;
    match (start.is_empty(), end.is_empty()) {
        (false, false) => {
            let start = start.parse::<u64>().map_err(|_| ())?;
            let end = end.parse::<u64>().map_err(|_| ())?;
            if start >= full_length || start > end {
                return Err(());
            }
            Ok(ByteRange {
                start,
                end: end.min(full_length - 1),
            })
        }
        (false, true) => {
            let start = start.parse::<u64>().map_err(|_| ())?;
            if start >= full_length {
                return Err(());
            }
            Ok(ByteRange {
                start,
                end: full_length - 1,
            })
        }
        (true, false) => {
            let suffix = end.parse::<u64>().map_err(|_| ())?;
            if suffix == 0 {
                return Err(());
            }
            let selected = suffix.min(full_length);
            Ok(ByteRange {
                start: full_length - selected,
                end: full_length - 1,
            })
        }
        (true, true) => Err(()),
    }
}

pub fn read_bounded<R: Read + Seek>(
    reader: &mut R,
    start: u64,
    length: u64,
) -> std::io::Result<Vec<u8>> {
    if length > MAX_PROTOCOL_BODY_BYTES {
        return Err(std::io::Error::other(
            "requested byte range exceeds protocol body limit",
        ));
    }
    let expected = usize::try_from(length)
        .map_err(|_| std::io::Error::other("requested byte range is too large"))?;
    reader.seek(SeekFrom::Start(start))?;
    let mut body = vec![0; expected];
    reader.take(length).read_exact(&mut body)?;
    Ok(body)
}

fn range_not_satisfiable(full_length: u64) -> Response<Vec<u8>> {
    Response::builder()
        .status(StatusCode::RANGE_NOT_SATISFIABLE)
        .header(header::CONTENT_RANGE, format!("bytes */{full_length}"))
        .header(header::CACHE_CONTROL, "no-store")
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
        .header(header::CONTENT_SECURITY_POLICY, DOCUMENT_CSP)
        .header(header::CONTENT_LENGTH, "21")
        .body(b"range not satisfiable".to_vec())
        .expect("static protocol response is valid")
}

fn plain_response(status: StatusCode, body: &'static [u8]) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .header(header::CACHE_CONTROL, "no-store")
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
        .header(header::CONTENT_SECURITY_POLICY, DOCUMENT_CSP)
        .header(header::CONTENT_LENGTH, body.len().to_string())
        .body(body.to_vec())
        .expect("static protocol response is valid")
}

fn empty_body_for_head(mut response: Response<Vec<u8>>, is_head: bool) -> Response<Vec<u8>> {
    if is_head {
        response.body_mut().clear();
    }
    response
}
