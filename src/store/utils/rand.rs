use rand::{thread_rng, Rng};

const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";

/// Generates a random alphanumeric string of a given length.
pub fn gen_rand_str(len: usize) -> String {
    let mut rng = thread_rng();
    let random_string: String = (0..len)
        .map(|_| {
            let idx = rng.gen_range(0, CHARSET.len());
            CHARSET[idx] as char
        })
        .collect();

    random_string
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_gen_rand_str_uniqueness() {
        // Generate 1000 random strings of length 8, verify zero collisions
        let mut seen = HashSet::with_capacity(1000);
        for _ in 0..1000 {
            let s = gen_rand_str(8);
            assert!(
                seen.insert(s.clone()),
                "Collision detected: gen_rand_str(8) produced duplicate '{}'",
                s
            );
            assert_eq!(s.len(), 8, "gen_rand_str(8) should produce 8-char strings");
        }
        assert_eq!(seen.len(), 1000, "All 1000 generated strings should be unique");
    }

    #[test]
    fn test_gen_rand_str_length() {
        assert_eq!(gen_rand_str(0).len(), 0);
        assert_eq!(gen_rand_str(1).len(), 1);
        assert_eq!(gen_rand_str(16).len(), 16);
        assert_eq!(gen_rand_str(64).len(), 64);
    }
}
