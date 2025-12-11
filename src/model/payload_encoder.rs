// Package model provides payload encoding functionality.

use byteorder::{ByteOrder, LittleEndian, WriteBytesExt};
use super::{Entry, Response};

/// Payload offset constants.
pub const OFFSETS_MAP_SIZE: usize = 20;
pub const OFF_QUERY: usize = 0;
pub const OFF_REQ_HDRS: usize = 4;
pub const OFF_STATUS: usize = 8;
pub const OFF_RESP_HDRS: usize = 12;
pub const OFF_BODY: usize = 16;
pub const OFF_WEIGHT: usize = 4;

impl Entry {
    /// Sets the payload from queries, headers, and response.
    pub fn set_payload(
        &mut self,
        queries: &[(Vec<u8>, Vec<u8>)],
        headers: &[(Vec<u8>, Vec<u8>)],
        resp: &Response,
    ) {
        let (length, _capacity) = self.calc_payload_length(queries, headers, resp);
        let mut buf = Vec::with_capacity(length);

        // Reserve space for offsets map
        buf.extend_from_slice(&[0u8; OFFSETS_MAP_SIZE]);

        // Pack sections
        let queries_offset = self.pack_queries(&mut buf, queries);
        let req_hdrs_offset = self.pack_request_headers(&mut buf, headers);
        let status_offset = self.pack_status_code(&mut buf, resp.status);
        let resp_hdrs_offset = self.pack_response_headers(&mut buf, &resp.headers);
        let body_offset = self.pack_body(&mut buf, &resp.body);

        // Write offsets back to header
        let mut cursor = std::io::Cursor::new(&mut buf);
        cursor.set_position(OFF_QUERY as u64);
        cursor.write_u32::<LittleEndian>(queries_offset as u32).unwrap();
        cursor.set_position(OFF_REQ_HDRS as u64);
        cursor.write_u32::<LittleEndian>(req_hdrs_offset as u32).unwrap();
        cursor.set_position(OFF_STATUS as u64);
        cursor.write_u32::<LittleEndian>(status_offset as u32).unwrap();
        cursor.set_position(OFF_RESP_HDRS as u64);
        cursor.write_u32::<LittleEndian>(resp_hdrs_offset as u32).unwrap();
        cursor.set_position(OFF_BODY as u64);
        cursor.write_u32::<LittleEndian>(body_offset as u32).unwrap();

        // Store payload
        *self.payload.lock().unwrap() = Some(buf);
    }

    /// Packs queries into the buffer.
    fn pack_queries(&self, dst: &mut Vec<u8>, queries: &[(Vec<u8>, Vec<u8>)]) -> usize {
        let offset = dst.len();
        for (key, val) in queries {
            append_u32(dst, key.len() as u32);
            dst.extend_from_slice(key);
            append_u32(dst, val.len() as u32);
            dst.extend_from_slice(val);
        }
        offset
    }

    /// Packs request headers into the buffer.
    fn pack_request_headers(&self, dst: &mut Vec<u8>, headers: &[(Vec<u8>, Vec<u8>)]) -> usize {
        let offset = dst.len();
        for (key, val) in headers {
            append_u32(dst, key.len() as u32);
            dst.extend_from_slice(key);
            append_u32(dst, val.len() as u32);
            dst.extend_from_slice(val);
        }
        offset
    }

    /// Packs status code into the buffer.
    fn pack_status_code(&self, dst: &mut Vec<u8>, status: u16) -> usize {
        let offset = dst.len();
        append_u32(dst, status as u32);
        offset
    }

    /// Packs response headers into the buffer.
    fn pack_response_headers(&self, dst: &mut Vec<u8>, headers: &[(String, String)]) -> usize {
        let offset = dst.len();
        for (key, val) in headers {
            append_u32(dst, key.len() as u32);
            dst.extend_from_slice(key.as_bytes());
            append_u32(dst, val.len() as u32);
            dst.extend_from_slice(val.as_bytes());
        }
        offset
    }

    /// Packs body into the buffer.
    fn pack_body(&self, dst: &mut Vec<u8>, body: &[u8]) -> usize {
        let offset = dst.len();
        append_u32(dst, body.len() as u32);
        dst.extend_from_slice(body);
        offset
    }

    /// Calculates the payload length needed.
    fn calc_payload_length(
        &self,
        queries: &[(Vec<u8>, Vec<u8>)],
        headers: &[(Vec<u8>, Vec<u8>)],
        resp: &Response,
    ) -> (usize, usize) {
        let mut length = OFFSETS_MAP_SIZE;

        // Request processing
        for (k, v) in queries {
            length += OFF_WEIGHT + k.len();
            length += OFF_WEIGHT + v.len();
        }
        for (k, v) in headers {
            length += OFF_WEIGHT + k.len();
            length += OFF_WEIGHT + v.len();
        }

        // Response processing
        length += OFF_WEIGHT; // status code
        for (k, v) in &resp.headers {
            length += OFF_WEIGHT + k.len();
            length += OFF_WEIGHT + v.len();
        }
        length += OFF_WEIGHT + resp.body.len();

        (length, length)
    }
}

/// Appends a little-endian uint32 to the buffer.
fn append_u32(dst: &mut Vec<u8>, v: u32) {
    let mut bytes = [0u8; 4];
    LittleEndian::write_u32(&mut bytes, v);
    dst.extend_from_slice(&bytes);
}

