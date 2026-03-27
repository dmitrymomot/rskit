/// Converts HTML to plain text by stripping tags, decoding entities,
/// and discarding script/style content.
pub fn html_to_text(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut state = State::Normal;
    let mut tag_buf = String::new();
    let mut entity_buf = String::new();

    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let ch = chars[i];
        match state {
            State::Normal => {
                if ch == '<' {
                    tag_buf.clear();
                    state = State::InsideTag;
                } else if ch == '&' {
                    entity_buf.clear();
                    state = State::InsideEntity;
                } else {
                    output.push(ch);
                }
            }
            State::InsideTag => {
                if ch == '>' {
                    let tag_lower = tag_buf.to_ascii_lowercase();
                    let tag_name = tag_lower.split_whitespace().next().unwrap_or("");
                    if tag_name == "script" {
                        state = State::InsideScript;
                    } else if tag_name == "style" {
                        state = State::InsideStyle;
                    } else {
                        // Tag removed — insert space to prevent word merging
                        if !output.ends_with(' ') && !output.is_empty() {
                            output.push(' ');
                        }
                        state = State::Normal;
                    }
                } else {
                    tag_buf.push(ch);
                }
            }
            State::InsideEntity => {
                if ch == ';' {
                    if let Some(decoded) = decode_entity(&entity_buf) {
                        output.push(decoded);
                    } else {
                        // Unknown entity — pass through as-is
                        output.push('&');
                        output.push_str(&entity_buf);
                        output.push(';');
                    }
                    state = State::Normal;
                } else if ch.is_ascii_alphanumeric() || ch == '#' {
                    entity_buf.push(ch);
                } else {
                    // Not a valid entity — emit what we have and process current char
                    output.push('&');
                    output.push_str(&entity_buf);
                    state = State::Normal;
                    continue; // re-process current char in Normal state
                }
            }
            State::InsideScript => {
                if ch == '<' && matches_closing_tag(&chars, i, "script") {
                    i += "</script>".len() - 1; // skip past closing tag
                    state = State::Normal;
                }
            }
            State::InsideStyle => {
                if ch == '<' && matches_closing_tag(&chars, i, "style") {
                    i += "</style>".len() - 1;
                    state = State::Normal;
                }
            }
        }
        i += 1;
    }

    // Handle unterminated entity
    if state == State::InsideEntity {
        output.push('&');
        output.push_str(&entity_buf);
    }

    collapse_and_trim(&output)
}

#[derive(PartialEq)]
enum State {
    Normal,
    InsideTag,
    InsideEntity,
    InsideScript,
    InsideStyle,
}

fn matches_closing_tag(chars: &[char], pos: usize, tag: &str) -> bool {
    let expected: Vec<char> = format!("</{tag}>").chars().collect();
    if pos + expected.len() > chars.len() {
        return false;
    }
    chars[pos..pos + expected.len()]
        .iter()
        .zip(expected.iter())
        .all(|(a, b)| a.to_ascii_lowercase() == *b)
}

fn decode_entity(name: &str) -> Option<char> {
    match name {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "#39" | "apos" => Some('\''),
        _ if name.starts_with("#x") || name.starts_with("#X") => {
            u32::from_str_radix(&name[2..], 16)
                .ok()
                .and_then(char::from_u32)
        }
        _ if name.starts_with('#') => name[1..].parse::<u32>().ok().and_then(char::from_u32),
        _ => None,
    }
}

fn collapse_and_trim(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut prev_space = false;
    for ch in s.chars() {
        if ch.is_whitespace() {
            if !prev_space && !result.is_empty() {
                result.push(' ');
            }
            prev_space = true;
        } else {
            result.push(ch);
            prev_space = false;
        }
    }
    if result.ends_with(' ') {
        result.pop();
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_basic_tags() {
        assert_eq!(html_to_text("<p>Hello <b>world</b></p>"), "Hello world");
    }

    #[test]
    fn decodes_xml_entities() {
        assert_eq!(html_to_text("&amp; &lt; &gt; &quot; &#39;"), "& < > \" '");
    }

    #[test]
    fn decodes_numeric_entities() {
        assert_eq!(html_to_text("&#65;&#x42;"), "AB");
    }

    #[test]
    fn strips_script_content() {
        assert_eq!(
            html_to_text("<p>before</p><script>if (a < b) { alert(1); }</script><p>after</p>"),
            "before after"
        );
    }

    #[test]
    fn strips_style_content() {
        assert_eq!(
            html_to_text("<p>text</p><style>.foo { color: red; }</style><p>more</p>"),
            "text more"
        );
    }

    #[test]
    fn collapses_whitespace() {
        assert_eq!(html_to_text("<p>hello</p>   <p>world</p>"), "hello world");
    }

    #[test]
    fn handles_self_closing_tags() {
        assert_eq!(html_to_text("hello<br/>world<hr />end"), "hello world end");
    }

    #[test]
    fn empty_input() {
        assert_eq!(html_to_text(""), "");
    }

    #[test]
    fn plain_text_passthrough() {
        assert_eq!(html_to_text("no html here"), "no html here");
    }

    #[test]
    fn unknown_entity_passthrough() {
        assert_eq!(html_to_text("&unknown;"), "&unknown;");
    }

    #[test]
    fn script_case_insensitive() {
        assert_eq!(html_to_text("<SCRIPT>var x = 1;</SCRIPT>hello"), "hello");
    }
}
