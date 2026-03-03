use memchr::memchr2_iter;
use std::collections::HashMap;

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

/// A type alias for `HashMap<LineEnding, usize>`
/// where each `LineEnding` key is a representation of how many
/// time the line ending appears.
///
/// Tracks the distribution of line endings in a text.
pub type LineEndingScores = HashMap<LineEnding, usize>;

pub const NEWLINE_BYTE: u8 = b'\n';
pub const CARRIAGE_RETURN_BYTE: u8 = b'\r';

impl From<&[u8]> for LineEnding {
    /// Detects the line ending style that appears
    /// the most in the bytes string.
    ///
    /// # Example
    /// ```
    /// use mynotes_core::line_ending::LineEnding
    ///
    /// let sample = b"f\r\ns\r\nt";
    ///
    /// assert_eq!(LineEnding::from(sample), LineEnding::CRLF);
    /// ```
    fn from(value: &[u8]) -> Self {
        let scores = Self::calculate_score(value);
        let crlf_score = *scores.get(&LineEnding::CRLF).unwrap_or(&0);
        let lf_score = *scores.get(&LineEnding::LF).unwrap_or(&0);
        let cr_score = *scores.get(&LineEnding::CR).unwrap_or(&0);

        let max_score = crlf_score.max(cr_score).max(lf_score);

        if max_score == 0 || crlf_score == max_score {
            // `CRLF` is chosen as a tie-breaker because it represents both `CR`
            // and `LF`, making it the most inclusive option
            Self::CRLF
        } else if cr_score == max_score {
            Self::CR
        } else {
            Self::LF
        }
    }
}

impl LineEnding {
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
    ///
    /// # Example
    pub fn calculate_score(bytes: &[u8]) -> LineEndingScores {
        let mut crlf_score = 0;
        let mut lf_score = 0;
        let mut cr_score = 0;

        {
            let mut skip = false;

            for i in memchr2_iter(NEWLINE_BYTE, CARRIAGE_RETURN_BYTE, bytes) {
                if skip {
                    skip = false;

                    continue;
                }

                match bytes[i] {
                    NEWLINE_BYTE => {
                        lf_score += 1;
                    }
                    CARRIAGE_RETURN_BYTE => {
                        if bytes.get(i + 1) == Some(&NEWLINE_BYTE) {
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

        [
            (Self::CRLF, crlf_score),
            (Self::LF, lf_score),
            (Self::CR, cr_score),
        ]
        .into_iter()
        .collect()
    }

    /// Returns the bytes representation of the line ending
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
            LineEnding::CR => b"\n",
        }
    }

    /// Returns the byte representation of the line ending
    /// if it is a single byte/character.
    ///
    /// # Panics
    ///
    /// Panics if the line ending is `CRLF`, because it composes
    /// of two bytes or characters and cannot be represented as a single byte.
    pub fn as_byte(&self) -> u8 {
        match self {
            Self::LF => b'\n',
            Self::CR => b'\r',
            Self::CRLF => panic!("CRLF cannot be represented as a byte"),
        }
    }
}
