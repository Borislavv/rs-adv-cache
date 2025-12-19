//! Lock-free, allocation-free random helpers.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

/// Shard structure for lock-free random number generation.
struct Shard {
    state: AtomicU64,
}

static SHARDS: OnceLock<Vec<Shard>> = OnceLock::new();
static MASK: OnceLock<u32> = OnceLock::new();
static RR: AtomicU32 = AtomicU32::new(0);

/// Initializes the random number generator.
fn init_rnd(n: usize) {
    let n = if n == 0 {
        let procs = num_cpus::get();
        (procs * 4).max(1)
    } else {
        n
    };

    // Round to power of 2
    let p = n.next_power_of_two();
    let mask = (p - 1) as u32;

    let seed = splitmix_seed(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as i64,
    );

    let mut shards = Vec::with_capacity(p);
    let mut current_seed = seed;
    for _ in 0..p {
        current_seed = splitmix_next(&mut current_seed);
        let state = if current_seed == 0 {
            0x9e3779b97f4a7c15
        } else {
            current_seed
        };
        shards.push(Shard {
            state: AtomicU64::new(state),
        });
    }

    SHARDS.set(shards).ok();
    MASK.set(mask).ok();
    RR.store(0, Ordering::Relaxed);
}

/// Returns a uniform random float in [0,1) using 53 random bits.
pub fn float64() -> f64 {
    // Ensure initialization
    if SHARDS.get().is_none() {
        init_rnd(0);
    }

    let shards = SHARDS.get().unwrap();
    let mask = MASK.get().unwrap();

    let i = (RR.fetch_add(1, Ordering::Relaxed) & mask) as usize;
    let x = splitmix_next_atomic(&shards[i].state);

    // Take top 53 bits -> [0,1)
    const INV53: f64 = 1.0 / 9007199254740992.0; // 2^53
    (x >> 11) as f64 * INV53
}

/// Advances the state atomically and returns a mixed 64-bit value.
fn splitmix_next_atomic(s: &AtomicU64) -> u64 {
    loop {
        let old = s.load(Ordering::Relaxed);
        let x = old.wrapping_add(0x9e3779b97f4a7c15);
        if s.compare_exchange(old, x, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
        {
            return mix(x);
        }
    }
}

/// Advances the state and returns a mixed 64-bit value (non-atomic version).
fn splitmix_next(s: &mut u64) -> u64 {
    *s = s.wrapping_add(0x9e3779b97f4a7c15);
    mix(*s)
}

/// Mixes a 64-bit value using SplitMix64 algorithm.
fn mix(z: u64) -> u64 {
    let mut z = z;
    z ^= z >> 30;
    z = z.wrapping_mul(0xbf58476d1ce4e5b9);
    z ^= z >> 27;
    z = z.wrapping_mul(0x94d049bb133111eb);
    z ^= z >> 31;
    z
}

/// Turns a signed seed into a decent 64-bit starting state.
fn splitmix_seed(seed: i64) -> u64 {
    let mut z = (seed as u64).wrapping_add(0x9e3779b97f4a7c15u64);
    z = mix(z);
    if z == 0 {
        z = 0x9e3779b97f4a7c15;
    }
    z
}

// Initialize on module load
#[ctor::ctor]
fn init() {
    init_rnd(0);
}
