//! Payload decoding functionality.

use byteorder::{ByteOrder, LittleEndian};
use bytes::Bytes;

use super::payload_encoder::*;
use super::{Entry, Payload, RequestPayload, ResponsePayload};

/// Error types for payload decoding.
#[derive(Debug, thiserror::Error)]
pub enum PayloadError {
    #[error("malformed or nil payload")]
    MalformedOrNilPayload,
    #[error("corrupted queries section")]
    CorruptedQueriesSection,
    #[error("corrupted request headers section")]
    CorruptedRequestHeadersSection,
    #[error("corrupted status code section")]
    CorruptedStatusCodeSection,
    #[error("corrupted response headers section")]
    CorruptedResponseHeadersSection,
    #[error("corrupted response body section")]
    CorruptedResponseBodySection,
}

impl Entry {
    /// Gets the full payload (queries, request headers, response headers, body, code).
    pub fn payload(&self) -> Result<Payload, PayloadError> {
        let req_payload = self.request_payload()?;
        let resp_payload = self.response_payload()?;

        Ok(Payload {
            queries: req_payload.queries,
            req_headers: req_payload.headers,
            rsp_headers: resp_payload.headers,
            body: resp_payload.body,
            code: resp_payload.code,
        })
    }

    /// Gets the request payload (queries and headers).
    pub fn request_payload(&self) -> Result<RequestPayload, PayloadError> {
        let data = self.get_payload_data()?;

        let queries = self.unpack_queries(&data)?;
        let headers = self.unpack_request_headers(&data)?;

        Ok(RequestPayload { queries, headers })
    }

    /// Gets the response payload (headers, body, code).
    pub fn response_payload(&self) -> Result<ResponsePayload, PayloadError> {
        let data = self.get_payload_data()?;

        let code = self.unpack_status_code(&data)?;
        let headers = self.unpack_response_headers(&data)?;
        let body = self.unpack_response_body(&data)?;

        Ok(ResponsePayload {
            headers,
            body: body.to_vec(), // Convert Bytes to Vec<u8> for compatibility
            code,
        })
    }

    /// Unpacks queries from the payload.
    fn unpack_queries(&self, data: &Bytes) -> Result<Vec<(Vec<u8>, Vec<u8>)>, PayloadError> {
        let offset_from = LittleEndian::read_u32(&data[OFF_QUERY..OFF_QUERY + OFF_WEIGHT]) as usize;
        let offset_to =
            LittleEndian::read_u32(&data[OFF_REQ_HDRS..OFF_REQ_HDRS + OFF_WEIGHT]) as usize;

        let mut queries = Vec::new();
        let mut pos = offset_from;

        while pos < offset_to {
            if pos + OFF_WEIGHT > data.len() {
                return Err(PayloadError::CorruptedQueriesSection);
            }

            let k_len = LittleEndian::read_u32(&data[pos..pos + OFF_WEIGHT]) as usize;
            pos += OFF_WEIGHT;
            if pos + k_len > data.len() {
                return Err(PayloadError::CorruptedQueriesSection);
            }
            let k = data[pos..pos + k_len].to_vec();
            pos += k_len;

            let v_len = LittleEndian::read_u32(&data[pos..pos + OFF_WEIGHT]) as usize;
            pos += OFF_WEIGHT;
            if pos + v_len > data.len() {
                return Err(PayloadError::CorruptedQueriesSection);
            }
            let v = data[pos..pos + v_len].to_vec();
            pos += v_len;

            queries.push((k, v));
        }

        Ok(queries)
    }

    /// Unpacks request headers from the payload.
    fn unpack_request_headers(&self, data: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>, PayloadError> {
        let offset_from =
            LittleEndian::read_u32(&data[OFF_REQ_HDRS..OFF_REQ_HDRS + OFF_WEIGHT]) as usize;
        let offset_to = LittleEndian::read_u32(&data[OFF_STATUS..OFF_STATUS + OFF_WEIGHT]) as usize;

        let mut headers = Vec::new();
        let mut pos = offset_from;

        while pos < offset_to {
            if pos + OFF_WEIGHT > data.len() {
                return Err(PayloadError::CorruptedRequestHeadersSection);
            }

            let k_len = LittleEndian::read_u32(&data[pos..pos + OFF_WEIGHT]) as usize;
            pos += OFF_WEIGHT;
            if pos + k_len > data.len() {
                return Err(PayloadError::CorruptedRequestHeadersSection);
            }
            let k = data[pos..pos + k_len].to_vec();
            pos += k_len;

            let v_len = LittleEndian::read_u32(&data[pos..pos + OFF_WEIGHT]) as usize;
            pos += OFF_WEIGHT;
            if pos + v_len > data.len() {
                return Err(PayloadError::CorruptedRequestHeadersSection);
            }
            let v = data[pos..pos + v_len].to_vec();
            pos += v_len;

            headers.push((k, v));
        }

        Ok(headers)
    }

    /// Unpacks status code from the payload.
    fn unpack_status_code(&self, data: &Bytes) -> Result<u16, PayloadError> {
        let offset_from =
            LittleEndian::read_u32(&data[OFF_STATUS..OFF_STATUS + OFF_WEIGHT]) as usize;
        let offset_to = offset_from + OFF_WEIGHT;

        if offset_to > data.len() {
            return Err(PayloadError::CorruptedStatusCodeSection);
        }

        let code = LittleEndian::read_u32(&data[offset_from..offset_to]) as u16;
        Ok(code)
    }

    /// Unpacks response headers from the payload.
    fn unpack_response_headers(
        &self,
        data: &[u8],
    ) -> Result<Vec<(Vec<u8>, Vec<u8>)>, PayloadError> {
        let offset_from =
            LittleEndian::read_u32(&data[OFF_RESP_HDRS..OFF_RESP_HDRS + OFF_WEIGHT]) as usize;
        let offset_to = LittleEndian::read_u32(&data[OFF_BODY..OFF_BODY + OFF_WEIGHT]) as usize;

        let mut headers = Vec::new();
        let mut pos = offset_from;

        while pos < offset_to {
            if pos + OFF_WEIGHT > data.len() {
                return Err(PayloadError::CorruptedResponseHeadersSection);
            }

            let k_len = LittleEndian::read_u32(&data[pos..pos + OFF_WEIGHT]) as usize;
            pos += OFF_WEIGHT;
            if pos + k_len > data.len() {
                return Err(PayloadError::CorruptedResponseHeadersSection);
            }
            let k = data[pos..pos + k_len].to_vec();
            pos += k_len;

            let v_len = LittleEndian::read_u32(&data[pos..pos + OFF_WEIGHT]) as usize;
            pos += OFF_WEIGHT;
            if pos + v_len > data.len() {
                return Err(PayloadError::CorruptedResponseHeadersSection);
            }
            let v = data[pos..pos + v_len].to_vec();
            pos += v_len;

            headers.push((k, v));
        }

        Ok(headers)
    }

    /// Unpacks response body from the payload.
    fn unpack_response_body(&self, data: &Bytes) -> Result<Bytes, PayloadError> {
        let body_offset = LittleEndian::read_u32(&data[OFF_BODY..OFF_BODY + OFF_WEIGHT]) as usize;

        if body_offset + OFF_WEIGHT > data.len() {
            return Err(PayloadError::CorruptedResponseBodySection);
        }

        let body_len =
            LittleEndian::read_u32(&data[body_offset..body_offset + OFF_WEIGHT]) as usize;
        let offset_from = body_offset + OFF_WEIGHT;
        let offset_to = offset_from + body_len;

        if offset_to > data.len() {
            return Err(PayloadError::CorruptedResponseBodySection);
        }

        // Zero-copy slice from Bytes
        Ok(data.slice(offset_from..offset_to))
    }

    /// Gets the payload data, checking for validity (returns Bytes - zero-copy).
    fn get_payload_data(&self) -> Result<bytes::Bytes, PayloadError> {
        let payload_guard = self.0.payload.load();
        let arc_bytes = match payload_guard.as_ref() {
            Some(arc_bytes) => arc_bytes,
            None => return Err(PayloadError::MalformedOrNilPayload),
        };
        
        if arc_bytes.is_empty() {
            return Err(PayloadError::MalformedOrNilPayload);
        }
        
        if arc_bytes.len() < OFFSETS_MAP_SIZE {
            return Err(PayloadError::MalformedOrNilPayload);
        }
        
        // Clone Bytes from Arc (cheap, just ref count increment)
        Ok(arc_bytes.as_ref().clone())
    }
}
