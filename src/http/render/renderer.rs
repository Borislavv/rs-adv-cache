use axum::{
    http::{HeaderMap, HeaderName, HeaderValue, StatusCode},
    response::Response,
};
use bytes::Bytes;

use crate::model::Entry;

use crate::http::utils::last_updated_at;

/// Writes a response from raw data.
pub fn write_from_raw_response(
    headers: &[(Vec<u8>, Vec<u8>)],
    body: &[u8],
    code: u16,
    updated_at: i64,
) -> Response {
    let mut header_map = HeaderMap::new();

    // Set headers
    for (k, v) in headers {
        if let (Ok(name), Ok(value)) = (
            HeaderName::try_from(k.as_slice()),
            HeaderValue::from_bytes(v),
        ) {
            header_map.insert(name, value);
        }
    }

    if let Some(last_updated) = last_updated_at::set_last_updated_at_value(updated_at) {
        if let (Ok(name), Ok(value)) = (
            HeaderName::try_from(last_updated_at::LAST_UPDATED_AT_KEY.as_bytes()),
            HeaderValue::from_str(&last_updated),
        ) {
            header_map.insert(name, value);
        }
    }

    header_map.insert(
        HeaderName::from_static("content-length"),
        HeaderValue::from_str(&body.len().to_string())
            .unwrap_or_else(|_| HeaderValue::from_static("0")),
    );

    // Build response
    let status = StatusCode::from_u16(code).unwrap_or(StatusCode::OK);

    // Use Bytes::from to avoid unnecessary Vec allocation if body is already Vec<u8>
    // For zero-copy, prefer using body directly as Bytes when possible
    let body_bytes = Bytes::from(body.to_vec());
    Response::builder()
        .status(status)
        .body(body_bytes.into())
        .map(|mut resp| {
            *resp.headers_mut() = header_map;
            resp
        })
        .unwrap_or_else(|_| {
            Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Vec::new().into())
                .unwrap()
        })
}

/// Writes a response from a Response struct.
pub fn write_from_response(resp: &crate::model::Response, last_refreshed_at: i64) -> Response {
    let mut header_map = HeaderMap::new();

    // Set headers
    for (k, v) in &resp.headers {
        if let (Ok(name), Ok(value)) =
            (HeaderName::try_from(k.as_bytes()), HeaderValue::from_str(v))
        {
            header_map.insert(name, value);
        }
    }

    // Set Last-Updated-At header
    if let Some(last_updated) = last_updated_at::set_last_updated_at_value(last_refreshed_at) {
        if let (Ok(name), Ok(value)) = (
            HeaderName::try_from(last_updated_at::LAST_UPDATED_AT_KEY.as_bytes()),
            HeaderValue::from_str(&last_updated),
        ) {
            header_map.insert(name, value);
        }
    }

    header_map.insert(
        HeaderName::from_static("content-length"),
        HeaderValue::from_str(&resp.body.len().to_string())
            .unwrap_or_else(|_| HeaderValue::from_static("0")),
    );

    // Build response
    let status = StatusCode::from_u16(resp.status).unwrap_or(StatusCode::OK);

    // Convert body to Bytes efficiently
    // For small bodies, use copy_from_slice to avoid Vec allocation overhead
    let body_bytes = if resp.body.len() < 1024 {
        Bytes::copy_from_slice(&resp.body)
    } else {
        Bytes::from(resp.body.clone())
    };
    Response::builder()
        .status(status)
        .body(body_bytes.into())
        .map(|mut resp| {
            *resp.headers_mut() = header_map;
            resp
        })
        .unwrap_or_else(|_| {
            Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Vec::new().into())
                .unwrap()
        })
}

/// Writes a response from a cache entry.
pub fn write_from_entry(
    entry: &Entry,
) -> Result<Response, Box<dyn std::error::Error + Send + Sync>> {
    let resp_payload = entry.response_payload()?;

    let headers: Vec<(Vec<u8>, Vec<u8>)> = resp_payload.headers;
    let body = resp_payload.body;
    let code = resp_payload.code;
    let fresh_at = entry.fresh_at();

    Ok(write_from_raw_response(&headers, &body, code, fresh_at))
}
