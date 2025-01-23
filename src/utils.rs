use nanoid::nanoid;

const DEFAULT_ID_ALPHABET: &str = "abcdefghijklmnopqrstuvwxyz0123456789";
const DEFAULT_ID_SIZE: usize = 10;

pub fn generate_id(alphabet: Option<&str>, size: Option<usize>) -> String {
    let alphabet = alphabet.unwrap_or(DEFAULT_ID_ALPHABET);
    let size = size.unwrap_or(DEFAULT_ID_SIZE);

    nanoid!(size, &alphabet.chars().collect::<Vec<char>>())
}

pub fn test_id(id: Option<String>, alphabet: Option<&str>, size: Option<usize>) -> bool {
    let alphabet = alphabet.unwrap_or(DEFAULT_ID_ALPHABET);
    let size = size.unwrap_or(DEFAULT_ID_SIZE);

    match id {
        Some(id) => id.len() == size && id.chars().all(|c| alphabet.contains(c)),
        None => false,
    }
}
