use axum::{
    http::{HeaderMap, HeaderName, HeaderValue, StatusCode},
    response::Response,
};

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

    // Set Last-Updated-At header
    if let Some(last_updated) = last_updated_at::set_last_updated_at_value(updated_at) {
        if let (Ok(name), Ok(value)) = (
            HeaderName::try_from(last_updated_at::LAST_UPDATED_AT_KEY.as_bytes()),
            HeaderValue::from_str(&last_updated),
        ) {
            header_map.insert(name, value);
        }
    }

    // Build response
    let status = StatusCode::from_u16(code).unwrap_or(StatusCode::OK);
    
    Response::builder()
        .status(status)
        .header("content-length", body.len())
        .body(body.to_vec().into())
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
pub fn write_from_response(
    resp: &crate::model::Response,
    last_refreshed_at: i64,
) -> Response {
    let mut header_map = HeaderMap::new();

    // Set headers
    for (k, v) in &resp.headers {
        if let (Ok(name), Ok(value)) = (
            HeaderName::try_from(k.as_bytes()),
            HeaderValue::from_str(v),
        ) {
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

    // Build response
    let status = StatusCode::from_u16(resp.status).unwrap_or(StatusCode::OK);
    
    Response::builder()
        .status(status)
        .header("content-length", resp.body.len())
        .body(resp.body.clone().into())
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
pub fn write_from_entry(entry: &Entry) -> Result<Response, Box<dyn std::error::Error + Send + Sync>> {
    let resp_payload = entry.response_payload()?;
    
    let headers: Vec<(Vec<u8>, Vec<u8>)> = resp_payload.headers;
    let body = resp_payload.body;
    let code = resp_payload.code;
    let fresh_at = entry.fresh_at();

    Ok(write_from_raw_response(&headers, &body, code, fresh_at))
}

