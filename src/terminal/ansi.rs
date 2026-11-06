//! Read the colors in what `tmux capture-pane -e` prints: plain text with
//! SGR escape sequences (`ESC [ … m`) for colors and bold, underline and so
//! on. Other escape sequences are skipped.

/// A text or background color.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Color {
    /// The terminal's own text or background color (the theme's).
    #[default]
    Default,
    /// One of the 256 numbered colors: 0 to 15 are the classic ones.
    Indexed(u8),
    Rgb(u8, u8, u8),
}

/// How a run of text looks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[expect(clippy::struct_excessive_bools, reason = "each flag is one independent SGR attribute")]
pub struct CellStyle {
    pub foreground: Color,
    pub background: Color,
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    /// Swap text and background colors.
    pub reverse: bool,
    pub strikethrough: bool,
}

/// A run of text in one style.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub style: CellStyle,
}

const ESCAPE: char = '\u{1b}';

/// Split one line into styled runs. `style` is the style the line starts
/// with, and is left as the style at its end (tmux keeps a style across
/// lines without repeating it).
pub fn parse_line(line: &str, style: &mut CellStyle) -> Vec<Span> {
    let mut spans: Vec<Span> = Vec::new();
    let mut text = String::new();
    let mut chars = line.chars().peekable();
    while let Some(character) = chars.next() {
        if character != ESCAPE {
            text.push(character);
            continue;
        }
        if chars.peek() != Some(&'[') {
            // Not a CSI sequence: drop the escape and the character after it,
            // plus the charset letter of `ESC ( B` and `ESC ) 0`.
            if matches!(chars.next(), Some('(' | ')')) {
                chars.next();
            }
            continue;
        }
        chars.next();
        let mut parameters = String::new();
        let mut final_byte = None;
        for next in chars.by_ref() {
            if next.is_ascii_digit() || next == ';' || next == ':' {
                parameters.push(next);
            } else {
                final_byte = Some(next);
                break;
            }
        }
        if final_byte == Some('m') {
            push_span(&mut spans, &mut text, *style);
            apply_sgr(style, &parameters);
        }
    }
    push_span(&mut spans, &mut text, *style);
    spans
}

/// Close the run of text typed so far, if any.
fn push_span(spans: &mut Vec<Span>, text: &mut String, style: CellStyle) {
    if !text.is_empty() {
        spans.push(Span { text: std::mem::take(text), style });
    }
}

/// Change `style` as the SGR parameters (`1;31`, `38;5;208`) say.
fn apply_sgr(style: &mut CellStyle, parameters: &str) {
    // Both `38;5;208` and `38:5:208` are used for extended colors.
    let numbers: Vec<u16> = parameters.split([';', ':']).map(|part| part.parse().unwrap_or(0)).collect();
    let mut index = 0;
    while index < numbers.len() {
        let code = numbers[index];
        index += 1;
        match code {
            0 => *style = CellStyle::default(),
            1 => style.bold = true,
            2 => style.dim = true,
            3 => style.italic = true,
            4 => style.underline = true,
            7 => style.reverse = true,
            9 => style.strikethrough = true,
            22 => {
                style.bold = false;
                style.dim = false;
            }
            23 => style.italic = false,
            24 => style.underline = false,
            27 => style.reverse = false,
            29 => style.strikethrough = false,
            30..=37 => style.foreground = Color::Indexed(low_byte(code - 30)),
            39 => style.foreground = Color::Default,
            40..=47 => style.background = Color::Indexed(low_byte(code - 40)),
            49 => style.background = Color::Default,
            90..=97 => style.foreground = Color::Indexed(low_byte(code - 90 + 8)),
            100..=107 => style.background = Color::Indexed(low_byte(code - 100 + 8)),
            38 | 48 => {
                let (color, used) = extended_color(&numbers[index..]);
                index += used;
                if let Some(color) = color {
                    if code == 38 {
                        style.foreground = color;
                    } else {
                        style.background = color;
                    }
                }
            }
            // Blinking, fonts and the rest are not drawn.
            _ => {}
        }
    }
}

/// `5;n` (a numbered color) or `2;r;g;b` after a 38 or 48. Returns the
/// color and how many numbers it used.
fn extended_color(rest: &[u16]) -> (Option<Color>, usize) {
    match rest {
        [5, number, ..] => (Some(Color::Indexed(low_byte(*number))), 2),
        [2, red, green, blue, ..] => (Some(Color::Rgb(low_byte(*red), low_byte(*green), low_byte(*blue))), 4),
        _ => (None, rest.len()),
    }
}

/// A color number cut to the 0 to 255 range escape sequences allow.
fn low_byte(number: u16) -> u8 {
    u8::try_from(number).unwrap_or(u8::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spans(line: &str) -> Vec<Span> {
        parse_line(line, &mut CellStyle::default())
    }

    #[test]
    fn plain_text_is_one_span() {
        assert_eq!(spans("hello"), vec![Span { text: "hello".to_string(), style: CellStyle::default() }]);
        assert_eq!(spans(""), Vec::new());
    }

    #[test]
    fn reads_basic_colors_and_resets() {
        let found = spans("\u{1b}[31mred\u{1b}[39m plain");
        assert_eq!(found[0].text, "red");
        assert_eq!(found[0].style.foreground, Color::Indexed(1));
        assert_eq!(found[1].text, " plain");
        assert_eq!(found[1].style, CellStyle::default());
    }

    #[test]
    fn reads_bright_256_and_true_colors() {
        let found = spans("\u{1b}[1;94ma\u{1b}[38;5;208;48:2:1:2:3mb");
        assert!(found[0].style.bold);
        assert_eq!(found[0].style.foreground, Color::Indexed(12));
        assert_eq!(found[1].style.foreground, Color::Indexed(208));
        assert_eq!(found[1].style.background, Color::Rgb(1, 2, 3));
        assert!(found[1].style.bold, "bold lasts until reset");
    }

    #[test]
    fn style_carries_over_to_the_next_line() {
        let mut style = CellStyle::default();
        parse_line("\u{1b}[7mstart", &mut style);
        let next = parse_line("more", &mut style);
        assert!(next[0].style.reverse);
    }

    #[test]
    fn skips_other_escape_sequences() {
        assert_eq!(spans("a\u{1b}[2Kb\u{1b}(Bc")[0].text, "abc");
    }
}
