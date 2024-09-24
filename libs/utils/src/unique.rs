use core::hash::Hash;
use hash32::{FnvHasher, Hasher};
use rand::Rng;

pub fn hash_from_string<T: Hash>(obj: T) -> u32 {
    let mut hasher = FnvHasher::default();
    obj.hash(&mut hasher);
    hasher.finish32()
}

pub fn generate_unique_i32() -> i32 {
    let mut rng = rand::thread_rng();
    rng.gen::<i32>()
}

pub fn generate_unique_u32() -> u32 {
    let mut rng = rand::thread_rng();
    rng.gen::<u32>()
}
