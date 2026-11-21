use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const ALPHABET: &[u8; 32] = b"0123456789abcdefghjkmnpqrstvwxyz";
static COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn new_id() -> String {
    let ms = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
    let mut hasher = RandomState::new().build_hasher();
    hasher.write_u64(COUNTER.fetch_add(1, Ordering::Relaxed));
    hasher.write_u64(ms);
    let random = hasher.finish();
    let mut out = String::with_capacity(23);
    push_base32(&mut out, ms, 10);
    push_base32(&mut out, random, 13);
    out
}

fn push_base32(out: &mut String, value: u64, digits: u32) {
    for i in (0..digits).rev() {
        out.push(ALPHABET[((value >> (i * 5)) & 31) as usize] as char);
    }
}
