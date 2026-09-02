use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use base64::Engine;
use bytes::Bytes;
use futures_util::{Stream, StreamExt};
use tauri::http::{header, HeaderMap, Method, Request, Response, StatusCode};
use url::Url;

use crate::cdn::{self, Sniffed};

const MAX_PROTOCOL_BODY_BYTES: u64 = crate::library_protocol::MAX_PROTOCOL_BODY_BYTES;
const MAX_DECODED_URL_BYTES: usize = 16 * 1024;
const MAX_ENCODED_URL_BYTES: usize = (MAX_DECODED_URL_BYTES * 4).div_ceil(3);
const MAX_CONCURRENT_REMOTE_MEDIA: usize = 4;
const REMOTE_MEDIA_DIAGNOSTIC_COOLDOWN: Duration = Duration::from_secs(30);
const DOCUMENT_CSP: &str = "default-src 'none'; sandbox";
static REMOTE_MEDIA_SLOTS: tokio::sync::Semaphore =
    tokio::sync::Semaphore::const_new(MAX_CONCURRENT_REMOTE_MEDIA);
static REMOTE_MEDIA_DIAGNOSTIC_LIMITER: Mutex<DiagnosticRateLimiter> =
    Mutex::new(DiagnosticRateLimiter::new(REMOTE_MEDIA_DIAGNOSTIC_COOLDOWN));

type UpstreamBody = Pin<Box<dyn Stream<Item = Result<Bytes, ()>> + Send + 'static>>;
type FetchFuture = Pin<Box<dyn Future<Output = Result<UpstreamResponse, ()>> + Send + 'static>>;

struct UpstreamResponse {
    status: StatusCode,
    headers: HeaderMap,
    body: UpstreamBody,
}

trait RemoteFetcher: Clone + Send + Sync + 'static {
    fn send(&self, url: Url, range: Option<String>) -> FetchFuture;
}

#[derive(Clone)]
struct ReqwestFetcher(reqwest::Client);

impl RemoteFetcher for ReqwestFetcher {
    fn send(&self, url: Url, range: Option<String>) -> FetchFuture {
        let client = self.0.clone();
        Box::pin(async move {
            let mut request = client.get(url).timeout(Duration::from_secs(30));
            if let Some(range) = range {
                request = request.header(reqwest::header::RANGE, range);
            }
            let response = request.send().await.map_err(|_| ())?;
            let status = response.status();
            let headers = response.headers().clone();
            let body = Box::pin(response.bytes_stream().map(|chunk| chunk.map_err(|_| ())))
                as UpstreamBody;
            Ok(UpstreamResponse {
                status,
                headers,
                body,
            })
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ByteRangeSpec {
    FromTo { start: u64, end: u64 },
    From { start: u64 },
    Suffix { length: u64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ResolvedRange {
    start: u64,
    end: u64,
    total: u64,
}

impl ResolvedRange {
    const fn len(self) -> u64 {
        self.end - self.start + 1
    }

    fn header_value(self) -> String {
        format!("bytes={}-{}", self.start, self.end)
    }

    fn content_range(self) -> String {
        format!("bytes {}-{}/{}", self.start, self.end, self.total)
    }
}

impl ByteRangeSpec {
    fn parse(value: &str) -> Result<Self, ()> {
        let (unit, value) = value.split_once('=').ok_or(())?;
        if !unit.eq_ignore_ascii_case("bytes")
            || value.is_empty()
            || value.contains(',')
            || value.bytes().filter(|byte| *byte == b'-').count() != 1
        {
            return Err(());
        }
        let (start, end) = value.split_once('-').ok_or(())?;
        let parse_number = |number: &str| {
            if number.is_empty() || !number.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(());
            }
            number.parse::<u64>().map_err(|_| ())
        };
        match (start.is_empty(), end.is_empty()) {
            (false, false) => {
                let start = parse_number(start)?;
                let end = parse_number(end)?;
                if start > end {
                    return Err(());
                }
                Ok(Self::FromTo { start, end })
            }
            (false, true) => Ok(Self::From {
                start: parse_number(start)?,
            }),
            (true, false) => {
                let length = parse_number(end)?;
                if length == 0 {
                    return Err(());
                }
                Ok(Self::Suffix { length })
            }
            (true, true) => Err(()),
        }
    }

    fn resolve(self, total: u64) -> Result<ResolvedRange, ()> {
        if total == 0 {
            return Err(());
        }
        let (start, end) = match self {
            Self::FromTo { start, end } if start < total => (start, end.min(total - 1)),
            Self::From { start } if start < total => (start, total - 1),
            Self::Suffix { length } => {
                let selected = length.min(total);
                (total - selected, total - 1)
            }
            _ => return Err(()),
        };
        Ok(ResolvedRange { start, end, total })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RemoteMediaDiagnostic {
    Network,
    UpstreamStatus(u16),
    Validation,
    RangeRejected(Option<u64>),
    ResponseTooLarge(Option<u64>),
}

impl fmt::Display for RemoteMediaDiagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Network => formatter.write_str("remote media fetch failed: network"),
            Self::UpstreamStatus(status) => {
                write!(formatter, "remote media upstream status: {status}")
            }
            Self::Validation => formatter.write_str("remote media response rejected: validation"),
            Self::RangeRejected(Some(length)) => {
                write!(formatter, "remote media range rejected: length {length}")
            }
            Self::RangeRejected(None) => formatter.write_str("remote media range rejected"),
            Self::ResponseTooLarge(Some(length)) => {
                write!(
                    formatter,
                    "remote media response too large: length {length}"
                )
            }
            Self::ResponseTooLarge(None) => formatter.write_str("remote media response too large"),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct DiagnosticEmission {
    diagnostic: RemoteMediaDiagnostic,
    suppressed: u64,
}

impl fmt::Display for DiagnosticEmission {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.diagnostic.fmt(formatter)?;
        match self.suppressed {
            0 => Ok(()),
            1 => formatter.write_str("; suppressed 1 similar event"),
            count => write!(formatter, "; suppressed {count} similar events"),
        }
    }
}

struct DiagnosticRateLimiter {
    cooldown: Duration,
    last_emitted: [Option<Instant>; 5],
    suppressed: [u64; 5],
}

impl DiagnosticRateLimiter {
    const fn new(cooldown: Duration) -> Self {
        Self {
            cooldown,
            last_emitted: [None; 5],
            suppressed: [0; 5],
        }
    }

    fn record(
        &mut self,
        diagnostic: RemoteMediaDiagnostic,
        now: Instant,
    ) -> Option<DiagnosticEmission> {
        let category = diagnostic.category_index();
        if self.last_emitted[category]
            .is_some_and(|last| now.saturating_duration_since(last) < self.cooldown)
        {
            self.suppressed[category] = self.suppressed[category].saturating_add(1);
            return None;
        }
        let suppressed = self.suppressed[category];
        self.last_emitted[category] = Some(now);
        self.suppressed[category] = 0;
        Some(DiagnosticEmission {
            diagnostic,
            suppressed,
        })
    }
}

impl RemoteMediaDiagnostic {
    const fn category_index(self) -> usize {
        match self {
            Self::Network => 0,
            Self::UpstreamStatus(_) => 1,
            Self::Validation => 2,
            Self::RangeRejected(_) => 3,
            Self::ResponseTooLarge(_) => 4,
        }
    }
}

trait DiagnosticSink: Clone + Send + Sync + 'static {
    fn emit(&self, diagnostic: RemoteMediaDiagnostic);
}

#[derive(Clone, Copy)]
struct StderrDiagnosticSink;

impl DiagnosticSink for StderrDiagnosticSink {
    fn emit(&self, diagnostic: RemoteMediaDiagnostic) {
        let emission = REMOTE_MEDIA_DIAGNOSTIC_LIMITER
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .record(diagnostic, Instant::now());
        if let Some(emission) = emission {
            eprintln!("{emission}");
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum ProtocolResponseError {
    NotFound,
    RangeNotSatisfiable(Option<u64>),
}

#[derive(Debug, Clone, Copy)]
struct ProtocolError {
    response: ProtocolResponseError,
    diagnostic: RemoteMediaDiagnostic,
}

impl ProtocolError {
    const fn validation() -> Self {
        Self {
            response: ProtocolResponseError::NotFound,
            diagnostic: RemoteMediaDiagnostic::Validation,
        }
    }

    const fn network() -> Self {
        Self {
            response: ProtocolResponseError::NotFound,
            diagnostic: RemoteMediaDiagnostic::Network,
        }
    }

    const fn upstream_status(status: StatusCode) -> Self {
        Self {
            response: ProtocolResponseError::NotFound,
            diagnostic: RemoteMediaDiagnostic::UpstreamStatus(status.as_u16()),
        }
    }

    const fn range_rejected(total: Option<u64>, length: Option<u64>) -> Self {
        Self {
            response: ProtocolResponseError::RangeNotSatisfiable(total),
            diagnostic: RemoteMediaDiagnostic::RangeRejected(length),
        }
    }

    const fn response_too_large(total: Option<u64>, length: Option<u64>) -> Self {
        Self {
            response: ProtocolResponseError::RangeNotSatisfiable(total),
            diagnostic: RemoteMediaDiagnostic::ResponseTooLarge(length),
        }
    }

    const fn declared_length_mismatch(total: Option<u64>) -> Self {
        Self {
            response: ProtocolResponseError::RangeNotSatisfiable(total),
            diagnostic: RemoteMediaDiagnostic::Validation,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Probe {
    kind: Sniffed,
    total: u64,
}

pub(crate) async fn handle_remote_media_protocol(
    client: reqwest::Client,
    webview_label: &str,
    request: Request<Vec<u8>>,
) -> Response<Vec<u8>> {
    with_remote_media_slot(handle_remote_media_protocol_with_fetcher(
        ReqwestFetcher(client),
        webview_label,
        request,
    ))
    .await
}

async fn with_remote_media_slot<F: Future>(future: F) -> F::Output {
    let _permit = REMOTE_MEDIA_SLOTS
        .acquire()
        .await
        .expect("static remote media semaphore is never closed");
    future.await
}

async fn handle_remote_media_protocol_with_fetcher<F: RemoteFetcher>(
    fetcher: F,
    webview_label: &str,
    request: Request<Vec<u8>>,
) -> Response<Vec<u8>> {
    handle_remote_media_protocol_with_fetcher_and_sink(
        fetcher,
        StderrDiagnosticSink,
        webview_label,
        request,
    )
    .await
}

async fn handle_remote_media_protocol_with_fetcher_and_sink<F: RemoteFetcher, S: DiagnosticSink>(
    fetcher: F,
    diagnostics: S,
    webview_label: &str,
    request: Request<Vec<u8>>,
) -> Response<Vec<u8>> {
    let is_head = request.method() == Method::HEAD;
    if webview_label != "main" {
        return empty_body_for_head(plain_response(StatusCode::NOT_FOUND, b"not found"), is_head);
    }
    if request.method() != Method::GET && !is_head {
        return method_not_allowed();
    }
    let Some(url) = parse_protocol_uri(request.uri()) else {
        return empty_body_for_head(plain_response(StatusCode::NOT_FOUND, b"not found"), is_head);
    };
    let url = match validate_target(url) {
        Ok(url) => url,
        Err(error) => return protocol_error_response(&diagnostics, error, is_head),
    };

    let result = if is_head {
        serve_head(fetcher, url).await
    } else {
        let range = match collect_range(request.headers()) {
            Ok(range) => range,
            Err(()) => {
                diagnostics.emit(RemoteMediaDiagnostic::RangeRejected(None));
                return range_not_satisfiable(None);
            }
        };
        if let Some(range) = range {
            serve_range(fetcher, url, range).await
        } else {
            serve_full(fetcher, url).await
        }
    };
    match result {
        Ok(response) => empty_body_for_head(response, is_head),
        Err(error) => protocol_error_response(&diagnostics, error, is_head),
    }
}

fn protocol_error_response<S: DiagnosticSink>(
    diagnostics: &S,
    error: ProtocolError,
    is_head: bool,
) -> Response<Vec<u8>> {
    diagnostics.emit(error.diagnostic);
    match error.response {
        ProtocolResponseError::NotFound => {
            empty_body_for_head(plain_response(StatusCode::NOT_FOUND, b"not found"), is_head)
        }
        ProtocolResponseError::RangeNotSatisfiable(total) => {
            empty_body_for_head(range_not_satisfiable(total), is_head)
        }
    }
}

fn parse_protocol_uri(uri: &tauri::http::Uri) -> Option<Url> {
    if uri.scheme_str() != Some("remote-media")
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
    let encoded = segments.next()?;
    if encoded.is_empty()
        || encoded.len() > MAX_ENCODED_URL_BYTES
        || !encoded
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return None;
    }
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(encoded)
        .ok()?;
    if decoded.len() > MAX_DECODED_URL_BYTES
        || base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&decoded) != encoded
    {
        return None;
    }
    let raw = std::str::from_utf8(&decoded).ok()?;
    Url::parse(raw).ok()
}

fn validate_target(url: Url) -> Result<Url, ProtocolError> {
    if url.as_str().len() > MAX_DECODED_URL_BYTES
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || url.port_or_known_default() != Some(443)
        || cdn::validate_remote_url(url.as_str(), false).is_err()
    {
        return Err(ProtocolError::validation());
    }
    Ok(url)
}

fn collect_range(headers: &HeaderMap) -> Result<Option<ByteRangeSpec>, ()> {
    let values = headers
        .get_all(header::RANGE)
        .iter()
        .map(|value| value.to_str().map_err(|_| ()))
        .collect::<Result<Vec<_>, _>>()?;
    match values.as_slice() {
        [] => Ok(None),
        [value] => ByteRangeSpec::parse(value).map(Some),
        _ => Err(()),
    }
}

async fn serve_full<F: RemoteFetcher>(
    fetcher: F,
    url: Url,
) -> Result<Response<Vec<u8>>, ProtocolError> {
    let response = fetch_follow_redirects(fetcher, url, None).await?;
    if response.status != StatusCode::OK {
        return Err(ProtocolError::upstream_status(response.status));
    }
    let declared_length = content_length(&response.headers)?;
    if declared_length.is_some_and(|length| length > MAX_PROTOCOL_BODY_BYTES) {
        return Err(ProtocolError::response_too_large(
            declared_length,
            declared_length,
        ));
    }
    let declared_type = content_type(&response.headers)?;
    let body = read_entire(response.body, MAX_PROTOCOL_BODY_BYTES, declared_length).await?;
    let kind = sniff_and_validate(&body, declared_type.as_deref())?;
    Ok(media_response(StatusCode::OK, kind, body, None))
}

async fn serve_head<F: RemoteFetcher>(
    fetcher: F,
    url: Url,
) -> Result<Response<Vec<u8>>, ProtocolError> {
    let probe = probe_media(fetcher, url).await?;
    Ok(media_response(
        StatusCode::OK,
        probe.kind,
        Vec::new(),
        Some((probe.total, None)),
    ))
}

async fn serve_range<F: RemoteFetcher>(
    fetcher: F,
    url: Url,
    requested: ByteRangeSpec,
) -> Result<Response<Vec<u8>>, ProtocolError> {
    let probe = probe_media(fetcher.clone(), url.clone()).await?;
    let selected = requested
        .resolve(probe.total)
        .map_err(|_| ProtocolError::range_rejected(Some(probe.total), None))?;
    if selected.len() > MAX_PROTOCOL_BODY_BYTES {
        return Err(ProtocolError::range_rejected(
            Some(probe.total),
            Some(selected.len()),
        ));
    }
    let response = fetch_follow_redirects(fetcher, url, Some(selected.header_value())).await?;
    if response.status == StatusCode::RANGE_NOT_SATISFIABLE {
        let total = unsatisfied_total(&response.headers)?;
        if total != probe.total {
            return Err(ProtocolError::validation());
        }
        return Err(ProtocolError::range_rejected(Some(total), None));
    }
    if response.status == StatusCode::OK {
        let declared_length = content_length(&response.headers)?;
        if declared_length.is_some_and(|length| length > MAX_PROTOCOL_BODY_BYTES) {
            return Err(ProtocolError::response_too_large(
                declared_length,
                declared_length,
            ));
        }
        let declared_type = content_type(&response.headers)?;
        let body = read_entire(response.body, MAX_PROTOCOL_BODY_BYTES, declared_length).await?;
        let kind = sniff_and_validate(&body, declared_type.as_deref())?;
        return Ok(media_response(StatusCode::OK, kind, body, None));
    }
    if response.status != StatusCode::PARTIAL_CONTENT {
        return Err(ProtocolError::upstream_status(response.status));
    }
    let actual = satisfied_range(&response.headers)?;
    if actual != selected || content_length(&response.headers)? != Some(selected.len()) {
        return Err(ProtocolError::validation());
    }
    let declared_type = content_type(&response.headers)?;
    if declared_type
        .as_deref()
        .is_some_and(|declared| !cdn::ct_compatible(declared, probe.kind))
    {
        return Err(ProtocolError::validation());
    }
    let body = read_entire(response.body, MAX_PROTOCOL_BODY_BYTES, Some(selected.len())).await?;
    Ok(media_response(
        StatusCode::PARTIAL_CONTENT,
        probe.kind,
        body,
        Some((probe.total, Some(selected))),
    ))
}

async fn probe_media<F: RemoteFetcher>(fetcher: F, url: Url) -> Result<Probe, ProtocolError> {
    let response = fetch_follow_redirects(
        fetcher,
        url,
        Some(format!("bytes=0-{}", cdn::SNIFF_SIZE - 1)),
    )
    .await?;
    let (total, expected) = match response.status {
        StatusCode::PARTIAL_CONTENT => {
            let range = satisfied_range(&response.headers)?;
            if range.start != 0 || range.end >= cdn::SNIFF_SIZE as u64 {
                return Err(ProtocolError::validation());
            }
            if content_length(&response.headers)? != Some(range.len()) {
                return Err(ProtocolError::validation());
            }
            (range.total, Some(range.len()))
        }
        StatusCode::OK => {
            let length = content_length(&response.headers)?.ok_or(ProtocolError::validation())?;
            if length == 0 {
                return Err(ProtocolError::validation());
            }
            (length, None)
        }
        _ => return Err(ProtocolError::upstream_status(response.status)),
    };
    let declared_type = content_type(&response.headers)?;
    let prefix = read_prefix(response.body, cdn::SNIFF_SIZE, expected).await?;
    let kind = sniff_and_validate(&prefix, declared_type.as_deref())?;
    Ok(Probe { kind, total })
}

async fn fetch_follow_redirects<F: RemoteFetcher>(
    fetcher: F,
    mut url: Url,
    range: Option<String>,
) -> Result<UpstreamResponse, ProtocolError> {
    let mut redirects = 0;
    loop {
        url = validate_target(url)?;
        let response = fetcher
            .send(url.clone(), range.clone())
            .await
            .map_err(|_| ProtocolError::network())?;
        if !response.status.is_redirection() {
            return Ok(response);
        }
        if redirects == cdn::MAX_REDIRECTS {
            return Err(ProtocolError::validation());
        }
        redirects += 1;
        let location = single_header(&response.headers, header::LOCATION)?
            .ok_or(ProtocolError::validation())?;
        url = validate_target(
            url.join(&location)
                .map_err(|_| ProtocolError::validation())?,
        )?;
    }
}

async fn read_entire(
    mut body: UpstreamBody,
    limit: u64,
    expected: Option<u64>,
) -> Result<Vec<u8>, ProtocolError> {
    let mut output = Vec::new();
    while let Some(chunk) = body.next().await {
        let chunk = chunk.map_err(|_| ProtocolError::network())?;
        let new_length = (output.len() as u64)
            .checked_add(chunk.len() as u64)
            .ok_or(ProtocolError::validation())?;
        if new_length > limit {
            return Err(ProtocolError::response_too_large(
                expected,
                Some(new_length),
            ));
        }
        if expected.is_some_and(|expected| new_length > expected) {
            return Err(ProtocolError::declared_length_mismatch(expected));
        }
        output.extend_from_slice(&chunk);
    }
    if output.is_empty() || expected.is_some_and(|expected| expected != output.len() as u64) {
        return Err(ProtocolError::validation());
    }
    Ok(output)
}

async fn read_prefix(
    mut body: UpstreamBody,
    limit: usize,
    expected: Option<u64>,
) -> Result<Vec<u8>, ProtocolError> {
    let mut output = Vec::with_capacity(limit);
    while output.len() < limit {
        let Some(chunk) = body.next().await else {
            break;
        };
        let chunk = chunk.map_err(|_| ProtocolError::network())?;
        let remaining = limit - output.len();
        output.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
    }
    if output.is_empty() || expected.is_some_and(|expected| expected != output.len() as u64) {
        return Err(ProtocolError::validation());
    }
    Ok(output)
}

fn sniff_and_validate(body: &[u8], declared: Option<&str>) -> Result<Sniffed, ProtocolError> {
    let kind = Sniffed::detect(body).ok_or(ProtocolError::validation())?;
    if declared.is_some_and(|declared| !cdn::ct_compatible(declared, kind)) {
        return Err(ProtocolError::validation());
    }
    Ok(kind)
}

fn content_length(headers: &HeaderMap) -> Result<Option<u64>, ProtocolError> {
    let Some(value) = single_header(headers, header::CONTENT_LENGTH)? else {
        return Ok(None);
    };
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(ProtocolError::validation());
    }
    value
        .parse::<u64>()
        .map(Some)
        .map_err(|_| ProtocolError::validation())
}

fn content_type(headers: &HeaderMap) -> Result<Option<String>, ProtocolError> {
    Ok(single_header(headers, header::CONTENT_TYPE)?.map(|value| cdn::normalize_ct(&value)))
}

fn single_header(
    headers: &HeaderMap,
    name: header::HeaderName,
) -> Result<Option<String>, ProtocolError> {
    let values = headers.get_all(name).iter().collect::<Vec<_>>();
    match values.as_slice() {
        [] => Ok(None),
        [value] => value
            .to_str()
            .map(|value| Some(value.to_owned()))
            .map_err(|_| ProtocolError::validation()),
        _ => Err(ProtocolError::validation()),
    }
}

fn satisfied_range(headers: &HeaderMap) -> Result<ResolvedRange, ProtocolError> {
    let value = single_header(headers, header::CONTENT_RANGE)?.ok_or(ProtocolError::validation())?;
    let value = value
        .strip_prefix("bytes ")
        .ok_or(ProtocolError::validation())?;
    let (range, total) = value.split_once('/').ok_or(ProtocolError::validation())?;
    let (start, end) = range.split_once('-').ok_or(ProtocolError::validation())?;
    let parse = |value: &str| {
        if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(ProtocolError::validation());
        }
        value
            .parse::<u64>()
            .map_err(|_| ProtocolError::validation())
    };
    let range = ResolvedRange {
        start: parse(start)?,
        end: parse(end)?,
        total: parse(total)?,
    };
    if range.total == 0 || range.start > range.end || range.end >= range.total {
        return Err(ProtocolError::validation());
    }
    Ok(range)
}

fn unsatisfied_total(headers: &HeaderMap) -> Result<u64, ProtocolError> {
    let value = single_header(headers, header::CONTENT_RANGE)?.ok_or(ProtocolError::validation())?;
    let total = value
        .strip_prefix("bytes */")
        .ok_or(ProtocolError::validation())?;
    if total.is_empty() || !total.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(ProtocolError::validation());
    }
    total.parse().map_err(|_| ProtocolError::validation())
}

fn media_response(
    status: StatusCode,
    kind: Sniffed,
    body: Vec<u8>,
    ranged: Option<(u64, Option<ResolvedRange>)>,
) -> Response<Vec<u8>> {
    let content_length = ranged
        .and_then(|(_, range)| range.map(ResolvedRange::len))
        .unwrap_or_else(|| ranged.map_or(body.len() as u64, |(total, _)| total));
    let mut builder = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, kind.mime())
        .header(header::CONTENT_LENGTH, content_length.to_string())
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CACHE_CONTROL, "no-store")
        .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
        .header(header::CONTENT_SECURITY_POLICY, DOCUMENT_CSP);
    if let Some((_, Some(range))) = ranged {
        builder = builder.header(header::CONTENT_RANGE, range.content_range());
    }
    builder
        .body(body)
        .expect("validated remote media headers produce a response")
}

fn method_not_allowed() -> Response<Vec<u8>> {
    Response::builder()
        .status(StatusCode::METHOD_NOT_ALLOWED)
        .header(header::ALLOW, "GET, HEAD")
        .header(header::CACHE_CONTROL, "no-store")
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
        .header(header::CONTENT_SECURITY_POLICY, DOCUMENT_CSP)
        .header(header::CONTENT_LENGTH, "18")
        .body(b"method not allowed".to_vec())
        .expect("static protocol response is valid")
}

fn range_not_satisfiable(total: Option<u64>) -> Response<Vec<u8>> {
    let mut builder = Response::builder()
        .status(StatusCode::RANGE_NOT_SATISFIABLE)
        .header(header::CACHE_CONTROL, "no-store")
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
        .header(header::CONTENT_SECURITY_POLICY, DOCUMENT_CSP)
        .header(header::CONTENT_LENGTH, "21");
    if let Some(total) = total {
        builder = builder.header(header::CONTENT_RANGE, format!("bytes */{total}"));
    }
    builder
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

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use base64::Engine;
    use bytes::Bytes;
    use futures_util::stream;
    use tauri::http::{header, HeaderMap, HeaderValue, Method, Request, StatusCode};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use url::Url;

    use super::{
        handle_remote_media_protocol, handle_remote_media_protocol_with_fetcher,
        handle_remote_media_protocol_with_fetcher_and_sink, with_remote_media_slot,
        DiagnosticRateLimiter, DiagnosticSink, RemoteFetcher, RemoteMediaDiagnostic,
        UpstreamResponse, MAX_CONCURRENT_REMOTE_MEDIA, MAX_ENCODED_URL_BYTES,
        MAX_PROTOCOL_BODY_BYTES,
    };

    static LIVE_PROTOCOL_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    type FetchFuture = Pin<Box<dyn Future<Output = Result<UpstreamResponse, ()>> + Send + 'static>>;

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct RecordedRequest {
        url: String,
        range: Option<String>,
    }

    #[derive(Clone, Default)]
    struct ScriptedFetcher {
        requests: Arc<Mutex<Vec<RecordedRequest>>>,
        responses: Arc<Mutex<VecDeque<Result<UpstreamResponse, ()>>>>,
    }

    impl ScriptedFetcher {
        fn with_responses(responses: Vec<UpstreamResponse>) -> Self {
            Self {
                requests: Arc::default(),
                responses: Arc::new(Mutex::new(
                    responses.into_iter().map(Ok).collect::<VecDeque<_>>(),
                )),
            }
        }

        fn requests(&self) -> Vec<RecordedRequest> {
            self.requests.lock().unwrap().clone()
        }
    }

    impl RemoteFetcher for ScriptedFetcher {
        fn send(&self, url: Url, range: Option<String>) -> FetchFuture {
            self.requests.lock().unwrap().push(RecordedRequest {
                url: url.into(),
                range,
            });
            let response = self
                .responses
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or(Err(()));
            Box::pin(async move { response })
        }
    }

    #[derive(Clone, Default)]
    struct CapturingDiagnosticSink(Arc<Mutex<Vec<String>>>);

    impl CapturingDiagnosticSink {
        fn messages(&self) -> Vec<String> {
            self.0.lock().unwrap().clone()
        }
    }

    impl DiagnosticSink for CapturingDiagnosticSink {
        fn emit(&self, diagnostic: RemoteMediaDiagnostic) {
            self.0.lock().unwrap().push(diagnostic.to_string());
        }
    }

    fn upstream(
        status: StatusCode,
        headers: &[(&'static str, String)],
        chunks: Vec<Vec<u8>>,
    ) -> UpstreamResponse {
        let mut map = HeaderMap::new();
        for (name, value) in headers {
            map.append(
                header::HeaderName::from_static(name),
                HeaderValue::from_str(value).unwrap(),
            );
        }
        UpstreamResponse {
            status,
            headers: map,
            body: Box::pin(stream::iter(
                chunks.into_iter().map(|chunk| Ok(Bytes::from(chunk))),
            )),
        }
    }

    fn jpeg() -> Vec<u8> {
        vec![0xff, 0xd8, 0xff, 0xe0, 0, 1]
    }

    fn mp4_prefix() -> Vec<u8> {
        b"\0\0\0\x18ftypisom\0\0\0\0".to_vec()
    }

    fn encoded_url(url: &str) -> String {
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(url)
    }

    fn request(method: Method, upstream_url: &str) -> Request<Vec<u8>> {
        Request::builder()
            .method(method)
            .uri(format!(
                "remote-media://localhost/media/{}",
                encoded_url(upstream_url)
            ))
            .body(Vec::new())
            .unwrap()
    }

    fn request_uri(method: Method, uri: impl AsRef<str>) -> Request<Vec<u8>> {
        Request::builder()
            .method(method)
            .uri(uri.as_ref())
            .body(Vec::new())
            .unwrap()
    }

    fn content_headers(content_type: &str, content_length: usize) -> Vec<(&'static str, String)> {
        vec![
            ("content-type", content_type.to_owned()),
            ("content-length", content_length.to_string()),
        ]
    }

    async fn response_with_diagnostics(
        fetcher: ScriptedFetcher,
        request: Request<Vec<u8>>,
    ) -> (tauri::http::Response<Vec<u8>>, Vec<String>) {
        let diagnostics = CapturingDiagnosticSink::default();
        let response = handle_remote_media_protocol_with_fetcher_and_sink(
            fetcher,
            diagnostics.clone(),
            "main",
            request,
        )
        .await;
        (response, diagnostics.messages())
    }

    fn assert_diagnostics_are_secret_safe(messages: &[String]) {
        for sentinel in [
            "TOP-SECRET",
            "cdninstagram.com/private.jpg",
            "proxy-user:proxy-password",
            "private response body text",
        ] {
            assert!(
                messages.iter().all(|message| !message.contains(sentinel)),
                "diagnostic leaked sentinel {sentinel:?}: {messages:?}"
            );
        }
    }

    #[test]
    fn diagnostic_messages_are_bounded_static_categories() {
        let diagnostics = [
            RemoteMediaDiagnostic::Network,
            RemoteMediaDiagnostic::UpstreamStatus(403),
            RemoteMediaDiagnostic::Validation,
            RemoteMediaDiagnostic::RangeRejected(None),
            RemoteMediaDiagnostic::RangeRejected(Some(u64::MAX)),
            RemoteMediaDiagnostic::ResponseTooLarge(None),
            RemoteMediaDiagnostic::ResponseTooLarge(Some(u64::MAX)),
        ];
        let rendered = diagnostics.map(|diagnostic| diagnostic.to_string());

        assert_eq!(rendered[0], "remote media fetch failed: network");
        assert_eq!(rendered[1], "remote media upstream status: 403");
        assert_eq!(rendered[2], "remote media response rejected: validation");
        assert_eq!(rendered[3], "remote media range rejected");
        assert_eq!(
            rendered[4],
            "remote media range rejected: length 18446744073709551615"
        );
        assert_eq!(rendered[5], "remote media response too large");
        assert_eq!(
            rendered[6],
            "remote media response too large: length 18446744073709551615"
        );
        assert!(rendered.iter().all(|message| message.len() <= 64));
    }

    #[test]
    fn diagnostic_rate_limiter_suppresses_by_category_and_reports_a_bounded_count() {
        let cooldown = Duration::from_secs(30);
        let start = Instant::now();
        let mut limiter = DiagnosticRateLimiter::new(cooldown);

        assert_eq!(
            limiter
                .record(RemoteMediaDiagnostic::RangeRejected(Some(10)), start)
                .unwrap()
                .to_string(),
            "remote media range rejected: length 10"
        );
        assert!(limiter
            .record(
                RemoteMediaDiagnostic::RangeRejected(Some(999)),
                start + Duration::from_secs(1)
            )
            .is_none());
        assert_eq!(
            limiter
                .record(
                    RemoteMediaDiagnostic::RangeRejected(Some(20)),
                    start + cooldown
                )
                .unwrap()
                .to_string(),
            "remote media range rejected: length 20; suppressed 1 similar event"
        );

        assert!(limiter
            .record(RemoteMediaDiagnostic::ResponseTooLarge(Some(1)), start)
            .is_some());
        assert!(limiter
            .record(
                RemoteMediaDiagnostic::ResponseTooLarge(Some(u64::MAX)),
                start + Duration::from_secs(1)
            )
            .is_none());
    }

    #[tokio::test]
    async fn diagnostic_handler_emits_network_failure_without_secrets() {
        let (response, diagnostics) = response_with_diagnostics(
            ScriptedFetcher::default(),
            request(
                Method::GET,
                "https://cdninstagram.com/private.jpg?token=TOP-SECRET",
            ),
        )
        .await;

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(diagnostics, ["remote media fetch failed: network"]);
        assert_diagnostics_are_secret_safe(&diagnostics);
    }

    #[tokio::test]
    async fn diagnostic_handler_emits_upstream_status_without_headers_or_body() {
        let fetcher = ScriptedFetcher::with_responses(vec![upstream(
            StatusCode::FORBIDDEN,
            &[(
                "x-upstream-debug",
                concat!(
                    "http://proxy-user",
                    ":",
                    "proxy-password",
                    "@127.0.0.1/private"
                )
                .into(),
            )],
            vec![b"private response body text TOP-SECRET".to_vec()],
        )]);

        let (response, diagnostics) = response_with_diagnostics(
            fetcher,
            request(Method::GET, "https://cdninstagram.com/private.jpg"),
        )
        .await;

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(diagnostics, ["remote media upstream status: 403"]);
        assert_diagnostics_are_secret_safe(&diagnostics);
    }

    #[tokio::test]
    async fn diagnostic_handler_emits_validation_for_rejected_content() {
        let cases = vec![
            upstream(
                StatusCode::OK,
                &[("content-length", "TOP-SECRET".into())],
                vec![jpeg()],
            ),
            upstream(
                StatusCode::OK,
                &content_headers("video/mp4", jpeg().len()),
                vec![jpeg()],
            ),
            upstream(
                StatusCode::OK,
                &content_headers("image/jpeg", "private response body text".len()),
                vec![b"private response body text".to_vec()],
            ),
        ];

        for response in cases {
            let (response, diagnostics) = response_with_diagnostics(
                ScriptedFetcher::with_responses(vec![response]),
                request(Method::GET, "https://cdninstagram.com/private.jpg"),
            )
            .await;
            assert_eq!(response.status(), StatusCode::NOT_FOUND);
            assert_eq!(diagnostics, ["remote media response rejected: validation"]);
            assert_diagnostics_are_secret_safe(&diagnostics);
        }
    }

    #[tokio::test]
    async fn diagnostic_handler_emits_range_rejection_for_malformed_and_unsatisfiable_ranges() {
        let mut malformed = request(Method::GET, "https://cdninstagram.com/private.jpg");
        malformed
            .headers_mut()
            .insert(header::RANGE, HeaderValue::from_static("bytes=TOP-SECRET"));
        let (response, diagnostics) =
            response_with_diagnostics(ScriptedFetcher::default(), malformed).await;
        assert_eq!(response.status(), StatusCode::RANGE_NOT_SATISFIABLE);
        assert_eq!(diagnostics, ["remote media range rejected"]);
        assert_diagnostics_are_secret_safe(&diagnostics);

        let prefix = jpeg();
        let fetcher = ScriptedFetcher::with_responses(vec![upstream(
            StatusCode::PARTIAL_CONTENT,
            &[
                ("content-type", "image/jpeg".into()),
                ("content-length", prefix.len().to_string()),
                ("content-range", format!("bytes 0-{}/100", prefix.len() - 1)),
            ],
            vec![prefix],
        )]);
        let mut unsatisfiable = request(Method::GET, "https://cdninstagram.com/private.jpg");
        unsatisfiable
            .headers_mut()
            .insert(header::RANGE, HeaderValue::from_static("bytes=1000-2000"));
        let (response, diagnostics) = response_with_diagnostics(fetcher, unsatisfiable).await;
        assert_eq!(response.status(), StatusCode::RANGE_NOT_SATISFIABLE);
        assert_eq!(diagnostics, ["remote media range rejected"]);
        assert_diagnostics_are_secret_safe(&diagnostics);
    }

    #[tokio::test]
    async fn diagnostic_handler_classifies_oversized_full_response_separately() {
        let oversized = MAX_PROTOCOL_BODY_BYTES + 1;
        let fetcher = ScriptedFetcher::with_responses(vec![upstream(
            StatusCode::OK,
            &[("content-length", oversized.to_string())],
            vec![],
        )]);
        let (response, diagnostics) = response_with_diagnostics(
            fetcher,
            request(Method::GET, "https://cdninstagram.com/private.jpg"),
        )
        .await;

        assert_eq!(response.status(), StatusCode::RANGE_NOT_SATISFIABLE);
        assert_eq!(
            diagnostics,
            [format!(
                "remote media response too large: length {oversized}"
            )]
        );
        assert_diagnostics_are_secret_safe(&diagnostics);
    }

    #[tokio::test]
    async fn diagnostic_handler_emits_validation_for_decoded_invalid_targets() {
        let invalid_targets = [
            "https://evil.example/private.jpg?token=TOP-SECRET",
            concat!(
                "https://user",
                ":",
                "TOP-SECRET",
                "@cdninstagram.com/private.jpg"
            ),
            "https://cdninstagram.com:444/private.jpg?token=TOP-SECRET",
            "https://cdninstagram.com/private.jpg#TOP-SECRET",
        ];

        for target in invalid_targets {
            let fetcher = ScriptedFetcher::default();
            let (response, diagnostics) =
                response_with_diagnostics(fetcher.clone(), request(Method::GET, target)).await;
            assert_eq!(response.status(), StatusCode::NOT_FOUND);
            assert_eq!(diagnostics, ["remote media response rejected: validation"]);
            assert!(fetcher.requests().is_empty());
            assert_diagnostics_are_secret_safe(&diagnostics);
        }
    }

    #[tokio::test]
    async fn serves_a_valid_jpeg_get_with_safe_headers() {
        let body = jpeg();
        let fetcher = ScriptedFetcher::with_responses(vec![upstream(
            StatusCode::OK,
            &content_headers("image/jpeg", body.len()),
            vec![body.clone()],
        )]);

        let response = handle_remote_media_protocol_with_fetcher(
            fetcher.clone(),
            "main",
            request(
                Method::GET,
                "https://scontent.cdninstagram.com/photo.jpg?sig=ok",
            ),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.body(), &body);
        assert_eq!(response.headers()[header::CONTENT_TYPE], "image/jpeg");
        assert_eq!(
            response.headers()[header::CONTENT_LENGTH],
            body.len().to_string()
        );
        assert_eq!(response.headers()[header::ACCEPT_RANGES], "bytes");
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        assert_eq!(
            response.headers()[header::X_CONTENT_TYPE_OPTIONS],
            "nosniff"
        );
        assert_eq!(
            response.headers()[header::CONTENT_SECURITY_POLICY],
            "default-src 'none'; sandbox"
        );
        assert_eq!(fetcher.requests().len(), 1);
    }

    #[tokio::test]
    async fn rejects_non_main_methods_noncanonical_uris_and_untrusted_urls_before_fetch() {
        let valid = "https://cdninstagram.com/photo.jpg";
        let cases = vec![
            (
                "secondary",
                request(Method::GET, valid),
                StatusCode::NOT_FOUND,
            ),
            (
                "main",
                request(Method::POST, valid),
                StatusCode::METHOD_NOT_ALLOWED,
            ),
            (
                "main",
                request_uri(
                    Method::GET,
                    format!("remote-media://evil.example/media/{}", encoded_url(valid)),
                ),
                StatusCode::NOT_FOUND,
            ),
            (
                "main",
                request_uri(
                    Method::GET,
                    format!(
                        "remote-media://localhost/media/{}?leak=1",
                        encoded_url(valid)
                    ),
                ),
                StatusCode::NOT_FOUND,
            ),
            (
                "main",
                request_uri(
                    Method::GET,
                    format!(
                        "remote-media://localhost/media/{}/extra",
                        encoded_url(valid)
                    ),
                ),
                StatusCode::NOT_FOUND,
            ),
            (
                "main",
                request_uri(Method::GET, "remote-media://localhost/media/not*base64"),
                StatusCode::NOT_FOUND,
            ),
            (
                "main",
                request_uri(
                    Method::GET,
                    format!("remote-media://localhost/media/{}=", encoded_url(valid)),
                ),
                StatusCode::NOT_FOUND,
            ),
            (
                "main",
                request_uri(
                    Method::GET,
                    format!(
                        "remote-media://localhost/media/{}",
                        "A".repeat(MAX_ENCODED_URL_BYTES + 1)
                    ),
                ),
                StatusCode::NOT_FOUND,
            ),
            (
                "main",
                request(Method::GET, "http://cdninstagram.com/photo.jpg"),
                StatusCode::NOT_FOUND,
            ),
            (
                "main",
                request(Method::GET, "https://evil.example/photo.jpg"),
                StatusCode::NOT_FOUND,
            ),
            (
                "main",
                request(Method::GET, "https://127.0.0.1/photo.jpg"),
                StatusCode::NOT_FOUND,
            ),
            (
                "main",
                request(
                    Method::GET,
                    "https://user:secret@cdninstagram.com/photo.jpg",
                ),
                StatusCode::NOT_FOUND,
            ),
        ];

        for (label, request, expected) in cases {
            let fetcher = ScriptedFetcher::default();
            let response =
                handle_remote_media_protocol_with_fetcher(fetcher.clone(), label, request).await;
            assert_eq!(response.status(), expected);
            assert!(fetcher.requests().is_empty());
            assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        }
    }

    #[tokio::test]
    async fn validates_relative_and_every_redirect_target_with_a_five_hop_limit() {
        let first = upstream(
            StatusCode::FOUND,
            &[("location", "/next.jpg?token=next".into())],
            vec![],
        );
        let body = jpeg();
        let fetcher = ScriptedFetcher::with_responses(vec![
            first,
            upstream(
                StatusCode::OK,
                &content_headers("image/jpeg", body.len()),
                vec![body.clone()],
            ),
        ]);
        let response = handle_remote_media_protocol_with_fetcher(
            fetcher.clone(),
            "main",
            request(Method::GET, "https://media.cdninstagram.com/start.jpg"),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            fetcher.requests()[1].url,
            "https://media.cdninstagram.com/next.jpg?token=next"
        );

        let malicious = ScriptedFetcher::with_responses(vec![upstream(
            StatusCode::FOUND,
            &[("location", "https://evil.example/stolen".into())],
            vec![],
        )]);
        let response = handle_remote_media_protocol_with_fetcher(
            malicious.clone(),
            "main",
            request(Method::GET, "https://cdninstagram.com/start.jpg"),
        )
        .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(malicious.requests().len(), 1);

        let redirects = (0..=5)
            .map(|index| {
                upstream(
                    StatusCode::FOUND,
                    &[("location", format!("/hop-{index}"))],
                    vec![],
                )
            })
            .collect();
        let too_many = ScriptedFetcher::with_responses(redirects);
        let response = handle_remote_media_protocol_with_fetcher(
            too_many.clone(),
            "main",
            request(Method::GET, "https://cdninstagram.com/start.jpg"),
        )
        .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(too_many.requests().len(), 6);
    }

    #[tokio::test]
    async fn rejects_multiple_and_malformed_ranges_before_fetch() {
        let invalid_values = [
            "items=0-1",
            "bytes=",
            "bytes=2-1",
            "bytes=0-1,4-5",
            "bytes=-0",
        ];
        for value in invalid_values {
            let fetcher = ScriptedFetcher::default();
            let mut req = request(Method::GET, "https://cdninstagram.com/video.mp4");
            req.headers_mut()
                .insert(header::RANGE, HeaderValue::from_str(value).unwrap());
            let response =
                handle_remote_media_protocol_with_fetcher(fetcher.clone(), "main", req).await;
            assert_eq!(response.status(), StatusCode::RANGE_NOT_SATISFIABLE);
            assert!(fetcher.requests().is_empty());
        }

        let fetcher = ScriptedFetcher::default();
        let mut req = request(Method::GET, "https://cdninstagram.com/video.mp4");
        req.headers_mut()
            .append(header::RANGE, HeaderValue::from_static("bytes=0-1"));
        req.headers_mut()
            .append(header::RANGE, HeaderValue::from_static("bytes=2-3"));
        let response =
            handle_remote_media_protocol_with_fetcher(fetcher.clone(), "main", req).await;
        assert_eq!(response.status(), StatusCode::RANGE_NOT_SATISFIABLE);
        assert!(fetcher.requests().is_empty());
    }

    #[tokio::test]
    async fn serves_nonzero_mp4_ranges_after_a_bounded_magic_probe() {
        let prefix = mp4_prefix();
        let ranged = vec![7; 100];
        let fetcher = ScriptedFetcher::with_responses(vec![
            upstream(
                StatusCode::PARTIAL_CONTENT,
                &[
                    ("content-type", "video/mp4".into()),
                    ("content-length", prefix.len().to_string()),
                    (
                        "content-range",
                        format!("bytes 0-{}/1000", prefix.len() - 1),
                    ),
                ],
                vec![prefix],
            ),
            upstream(
                StatusCode::PARTIAL_CONTENT,
                &[
                    ("content-type", "video/mp4".into()),
                    ("content-length", ranged.len().to_string()),
                    ("content-range", "bytes 100-199/1000".into()),
                ],
                vec![ranged.clone()],
            ),
        ]);
        let mut req = request(Method::GET, "https://video.cdninstagram.com/video.mp4");
        req.headers_mut()
            .insert(header::RANGE, HeaderValue::from_static("bytes=100-199"));

        let response =
            handle_remote_media_protocol_with_fetcher(fetcher.clone(), "main", req).await;

        assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(response.body(), &ranged);
        assert_eq!(response.headers()[header::CONTENT_TYPE], "video/mp4");
        assert_eq!(
            response.headers()[header::CONTENT_RANGE],
            "bytes 100-199/1000"
        );
        assert_eq!(response.headers()[header::CONTENT_LENGTH], "100");
        assert_eq!(
            fetcher.requests(),
            vec![
                RecordedRequest {
                    url: "https://video.cdninstagram.com/video.mp4".into(),
                    range: Some("bytes=0-511".into()),
                },
                RecordedRequest {
                    url: "https://video.cdninstagram.com/video.mp4".into(),
                    range: Some("bytes=100-199".into()),
                },
            ]
        );
    }

    #[tokio::test]
    async fn returns_a_safe_full_200_when_the_origin_ignores_range() {
        let body = mp4_prefix();
        let fetcher = ScriptedFetcher::with_responses(vec![
            upstream(
                StatusCode::PARTIAL_CONTENT,
                &[
                    ("content-type", "video/mp4".into()),
                    ("content-length", body.len().to_string()),
                    (
                        "content-range",
                        format!("bytes 0-{}/{}", body.len() - 1, body.len()),
                    ),
                ],
                vec![body.clone()],
            ),
            upstream(
                StatusCode::OK,
                &content_headers("video/mp4", body.len()),
                vec![body.clone()],
            ),
        ]);
        let mut req = request(Method::GET, "https://video.cdninstagram.com/video.mp4");
        req.headers_mut()
            .insert(header::RANGE, HeaderValue::from_static("bytes=4-7"));

        let response = handle_remote_media_protocol_with_fetcher(fetcher, "main", req).await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.body(), &body);
        assert_eq!(
            response.headers()[header::CONTENT_LENGTH],
            body.len().to_string()
        );
        assert!(response.headers().get(header::CONTENT_RANGE).is_none());
    }

    #[tokio::test]
    async fn preserves_a_valid_upstream_416_after_the_magic_probe() {
        let prefix = mp4_prefix();
        let fetcher = ScriptedFetcher::with_responses(vec![
            upstream(
                StatusCode::PARTIAL_CONTENT,
                &[
                    ("content-type", "video/mp4".into()),
                    ("content-length", prefix.len().to_string()),
                    (
                        "content-range",
                        format!("bytes 0-{}/1000", prefix.len() - 1),
                    ),
                ],
                vec![prefix],
            ),
            upstream(
                StatusCode::RANGE_NOT_SATISFIABLE,
                &[("content-range", "bytes */1000".into())],
                vec![],
            ),
        ]);
        let mut req = request(Method::GET, "https://video.cdninstagram.com/video.mp4");
        req.headers_mut()
            .insert(header::RANGE, HeaderValue::from_static("bytes=100-199"));

        let response = handle_remote_media_protocol_with_fetcher(fetcher, "main", req).await;

        assert_eq!(response.status(), StatusCode::RANGE_NOT_SATISFIABLE);
        assert_eq!(response.headers()[header::CONTENT_RANGE], "bytes */1000");
        assert_eq!(response.body(), b"range not satisfiable");
    }

    #[tokio::test]
    async fn head_uses_a_bounded_magic_probe_and_returns_no_body() {
        let prefix = mp4_prefix();
        let fetcher = ScriptedFetcher::with_responses(vec![upstream(
            StatusCode::PARTIAL_CONTENT,
            &[
                ("content-type", "video/mp4".into()),
                ("content-length", prefix.len().to_string()),
                (
                    "content-range",
                    format!("bytes 0-{}/4096", prefix.len() - 1),
                ),
            ],
            vec![prefix],
        )]);

        let response = handle_remote_media_protocol_with_fetcher(
            fetcher.clone(),
            "main",
            request(Method::HEAD, "https://cdninstagram.com/video.mp4"),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        assert!(response.body().is_empty());
        assert_eq!(response.headers()[header::CONTENT_TYPE], "video/mp4");
        assert_eq!(response.headers()[header::CONTENT_LENGTH], "4096");
        assert_eq!(fetcher.requests()[0].range.as_deref(), Some("bytes=0-511"));
    }

    #[tokio::test]
    async fn head_ignores_malformed_and_multiple_request_ranges() {
        let range_cases: &[&[&str]] = &[&["items=0-1"], &["bytes=0-1", "bytes=2-3"]];

        for range_values in range_cases {
            let prefix = mp4_prefix();
            let fetcher = ScriptedFetcher::with_responses(vec![upstream(
                StatusCode::PARTIAL_CONTENT,
                &[
                    ("content-type", "video/mp4".into()),
                    ("content-length", prefix.len().to_string()),
                    (
                        "content-range",
                        format!("bytes 0-{}/4096", prefix.len() - 1),
                    ),
                ],
                vec![prefix],
            )]);
            let mut req = request(Method::HEAD, "https://cdninstagram.com/video.mp4");
            for range in *range_values {
                req.headers_mut().append(
                    header::RANGE,
                    HeaderValue::from_str(range).expect("static test range is valid header text"),
                );
            }

            let response =
                handle_remote_media_protocol_with_fetcher(fetcher.clone(), "main", req).await;

            assert_eq!(response.status(), StatusCode::OK);
            assert!(response.body().is_empty());
            assert_eq!(response.headers()[header::CONTENT_TYPE], "video/mp4");
            assert_eq!(response.headers()[header::CONTENT_LENGTH], "4096");
            assert_eq!(fetcher.requests().len(), 1);
            assert_eq!(fetcher.requests()[0].range.as_deref(), Some("bytes=0-511"));
        }
    }

    #[tokio::test]
    async fn rejects_empty_unknown_mismatched_oversized_and_truncated_media() {
        let cases = vec![
            (
                upstream(StatusCode::OK, &content_headers("image/jpeg", 0), vec![]),
                StatusCode::NOT_FOUND,
            ),
            (
                upstream(
                    StatusCode::OK,
                    &content_headers("image/jpeg", 6),
                    vec![b"unsafe".to_vec()],
                ),
                StatusCode::NOT_FOUND,
            ),
            (
                upstream(
                    StatusCode::OK,
                    &content_headers("video/mp4", jpeg().len()),
                    vec![jpeg()],
                ),
                StatusCode::NOT_FOUND,
            ),
            (
                upstream(
                    StatusCode::OK,
                    &content_headers("image/jpeg", MAX_PROTOCOL_BODY_BYTES as usize + 1),
                    vec![],
                ),
                StatusCode::RANGE_NOT_SATISFIABLE,
            ),
            (
                upstream(
                    StatusCode::OK,
                    &content_headers("image/jpeg", jpeg().len() + 10),
                    vec![jpeg()],
                ),
                StatusCode::NOT_FOUND,
            ),
        ];

        for (scripted, expected_status) in cases {
            let fetcher = ScriptedFetcher::with_responses(vec![scripted]);
            let response = handle_remote_media_protocol_with_fetcher(
                fetcher,
                "main",
                request(Method::GET, "https://cdninstagram.com/media.jpg"),
            )
            .await;
            assert_eq!(response.status(), expected_status);
            assert!(response.body().len() < 64);
        }
    }

    #[tokio::test]
    async fn returns_sanitized_failures_without_upstream_urls_or_secrets() {
        let secret = "TOP-SECRET-CREDENTIAL";
        let upstream_url = format!("https://cdninstagram.com/media.jpg?token={secret}");
        let fetcher = ScriptedFetcher::with_responses(vec![upstream(
            StatusCode::INTERNAL_SERVER_ERROR,
            &[("x-upstream-debug", upstream_url.clone())],
            vec![upstream_url.clone().into_bytes()],
        )]);

        let response = handle_remote_media_protocol_with_fetcher(
            fetcher,
            "main",
            request(Method::GET, &upstream_url),
        )
        .await;
        let rendered = format!("{:?}{:?}", response.headers(), response.body());
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert!(!rendered.contains(secret));
        assert!(!rendered.contains("cdninstagram.com"));
    }

    #[tokio::test]
    async fn remote_protocol_fetch_traverses_the_configured_http_proxy() {
        let _test_lock = LIVE_PROTOCOL_TEST_LOCK.lock().await;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = listener.local_addr().unwrap();
        let observed = Arc::new(Mutex::new(String::new()));
        let captured = Arc::clone(&observed);
        let proxy = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut chunk = [0; 1024];
            while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                let read = socket.read(&mut chunk).await.unwrap();
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&chunk[..read]);
            }
            *captured.lock().unwrap() = String::from_utf8_lossy(&request).into_owned();
            socket
                .write_all(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\n\r\n")
                .await
                .unwrap();
        });
        let client = crate::network::build_cdn_client(Some(&format!("http://{proxy_addr}")))
            .expect("proxy-aware CDN client");

        let response = tokio::time::timeout(
            Duration::from_secs(5),
            handle_remote_media_protocol(
                client,
                "main",
                request(Method::GET, "https://cdninstagram.com/proxied.jpg"),
            ),
        )
        .await
        .expect("protocol request timed out");
        proxy.await.unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let captured = observed.lock().unwrap();
        assert!(
            captured.starts_with("CONNECT cdninstagram.com:443 HTTP/1.1\r\n"),
            "unexpected proxy request: {captured:?}"
        );
    }

    #[tokio::test]
    async fn bounds_concurrent_remote_media_buffering_and_releases_slots() {
        let _test_lock = LIVE_PROTOCOL_TEST_LOCK.lock().await;
        let active = Arc::new(AtomicUsize::new(0));
        let maximum = Arc::new(AtomicUsize::new(0));
        let gate = Arc::new(tokio::sync::Semaphore::new(0));
        let mut tasks = Vec::new();

        for _ in 0..(MAX_CONCURRENT_REMOTE_MEDIA + 2) {
            let active = Arc::clone(&active);
            let maximum = Arc::clone(&maximum);
            let gate = Arc::clone(&gate);
            tasks.push(tokio::spawn(async move {
                with_remote_media_slot(async move {
                    let now = active.fetch_add(1, Ordering::SeqCst) + 1;
                    maximum.fetch_max(now, Ordering::SeqCst);
                    gate.acquire().await.unwrap().forget();
                    active.fetch_sub(1, Ordering::SeqCst);
                })
                .await;
            }));
        }

        tokio::time::timeout(Duration::from_secs(1), async {
            while active.load(Ordering::SeqCst) < MAX_CONCURRENT_REMOTE_MEDIA {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("concurrency limit was never reached");
        assert_eq!(maximum.load(Ordering::SeqCst), MAX_CONCURRENT_REMOTE_MEDIA);
        assert_eq!(active.load(Ordering::SeqCst), MAX_CONCURRENT_REMOTE_MEDIA);

        gate.add_permits(MAX_CONCURRENT_REMOTE_MEDIA + 2);
        for task in tasks {
            task.await.unwrap();
        }
        assert_eq!(active.load(Ordering::SeqCst), 0);
        assert_eq!(maximum.load(Ordering::SeqCst), MAX_CONCURRENT_REMOTE_MEDIA);
    }
}
