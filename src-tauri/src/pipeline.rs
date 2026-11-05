//! What happens to the text between transcription and paste.
//!
//! Today it only tidies whitespace. Translation, punctuation fixes or other
//! rewrites plug in here later as extra steps.

pub fn process(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapses_whitespace() {
        assert_eq!(process("  سلام   دنیا \n"), "سلام دنیا");
    }
}
