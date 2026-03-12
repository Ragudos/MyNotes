#[cfg(test)]
mod line_ending_test {
    use mynotes_core::line_ending::{LineEnding, LineEndingScores};

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
    fn detects_platform_line_ending_correctly() {
        let platform_detected = LineEnding::from_current_platform();
        let read_file_detected = LineEnding::from(get_readme_contents().as_bytes());

        // Both methods should produce the same result
        assert_eq!(
            platform_detected, read_file_detected,
            "Platform and read file should match"
        );

        // Expected result based on platform
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
    fn detects_lf_correctly() {
        let sample = "l1\nl2\nl3".as_bytes();

        assert_eq!(LineEnding::from(sample), LineEnding::LF);
    }

    #[test]
    fn detects_crlf_correctly() {
        let sample = "l1\r\nl2\r\nl3".as_bytes();

        assert_eq!(LineEnding::from(sample), LineEnding::CRLF);
    }

    #[test]
    fn detects_cr_correctly() {
        let sample = "l1\rl2\rl3".as_bytes();

        assert_eq!(LineEnding::from(sample), LineEnding::CR);
    }

    #[test]
    fn handles_mixed_line_endings() {
        let mostly_lf = "l1\nl2\r\nl3\rl4\nl5\nl6\n".as_bytes();

        assert_eq!(LineEnding::from(mostly_lf), LineEnding::LF);
        assert_eq!(
            LineEnding::calculate_score(mostly_lf),
            [
                (LineEnding::LF, 4),
                (LineEnding::CRLF, 1),
                (LineEnding::CR, 1),
            ]
            .into_iter()
            .collect::<LineEndingScores>()
        );

        let mostly_cr = "l1\rl2\r\nl3\nl4\rl5\rl6\r".as_bytes();

        assert_eq!(LineEnding::from(mostly_cr), LineEnding::CR);
        assert_eq!(
            LineEnding::calculate_score(mostly_cr),
            [
                (LineEnding::LF, 1),
                (LineEnding::CRLF, 1),
                (LineEnding::CR, 4),
            ]
            .into_iter()
            .collect::<LineEndingScores>()
        );

        let mostly_crlf = "l1\r\nl2\rl3\nl4\r\nl5\r\nl6\r\n".as_bytes();

        assert_eq!(LineEnding::from(mostly_crlf), LineEnding::CRLF);
        assert_eq!(
            LineEnding::calculate_score(mostly_crlf),
            [
                (LineEnding::LF, 1),
                (LineEnding::CRLF, 4),
                (LineEnding::CR, 1),
            ]
            .into_iter()
            .collect::<LineEndingScores>()
        );
    }

    #[test]
    fn handles_mixed_line_edge_cases() {
        // Case 1: One line ending is dominant
        let mostly_lf = "l1\nl2\r\nl3\rl4\nl5\nl6\n".as_bytes();

        assert_eq!(LineEnding::from(mostly_lf), LineEnding::LF);

        // Case 2: All line endings appear equally
        let equal_mixed = "l1\nl2\r\nl3\rl4\nl5\r\nl6\r".as_bytes();

        // CRLF is the fallback if all exists
        assert_eq!(LineEnding::from(equal_mixed), LineEnding::CRLF);

        // Case 4: Empty Defaults to CRLF
        let empty = "".as_bytes();

        assert_eq!(LineEnding::from(empty), LineEnding::CRLF);
    }

    #[test]
    fn test_as_char_returns_single_byte_for_lf_and_cr() {
        // LF should return '\n'
        assert_eq!(LineEnding::LF.as_byte(), b'\n');
        // CR should return '\r'
        assert_eq!(LineEnding::CR.as_byte(), b'\r');
    }

    #[test]
    #[should_panic(expected = "CRLF cannot be represented as a byte")]
    fn test_as_char_panics_for_crlf() {
        // CRLF is composed of two characters, so this should panic.
        let _ = LineEnding::CRLF.as_byte();
    }
}
