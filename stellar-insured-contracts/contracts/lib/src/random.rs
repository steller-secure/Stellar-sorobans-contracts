use soroban_sdk::{Env, IntoVal, TryFromVal, Val, Vec, Address};

pub struct Randomness;

impl Randomness {
    /// Generates a random u64 using Soroban's PRNG.
    /// This is secure and deterministic across validators for a given ledger.
    pub fn next_u64(env: &Env, max: u64) -> u64 {
        env.prng().gen_range(0..max)
    }

    /// Selects a random item from a Vec.
    pub fn select_one<T>(env: &Env, items: Vec<T>) -> Option<T>
    where
        T: Clone + IntoVal<Env, Val> + TryFromVal<Env, Val>,
    {
        if items.is_empty() {
            return None;
        }
        // `gen_range` is implemented for u64 in this SDK; `Vec::len` yields u32.
        let index = env.prng().gen_range::<u64>(0..items.len() as u64) as u32;
        Some(items.get(index).unwrap())
    }

    /// Selects multiple unique items from a Vec (e.g., for auditor selection).
    pub fn select_multiple<T>(env: &Env, items: Vec<T>, count: u32) -> Vec<T>
    where
        T: Clone + PartialEq + IntoVal<Env, Val> + TryFromVal<Env, Val>,
    {
        if items.len() <= count {
            return items;
        }

        let mut selected = Vec::new(env);
        let mut available = items;

        for _ in 0..count {
            let index = env.prng().gen_range::<u64>(0..available.len() as u64) as u32;
            let item = available.get(index).unwrap();
            selected.push_back(item.clone());
            available.remove(index);
        }

        selected
    }
}
