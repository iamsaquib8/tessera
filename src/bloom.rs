use std::hash::{Hash, Hasher};

use bit_vec::BitVec;

/// A small, allocation-free Bloom filter. We use it to short-circuit the
/// hallucination validator: if a symbol isn't in the bloom it's *definitely*
/// not in the graph, so we can return early without an SQL round-trip.
#[derive(Debug, Clone)]
pub struct BloomFilter {
    bits: BitVec,
    k: u32,
    m: u64,
}

impl BloomFilter {
    pub fn for_expected(items: usize, false_positive: f64) -> Self {
        let items = items.max(1) as f64;
        let p = false_positive.clamp(1e-6, 0.5);
        let ln2 = std::f64::consts::LN_2;
        let m = (-(items * p.ln()) / (ln2 * ln2)).ceil() as u64;
        let m = m.max(64);
        let k = ((m as f64 / items) * ln2).ceil() as u32;
        let k = k.clamp(2, 16);
        Self {
            bits: BitVec::from_elem(m as usize, false),
            k,
            m,
        }
    }

    pub fn insert(&mut self, item: &str) {
        let (h1, h2) = double_hash(item);
        for i in 0..self.k {
            let combined = h1.wrapping_add((i as u64).wrapping_mul(h2));
            let index = (combined % self.m) as usize;
            self.bits.set(index, true);
        }
    }

    pub fn contains(&self, item: &str) -> bool {
        let (h1, h2) = double_hash(item);
        (0..self.k).all(|i| {
            let combined = h1.wrapping_add((i as u64).wrapping_mul(h2));
            let index = (combined % self.m) as usize;
            self.bits.get(index).unwrap_or(false)
        })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let bytes = self.bits.to_bytes();
        let mut out = Vec::with_capacity(bytes.len() + 16);
        out.extend_from_slice(&self.k.to_le_bytes());
        out.extend_from_slice(&self.m.to_le_bytes());
        let bit_len = self.bits.len() as u64;
        out.extend_from_slice(&bit_len.to_le_bytes());
        out.extend_from_slice(&bytes);
        out
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 4 + 8 + 8 {
            return None;
        }
        let k = u32::from_le_bytes(bytes[0..4].try_into().ok()?);
        let m = u64::from_le_bytes(bytes[4..12].try_into().ok()?);
        let bit_len = u64::from_le_bytes(bytes[12..20].try_into().ok()?) as usize;
        let raw = &bytes[20..];
        let mut bits = BitVec::from_bytes(raw);
        bits.truncate(bit_len);
        Some(Self { bits, k, m })
    }
}

fn double_hash(item: &str) -> (u64, u64) {
    let mut h1 = std::collections::hash_map::DefaultHasher::new();
    item.hash(&mut h1);
    let h1 = h1.finish();

    let mut h2 = std::collections::hash_map::DefaultHasher::new();
    (item, "tessera").hash(&mut h2);
    let h2 = h2.finish() | 1;
    (h1, h2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let mut bloom = BloomFilter::for_expected(100, 0.01);
        bloom.insert("findById");
        bloom.insert("loadUser");
        assert!(bloom.contains("findById"));
        assert!(bloom.contains("loadUser"));
        // Probabilistic; "nonexistent_unique_xyz_123" should almost certainly miss.
        assert!(!bloom.contains("nonexistent_unique_xyz_123_!@#"));
    }

    #[test]
    fn serialization() {
        let mut bloom = BloomFilter::for_expected(50, 0.01);
        bloom.insert("alpha");
        bloom.insert("beta");
        let bytes = bloom.to_bytes();
        let restored = BloomFilter::from_bytes(&bytes).expect("decode bloom");
        assert!(restored.contains("alpha"));
        assert!(restored.contains("beta"));
    }
}
