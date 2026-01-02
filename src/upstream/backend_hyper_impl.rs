//! Hyper-based implementation of upstream request methods.
//! This module contains the actual request/response handling using hyper client.

use anyhow::{Context, Result};
use http_body_util::BodyExt;
use hyper::{Method, Request, Uri};
use bytes::Bytes;
use std::time::Duration;
use tokio::time::timeout;
use http_body_util::{Empty, Full};
use http_body_util::combinators::BoxBody;

use crate::http::client::HyperClient;

/// Makes a GET request to upstream using hyper client.
pub async fn make_get_request(
    client: &HyperClient,
    uri: Uri,
    headers: Vec<(&str, &str)>,
    timeout_duration: Duration,
    forwarded_host: Option<&[u8]>,
) -> anyhow::Result<(u16, hyper::HeaderMap, Bytes)> {
    let uri_str = uri.to_string();
    
    let mut builder = Request::builder()
        .method(Method::GET)
        .uri(uri);
    
    // Set all headers except Host (Host will be set after build() to override URI-based Host)
    for (name, value) in headers {
        if !name.eq_ignore_ascii_case("host") {
            builder = builder.header(name, value);
        }
    }
    
    let empty_with_error: BoxBody<Bytes, hyper::Error> = Empty::<Bytes>::new()
        .map_err(|never: std::convert::Infallible| match never {})
        .boxed();
    
    let mut req = builder.body(empty_with_error)?;
    
    // Set Host header after build() to ensure it's actually sent as HTTP/1.1 header.
    // This bypasses any builder/client normalization that might ignore or modify Host.
    if let Some(host_bytes) = forwarded_host {
        if let Ok(host_value) = hyper::header::HeaderValue::from_bytes(host_bytes) {
            // Explicitly remove any existing Host header (from URI) and insert the forwarded one
            req.headers_mut().remove(hyper::header::HOST);
            req.headers_mut().insert(hyper::header::HOST, host_value);
        }
    }
    
    let response = match timeout(timeout_duration, client.request(req)).await {
        Ok(Ok(resp)) => resp,
        Ok(Err(e)) => {
            tracing::error!(
                uri = %uri_str,
                error = %e,
                error_debug = ?e,
                "Hyper client request failed"
            );
            return Err(anyhow::anyhow!("Hyper client error: {} (URI: {})", e, uri_str))
                .context("Request failed");
        }
        Err(_) => {
            tracing::warn!(
                uri = %uri_str,
                timeout = ?timeout_duration,
                "Request timed out"
            );
            return Err(anyhow::anyhow!("Request timed out after {:?} (URI: {})", timeout_duration, uri_str))
                .context("Request timeout");
        }
    };
    
    let status = response.status().as_u16();
    let headers = response.headers().clone();
    
    let (_, body_stream) = response.into_parts();
    let body_bytes = body_stream
        .collect()
        .await
        .context("Failed to read response body")?
        .to_bytes();
    
    Ok((status, headers, body_bytes))
}

/// Makes a request with custom method and optional body.
pub async fn make_method_request(
    client: &HyperClient,
    method: Method,
    uri: Uri,
    headers: Vec<(&str, &str)>,
    body: Option<Bytes>,
    timeout_duration: Duration,
    forwarded_host: Option<&[u8]>,
) -> Result<(u16, hyper::HeaderMap, Vec<u8>)> {
    let uri_str = uri.to_string();
    
    let mut builder = Request::builder()
        .method(method)
        .uri(uri);
    
    // Set all headers except Host (Host will be set after build() to override URI-based Host)
    for (name, value) in headers {
        if !name.eq_ignore_ascii_case("host") {
            builder = builder.header(name, value);
        }
    }
    
    let req_body: BoxBody<Bytes, hyper::Error> = if let Some(body_bytes) = body {
        Full::new(body_bytes)
            .map_err(|never: std::convert::Infallible| match never {})
            .boxed()
    } else {
        Empty::<Bytes>::new()
            .map_err(|never: std::convert::Infallible| match never {})
            .boxed()
    };
    
    let mut req = builder.body(req_body)?;
    
    // Set Host header after build() to ensure it's actually sent as HTTP/1.1 header.
    // This bypasses any builder/client normalization that might ignore or modify Host.
    if let Some(host_bytes) = forwarded_host {
        if let Ok(host_value) = hyper::header::HeaderValue::from_bytes(host_bytes) {
            // Explicitly remove any existing Host header (from URI) and insert the forwarded one
            req.headers_mut().remove(hyper::header::HOST);
            req.headers_mut().insert(hyper::header::HOST, host_value);
        }
    }
    
    let response = match timeout(timeout_duration, client.request(req)).await {
        Ok(Ok(resp)) => resp,
        Ok(Err(e)) => {
            tracing::error!(
                uri = %uri_str,
                error = %e,
                error_debug = ?e,
                "Hyper client request failed"
            );
            return Err(anyhow::anyhow!("Hyper client error: {} (URI: {})", e, uri_str))
                .context("Request failed");
        }
        Err(_) => {
            tracing::warn!(
                uri = %uri_str,
                timeout = ?timeout_duration,
                "Request timed out"
            );
            return Err(anyhow::anyhow!("Request timed out after {:?} (URI: {})", timeout_duration, uri_str))
                .context("Request timeout");
        }
    };
    
    let status = response.status().as_u16();
    let headers = response.headers().clone();
    
    let (_, body_stream) = response.into_parts();
    let body_bytes = body_stream
        .collect()
        .await
        .context("Failed to read response body")?
        .to_bytes();
    
    Ok((status, headers, body_bytes.to_vec()))
}
