// Package model provides entry serialization to/from bytes.

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use std::io::{Read, Write};
use crate::config::Config;
use super::{Entry, match_cache_rule};

impl Entry {
    /// Encodes Entry into a compact little-endian binary format.
    /// Fingerprint is intentionally excluded from the wire format.
    ///
    /// Layout (Little-Endian):
    /// - uint32  rulePathLen
    /// - []byte  rulePath
    /// - uint64  key
    /// - uint64  fingerprintHi
    /// - uint64  fingerprintLo
    /// - uint64  updatedAtUnix
    /// - uint32  payloadLen
    /// - []byte  payload
    ///
    pub fn to_bytes(&self) -> Vec<u8> {
        let rule_path = self.rule.path_bytes.as_deref().unwrap_or(&[]);
        let payload = self.payload_bytes();

        // Pre-calculate size
        let mut total = 0;
        total += 4 + rule_path.len(); // rulePathLen + rulePath
        total += 8; // key
        total += 8; // fingerprintHi
        total += 8; // fingerprintLo
        total += 8; // updatedAt
        total += 4 + payload.len(); // payloadLen + payload

        let mut buf = Vec::with_capacity(total);

        // rulePath
        buf.write_u32::<LittleEndian>(rule_path.len() as u32).unwrap();
        buf.write_all(rule_path).unwrap();

        // key
        buf.write_u64::<LittleEndian>(self.key).unwrap();

        // fingerprint HI
        buf.write_u64::<LittleEndian>(self.fingerprint_hi).unwrap();

        // fingerprint LO
        buf.write_u64::<LittleEndian>(self.fingerprint_lo).unwrap();

        // updatedAt
        let updated_at = self.updated_at.load(std::sync::atomic::Ordering::Relaxed);
        buf.write_u64::<LittleEndian>(updated_at as u64).unwrap();

        // payload
        buf.write_u32::<LittleEndian>(payload.len() as u32).unwrap();
        if !payload.is_empty() {
            buf.write_all(&payload).unwrap();
        }

        buf
    }
}

/// Decodes Entry from the wire format described in ToBytes.
pub fn from_bytes(data: &[u8], cfg: &Config) -> Result<Entry, Box<dyn std::error::Error + Send + Sync>> {
    use std::io::Cursor;
    use std::sync::Arc;

    const U32: usize = 4;
    const U64: usize = 8;

    let mut cursor = Cursor::new(data);

    // rulePathLen
    if cursor.position() as usize + U32 > data.len() {
        return Err("truncated buffer: rulePathLen".into());
    }
    let rule_path_len = cursor.read_u32::<LittleEndian>()? as usize;

    // rulePath
    if cursor.position() as usize + rule_path_len > data.len() {
        return Err("truncated buffer: rulePath".into());
    }
    let mut rule_path = vec![0u8; rule_path_len];
    cursor.read_exact(&mut rule_path)?;

    let rule = match_cache_rule(cfg, &rule_path)?;

    // key
    if cursor.position() as usize + U64 > data.len() {
        return Err("truncated buffer: key".into());
    }
    let key = cursor.read_u64::<LittleEndian>()?;

    // fingerprint HI
    if cursor.position() as usize + U64 > data.len() {
        return Err("truncated buffer: fingerprintHI".into());
    }
    let f_hi = cursor.read_u64::<LittleEndian>()?;

    // fingerprint LO
    if cursor.position() as usize + U64 > data.len() {
        return Err("truncated buffer: fingerprintLO".into());
    }
    let f_lo = cursor.read_u64::<LittleEndian>()?;

    // updatedAt
    if cursor.position() as usize + U64 > data.len() {
        return Err("truncated buffer: updatedAt".into());
    }
    let updated_at = cursor.read_u64::<LittleEndian>()? as i64;

    // payloadLen
    if cursor.position() as usize + U32 > data.len() {
        return Err("truncated buffer: payloadLen".into());
    }
    let payload_len = cursor.read_u32::<LittleEndian>()? as usize;

    // payload
    if cursor.position() as usize + payload_len > data.len() {
        return Err("truncated buffer: payload".into());
    }
    let mut payload = vec![0u8; payload_len];
    cursor.read_exact(&mut payload)?;

    Ok(Entry::from_field(
        key,
        f_hi,
        f_lo,
        payload,
        Arc::new(rule.clone()),
        updated_at,
    ))
}

