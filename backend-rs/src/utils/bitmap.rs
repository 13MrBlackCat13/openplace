use base64::Engine;

/// Port of src/utils/bitmap.ts (WplaceBitMap): flag bit i lives in
/// `byteIndex = i / 8`, `bitIndex = i % 8`, but the byte is indexed from the
/// END of the buffer (`realIndex = len - 1 - byteIndex`) — a big-endian
/// number layout. Stored in the DB as bytea, over the API as base64.
#[derive(Debug, Clone, Default)]
pub struct WplaceBitMap {
    pub bytes: Vec<u8>,
}

impl WplaceBitMap {
    pub fn new() -> Self {
        Self { bytes: vec![0] }
    }

    pub fn from_bytes(bytes: Option<Vec<u8>>) -> Self {
        match bytes {
            Some(b) if !b.is_empty() => Self { bytes: b },
            _ => Self::new(),
        }
    }

    fn real_index(&self, byte_index: usize) -> Option<usize> {
        byte_index
            .checked_sub(0)
            .and_then(|_| self.bytes.len().checked_sub(byte_index + 1))
    }

    pub fn get(&self, index: u32) -> bool {
        let byte_index = (index / 8) as usize;
        let Some(real) = self.real_index(byte_index) else {
            return false;
        };
        (self.bytes[real] >> (index % 8)) & 1 == 1
    }

    pub fn set(&mut self, index: u32, value: bool) {
        let byte_index = (index / 8) as usize;
        let bit = 1u8 << (index % 8);
        if self.bytes.len() <= byte_index {
            // Grow preserving the reversed layout: new bytes go to the front.
            let needed = byte_index + 1;
            let mut bytes = vec![0u8; needed - self.bytes.len()];
            bytes.extend_from_slice(&self.bytes);
            self.bytes = bytes;
        }
        let real = self.real_index(byte_index).unwrap();
        if value {
            self.bytes[real] |= bit;
        } else {
            self.bytes[real] &= !bit;
        }
    }

    pub fn to_base64(&self) -> String {
        base64::engine::general_purpose::STANDARD.encode(&self.bytes)
    }

    pub fn from_base64(s: &str) -> Self {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(s)
            .unwrap_or_else(|_| vec![0]);
        Self::from_bytes(Some(bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_get_roundtrip_reversed_layout() {
        let mut bm = WplaceBitMap::new();
        assert!(!bm.get(1));
        bm.set(1, true);
        bm.set(251, true);
        assert!(bm.get(1));
        assert!(bm.get(251));
        bm.set(1, false);
        assert!(!bm.get(1));
        assert!(bm.get(251));
    }

    #[test]
    fn base64_roundtrip() {
        let mut bm = WplaceBitMap::new();
        bm.set(64, true);
        let restored = WplaceBitMap::from_base64(&bm.to_base64());
        assert!(restored.get(64));
        assert!(!restored.get(63));
    }

    #[test]
    fn out_of_range_get_is_false() {
        let bm = WplaceBitMap::new();
        assert!(!bm.get(10_000));
    }
}
