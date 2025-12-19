//! Entry serialization to/from bytes.
//

use byteorder::{LittleEndian, ReadBytesExt};
use std::io::Read;

use crate::config::Config;

use super::{match_cache_rule, Entry};

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
        let rule_path = self.0.rule.path_bytes.as_deref().unwrap_or(&[]);
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

        // rulePath - write u32 as little-endian bytes
        buf.extend_from_slice(&(rule_path.len() as u32).to_le_bytes());
        buf.extend_from_slice(rule_path);

        // key
        buf.extend_from_slice(&self.0.key.to_le_bytes());

        // fingerprint HI
        buf.extend_from_slice(&self.0.fingerprint_hi.to_le_bytes());

        // fingerprint LO
        buf.extend_from_slice(&self.0.fingerprint_lo.to_le_bytes());

        // updatedAt
        let updated_at = self.0.updated_at.load(std::sync::atomic::Ordering::Relaxed);
        buf.extend_from_slice(&(updated_at as u64).to_le_bytes());

        // payload
        buf.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        if !payload.is_empty() {
            buf.extend_from_slice(&payload);
        }

        buf
    }
}

/// Decodes Entry from the wire format described in ToBytes.
pub fn from_bytes(
    data: &[u8],
    cfg: &Config,
) -> Result<Entry, Box<dyn std::error::Error + Send + Sync>> {
    use std::io::Cursor;

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
        rule.clone(), // Already Arc<Rule>, just clone the Arc
        updated_at,
    ))
}
