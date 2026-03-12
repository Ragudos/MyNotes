use memchr::memchr2_iter;

use crate::types::{CARRIAGE_RETURN_BYTE, NEWLINE_BYTE};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LineEndingScores {
    pub lf: u64,
    pub cr_lf: u64,
    pub cr: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(clippy::upper_case_acronyms)]
pub enum LineEnding {
    /// Line Feed (LF) - Common on Unix, Linux, and macOS (`\n`).
    LF,
    /// Carriage Return + Line Feed (CRLF) - Used on Windows (`\r\n`).
    CRLF,
    /// Carriage Return (CR) - Used in older Mac OS (pre-OS X) (`\r`).
    CR,
}

impl LineEnding {
    /// # Purpose
    ///
    /// Counts occurrences of each line ending type in the given byte string.
    ///
    /// Analyzes the input and returns a `LineEndingScores`
    /// or a `HashMap<LineEnding, usize>` containing the number of times
    /// each line ending appears.
    ///
    /// - `CRLF (\r\n)` is counted if a `\n` byte is preceded by a `\r` byte
    /// - `CR (\r)` is counted if a `\r` byte does not precede a `\n` byte
    /// - `LF (\r)` is counted if a `\n` byte is not preceded by a `\r` byte
    ///
    /// # Optimization
    ///
    /// Uses `memchr` to find the positions of all line ending bytes to
    /// lessen the amount of iterations needed (SIMD).
    ///
    /// # Misc
    ///
    /// If we don't deal with bytes, potential way to do this is
    /// `String::new().split(line_ending.as_str()).len()`
    pub(crate) fn calculate_score(bytes: &[u8]) -> LineEndingScores {
        let mut crlf_score = 0u64;
        let mut lf_score = 0u64;
        let mut cr_score = 0u64;

        {
            let mut skip = false;

            for byte_idx in memchr2_iter(NEWLINE_BYTE, CARRIAGE_RETURN_BYTE, bytes) {
                if skip {
                    skip = false;
                    continue;
                }

                match bytes[byte_idx] {
                    NEWLINE_BYTE => {
                        lf_score += 1;
                    }
                    CARRIAGE_RETURN_BYTE => {
                        if bytes.get(byte_idx + 1) == Some(&NEWLINE_BYTE) {
                            crlf_score += 1;
                            skip = true;
                        } else {
                            cr_score += 1;
                        }
                    }
                    _ => unreachable!("Encountered an invalid byte provided by memchr2_iter"),
                }
            }
        }

        LineEndingScores {
            lf: lf_score,
            cr_lf: crlf_score,
            cr: cr_score,
        }
    }
}

impl LineEnding {
    /// # Purpose
    ///
    /// Detects the current operating system
    /// and returns the respective `LineEnding`
    /// based on that
    ///
    /// - **Unix (Linux/macOS):** Has LF (`\n`)
    /// - **Windows:** Has CRLF (`\r\n`)
    ///
    /// # Example
    ///
    /// ```
    /// use mynotes_core::line_ending::LineEnding;
    ///
    /// let default_ln = LineEnding::from_current_platform();
    ///
    /// println!("Default line ending: {:?}", default_ln);
    /// ```
    pub fn from_current_platform() -> Self {
        if cfg!(windows) { Self::CRLF } else { Self::LF }
    }

    /// # Returns
    ///
    /// The bytes representation of the line ending
    /// (`\n`, `\r\n`, or `\r`).
    ///
    /// # Example
    ///
    /// ```
    /// use mynotes_core::line_ending::LineEnding;
    ///
    /// assert_eq!(LineEnding::LF.as_bytes(), b"\n");
    /// assert_eq!(LineEnding::CRLF.as_bytes(), b"\r\n");
    /// assert_eq!(LineEnding::CR.as_bytes(), b"\r");
    /// ```
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            LineEnding::LF => b"\n",
            LineEnding::CRLF => b"\r\n",
            LineEnding::CR => b"\r",
        }
    }

    /// # Returns
    ///
    /// The byte representation of the line ending
    /// if it is a single byte/character.
    ///
    /// # Panics
    ///
    /// If the line ending is `CRLF`, because it composes
    /// of two bytes or characters and cannot be represented as a single byte.
    pub fn as_byte(&self) -> u8 {
        match self {
            Self::LF => b'\n',
            Self::CR => b'\r',
            Self::CRLF => panic!("CRLF cannot be represented as a byte"),
        }
    }
}

/// # Purpose
///
/// Detects the line ending style that appears
/// the most in a string in any representation of itself.
///
/// # Edge Case
///
/// Defaults to `CRLF` if all line endings appear equally
/// or if there are no line endings.
///
/// # Example
///
/// ```
/// use utils::line_ending::{LineEnding, create_line_ending};
///
/// let text = "Hello\r\nWorld\r\nThis is a test.\r\n";
/// let bytes_text = text.as_bytes();
/// let text_line_ending = create_line_ending(text);
/// let bytes_line_ending = create_line_ending(bytes_text);
///
/// assert_eq!(text_line_ending, LineEnding::CRLF);
/// assert_eq!(bytes_line_ending, LineEnding::CRLF);
/// ```
#[inline]
pub fn create_line_ending<T: AsRef<[u8]>>(text: T) -> LineEnding {
    create_line_ending_impl(text.as_ref())
}

fn create_line_ending_impl(bytes: &[u8]) -> LineEnding {
    let scores = LineEnding::calculate_score(bytes);
    let max_score = scores.cr_lf.max(scores.cr).max(scores.lf);

    if max_score == 0 || scores.cr_lf == max_score {
        LineEnding::CRLF
    } else if scores.cr == max_score {
        LineEnding::CR
    } else {
        LineEnding::LF
    }
}

#[cfg(test)]
mod line_ending_tests {
    use super::*;

    fn get_readme_contents() -> String {
        use std::fs::File;
        use std::io::Read;

        let mut read_content = String::new();

        File::open("../../README.md")
            .unwrap_or_else(|_| panic!("Could not find README.md!"))
            .read_to_string(&mut read_content)
            .unwrap_or_else(|_| panic!("Could not read README.md!"));

        read_content
    }

    #[test]
    fn detects_platform_line_ending() {
        let platform_detected = LineEnding::from_current_platform();
        let read_file_detected = create_line_ending(get_readme_contents());

        assert_eq!(
            platform_detected, read_file_detected,
            "Platform and read file should match"
        );

        let expected = if cfg!(target_os = "windows") {
            LineEnding::CRLF
        } else {
            LineEnding::LF
        };

        assert_eq!(
            platform_detected, expected,
            "Detected platform line ending should match expected"
        );
    }

    #[test]
    fn detects_lf() {
        assert_eq!(create_line_ending("l1\nl2\nl3"), LineEnding::LF);
    }

    #[test]
    fn detects_crlf() {
        assert_eq!(create_line_ending("l1\r\nl2\r\nl3"), LineEnding::CRLF);
    }

    #[test]
    fn detects_cr() {
        assert_eq!(create_line_ending("l1\rl2\rl3"), LineEnding::CR);
    }

    #[test]
    fn detects_mixed_line_endings() {
        let mostly_lf = "l1\nl2\nl3\nl4\nl5\r\nl6\rl7";

        assert_eq!(create_line_ending(mostly_lf), LineEnding::LF);
        assert_eq!(
            LineEnding::calculate_score(mostly_lf.as_bytes()),
            LineEndingScores {
                lf: 4,
                cr_lf: 1,
                cr: 1
            }
        );

        let mostly_crlf = "l1\r\nl2\r\nl3\r\nl4\r\nl5\nl6\rl7";

        assert_eq!(create_line_ending(mostly_crlf), LineEnding::CRLF);
        assert_eq!(
            LineEnding::calculate_score(mostly_crlf.as_bytes()),
            LineEndingScores {
                lf: 1,
                cr_lf: 4,
                cr: 1
            }
        );

        let mostly_cr = "l1\rl2\rl3\rl4\rl5\r\nl6\nl7";

        assert_eq!(create_line_ending(mostly_cr), LineEnding::CR);
        assert_eq!(
            LineEnding::calculate_score(mostly_cr.as_bytes()),
            LineEndingScores {
                lf: 1,
                cr_lf: 1,
                cr: 4
            }
        );
    }

    #[test]
    fn handles_mixed_line_edge_cases() {
        // CASE 1: All line endings are equal
        assert_eq!(create_line_ending("l1\nl2\r\nl3\rl4"), LineEnding::CRLF,);
        // CASE 2: Empty string
        assert_eq!(create_line_ending(""), LineEnding::CRLF);
    }

    #[test]
    fn as_bytes_returns_correct_bytes() {
        assert_eq!(LineEnding::LF.as_bytes(), b"\n");
        assert_eq!(LineEnding::CRLF.as_bytes(), b"\r\n");
        assert_eq!(LineEnding::CR.as_bytes(), b"\r");
    }

    #[test]
    #[should_panic(expected = "CRLF cannot be represented as a byte")]
    fn as_byte_panics_for_crlf() {
        LineEnding::CRLF.as_byte();
    }

    #[test]
    fn as_byte_returns_correct_bytes() {
        assert_eq!(LineEnding::LF.as_byte(), b'\n');
        assert_eq!(LineEnding::CR.as_byte(), b'\r');
    }
}
