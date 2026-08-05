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
