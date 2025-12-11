// Package model provides payload decoding functionality.

use byteorder::{ByteOrder, LittleEndian};
use super::payload_encoder::*;
use super::{Entry, Payload, RequestPayload, ResponsePayload};

/// Error types for payload decoding.
#[derive(Debug, thiserror::Error)]
pub enum PayloadError {
    #[error("malformed or nil payload")]
    MalformedOrNilPayload,
    #[error("corrupted queries section")]
    #[allow(dead_code)]
    CorruptedQueriesSection,
    #[error("corrupted request headers section")]
    #[allow(dead_code)]
    CorruptedRequestHeadersSection,
    #[error("corrupted status code section")]
    CorruptedStatusCodeSection,
    #[error("corrupted response headers section")]
    CorruptedResponseHeadersSection,
    #[error("corrupted response body section")]
    CorruptedResponseBodySection,
}

impl Entry {
    /// Gets the response payload (headers, body, code).
    pub fn response_payload(&self) -> Result<ResponsePayload, PayloadError> {
        let data = self.get_payload_data()?;

        let code = self.unpack_status_code(&data)?;
        let headers = self.unpack_response_headers(&data)?;
        let body = self.unpack_response_body(&data)?;

        Ok(ResponsePayload {
            headers,
            body,
            code,
        })
    }

    /// Unpacks status code from the payload.
    fn unpack_status_code(&self, data: &[u8]) -> Result<u16, PayloadError> {
        let offset_from = LittleEndian::read_u32(&data[OFF_STATUS..OFF_STATUS + OFF_WEIGHT]) as usize;
        let offset_to = offset_from + OFF_WEIGHT;

        if offset_to > data.len() {
            return Err(PayloadError::CorruptedStatusCodeSection);
        }

        let code = LittleEndian::read_u32(&data[offset_from..offset_to]) as u16;
        Ok(code)
    }

    /// Unpacks response headers from the payload.
    fn unpack_response_headers(&self, data: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>, PayloadError> {
        let offset_from = LittleEndian::read_u32(&data[OFF_RESP_HDRS..OFF_RESP_HDRS + OFF_WEIGHT]) as usize;
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
    fn unpack_response_body(&self, data: &[u8]) -> Result<Vec<u8>, PayloadError> {
        let body_offset = LittleEndian::read_u32(&data[OFF_BODY..OFF_BODY + OFF_WEIGHT]) as usize;

        if body_offset + OFF_WEIGHT > data.len() {
            return Err(PayloadError::CorruptedResponseBodySection);
        }

        let body_len = LittleEndian::read_u32(&data[body_offset..body_offset + OFF_WEIGHT]) as usize;
        let offset_from = body_offset + OFF_WEIGHT;
        let offset_to = offset_from + body_len;

        if offset_to > data.len() {
            return Err(PayloadError::CorruptedResponseBodySection);
        }

        Ok(data[offset_from..offset_to].to_vec())
    }

    /// Gets the payload data, checking for validity.
    fn get_payload_data(&self) -> Result<Vec<u8>, PayloadError> {
        let payload_guard = self.payload.lock().unwrap();
        match payload_guard.as_ref() {
            Some(data) => {
                if data.len() < OFFSETS_MAP_SIZE {
                    return Err(PayloadError::MalformedOrNilPayload);
                }
                Ok(data.clone())
            }
            None => Err(PayloadError::MalformedOrNilPayload),
        }
    }
}

