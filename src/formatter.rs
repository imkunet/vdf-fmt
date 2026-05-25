use anyhow::{Result, bail};

use crate::syntax::{self, LineToken, ParsedLine};

const INDENT: &str = "    ";
const DISABLED_PAIR_PREFIX: &str = "// ";

#[derive(Debug, Clone, PartialEq, Eq)]
enum OutputBody {
    ActivePair { key: String, value: String },
    DisabledPair { key: String, value: String },
    StandaloneText(String),
    PlainComment(String),
    Blank,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OutputLine {
    indent: usize,
    body: OutputBody,
    trailing_comment: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RenderedLine {
    indent: usize,
    code: String,
    trailing_comment: Option<String>,
}

#[derive(Debug, Clone, Copy)]
struct PairParts<'a> {
    prefix: &'static str,
    key: &'a str,
    value: &'a str,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Options {
    pub reflow_comments: bool,
    pub bare_literals: bool,
}

pub fn format(input: &str) -> Result<String> {
    format_with_options(input, Options::default())
}

pub fn format_with_options(input: &str, options: Options) -> Result<String> {
    let input = syntax::normalize_newlines(input);
    let had_final_newline = input.ends_with('\n');
    let mut formatter = Formatter::new(options);

    for line in input.split('\n') {
        formatter.collect_line(syntax::parse_line(line)?)?;
    }

    formatter.finish(had_final_newline)
}

#[derive(Debug, Default)]
struct Formatter {
    lines: Vec<OutputLine>,
    indent: usize,
    options: Options,
}

impl Formatter {
    fn new(options: Options) -> Self {
        Self {
            options,
            ..Self::default()
        }
    }

    fn collect_line(&mut self, line: ParsedLine) -> Result<()> {
        let mut pending_words = Vec::new();
        let mut emitted_line = false;

        for token in line.tokens {
            match token {
                LineToken::Word(word) => pending_words.push(word),
                LineToken::OpenBrace => {
                    self.flush_pending_words(&mut pending_words);
                    self.push_output_line(OutputBody::StandaloneText("{".to_owned()));
                    self.indent += 1;
                    emitted_line = true;
                }
                LineToken::CloseBrace => {
                    self.flush_pending_words(&mut pending_words);
                    if self.indent == 0 {
                        bail!("unmatched closing brace");
                    }
                    self.indent -= 1;
                    self.push_output_line(OutputBody::StandaloneText("}".to_owned()));
                    emitted_line = true;
                }
            }
        }

        emitted_line |= self.flush_pending_words(&mut pending_words);
        self.finish_source_line(emitted_line, line.trailing_comment);
        Ok(())
    }

    fn flush_pending_words(&mut self, words: &mut Vec<String>) -> bool {
        if words.is_empty() {
            return false;
        }

        let key = syntax::normalize_key(&words[0]);
        let body = if words.len() == 1 {
            OutputBody::StandaloneText(key)
        } else {
            OutputBody::ActivePair {
                key,
                value: syntax::normalize_value_tokens(&words[1..], self.options.bare_literals),
            }
        };

        words.clear();
        self.push_output_line(body);
        true
    }

    fn finish_source_line(&mut self, emitted_line: bool, comment: Option<String>) {
        match (emitted_line, comment) {
            (true, Some(comment)) => {
                if let Some(line) = self.lines.last_mut() {
                    line.trailing_comment = Some(comment);
                }
            }
            (false, Some(comment)) => self.push_comment_line(comment),
            (false, None) => self.lines.push(OutputLine {
                indent: 0,
                body: OutputBody::Blank,
                trailing_comment: None,
            }),
            (true, None) => {}
        }
    }

    fn push_comment_line(&mut self, comment: String) {
        // With --reflow-comments, comments that look like disabled key/value pairs
        // join the same alignment pass as active pairs.
        if !self.options.reflow_comments {
            self.push_output_line(OutputBody::PlainComment(comment));
            return;
        }

        if let Some(pair) = syntax::parse_disabled_pair(&comment, self.options.bare_literals) {
            self.lines.push(OutputLine {
                indent: self.indent,
                body: OutputBody::DisabledPair {
                    key: pair.key,
                    value: pair.value,
                },
                trailing_comment: pair.trailing_comment,
            });
            return;
        }

        self.push_output_line(OutputBody::PlainComment(comment));
    }

    fn push_output_line(&mut self, body: OutputBody) {
        self.lines.push(OutputLine {
            indent: self.indent,
            body,
            trailing_comment: None,
        });
    }

    fn finish(mut self, had_final_newline: bool) -> Result<String> {
        if self.indent != 0 {
            bail!("unclosed opening brace");
        }

        if had_final_newline {
            self.lines.pop();
        }
        while matches!(
            self.lines.last(),
            Some(OutputLine {
                body: OutputBody::Blank,
                trailing_comment: None,
                ..
            })
        ) {
            self.lines.pop();
        }

        let mut output = render_lines(&self.lines).join("\n");
        if had_final_newline {
            output.push('\n');
        }
        Ok(output)
    }
}

fn render_lines(lines: &[OutputLine]) -> Vec<String> {
    let mut rendered = render_code_columns(lines);
    align_trailing_comments(&mut rendered);
    rendered.into_iter().map(RenderedLine::into_text).collect()
}

fn render_code_columns(lines: &[OutputLine]) -> Vec<RenderedLine> {
    let mut rendered = Vec::with_capacity(lines.len());
    let mut index = 0;

    while index < lines.len() {
        if lines[index].body.is_pair() {
            index = render_pair_group(lines, index, &mut rendered);
        } else {
            rendered.push(render_single_line(&lines[index]));
            index += 1;
        }
    }

    rendered
}

fn render_pair_group(
    lines: &[OutputLine],
    start: usize,
    rendered: &mut Vec<RenderedLine>,
) -> usize {
    let end = find_pair_group_end(lines, start);
    let group = &lines[start..end];

    let widest_key = group
        .iter()
        .map(|line| line.body.pair_parts().visible_key_width())
        .max()
        .unwrap_or(0);

    for line in group {
        let pair = line.body.pair_parts();
        let spaces_after_key = widest_key - pair.visible_key_width() + 1;

        let mut code = indent_text(line.indent, pair.prefix);
        code.push_str(pair.key);
        code.push_str(&" ".repeat(spaces_after_key));
        code.push_str(pair.value);

        rendered.push(RenderedLine {
            indent: line.indent,
            code,
            trailing_comment: line.trailing_comment.clone(),
        });
    }

    end
}

fn find_pair_group_end(lines: &[OutputLine], start: usize) -> usize {
    let indent = lines[start].indent;
    let mut end = start;

    while end < lines.len() && lines[end].indent == indent && lines[end].body.is_pair() {
        end += 1;
    }

    end
}

fn render_single_line(line: &OutputLine) -> RenderedLine {
    let code = match &line.body {
        OutputBody::ActivePair { .. } | OutputBody::DisabledPair { .. } => {
            unreachable!("pairs are rendered in groups")
        }
        OutputBody::StandaloneText(text) | OutputBody::PlainComment(text) => {
            indent_text(line.indent, text)
        }
        OutputBody::Blank => String::new(),
    };

    RenderedLine {
        indent: line.indent,
        code,
        trailing_comment: line.trailing_comment.clone(),
    }
}

fn align_trailing_comments(lines: &mut [RenderedLine]) {
    let mut index = 0;

    while index < lines.len() {
        if !can_align_trailing_comment(&lines[index]) {
            index += 1;
            continue;
        }

        let start = index;
        let end = find_trailing_comment_group_end(lines, start);

        if end - start > 1 {
            pad_code_to_widest_line(&mut lines[start..end]);
        }

        index = end;
    }
}

fn can_align_trailing_comment(line: &RenderedLine) -> bool {
    line.trailing_comment.is_some() && !line.code.is_empty()
}

fn find_trailing_comment_group_end(lines: &[RenderedLine], start: usize) -> usize {
    let indent = lines[start].indent;
    let mut end = start;

    while end < lines.len()
        && lines[end].indent == indent
        && can_align_trailing_comment(&lines[end])
    {
        end += 1;
    }

    end
}

fn pad_code_to_widest_line(lines: &mut [RenderedLine]) {
    let widest_code = lines.iter().map(|line| line.code.len()).max().unwrap_or(0);

    for line in lines {
        line.code
            .push_str(&" ".repeat(widest_code - line.code.len()));
    }
}

impl OutputBody {
    fn is_pair(&self) -> bool {
        matches!(
            self,
            OutputBody::ActivePair { .. } | OutputBody::DisabledPair { .. }
        )
    }

    fn pair_parts(&self) -> PairParts<'_> {
        match self {
            OutputBody::ActivePair { key, value } => PairParts {
                prefix: "",
                key,
                value,
            },
            OutputBody::DisabledPair { key, value } => PairParts {
                prefix: DISABLED_PAIR_PREFIX,
                key,
                value,
            },
            OutputBody::StandaloneText(_) | OutputBody::PlainComment(_) | OutputBody::Blank => {
                unreachable!("only pair-like bodies have pair parts")
            }
        }
    }
}

impl PairParts<'_> {
    fn visible_key_width(&self) -> usize {
        self.prefix.len() + self.key.len()
    }
}

impl RenderedLine {
    fn into_text(mut self) -> String {
        if let Some(comment) = self.trailing_comment {
            if !self.code.is_empty() {
                self.code.push(' ');
            }
            self.code.push_str(&comment);
        }
        self.code
    }
}

fn indent_text(indent: usize, text: &str) -> String {
    let mut line = String::with_capacity(indent * INDENT.len() + text.len());
    for _ in 0..indent {
        line.push_str(INDENT);
    }
    line.push_str(text);
    line
}

#[cfg(test)]
mod tests {
    use super::{Options, format, format_with_options};

    #[test]
    fn formats_indentation_and_key_value_spacing() {
        let input = "\"GameInfo\"\n{\n\tgame                            \"citadel\"\n    hidden_maps\n    {\n\"test_speakers\"         1\n    }\n}\n";

        let expected = "GameInfo\n{\n    game \"citadel\"\n    hidden_maps\n    {\n        test_speakers \"1\"\n    }\n}\n";

        assert_eq!(format(input).unwrap(), expected);
    }

    #[test]
    fn splits_braces_onto_their_own_indented_lines() {
        let input = "\"Root\" { Child { key      value } }";

        let expected = "Root\n{\n    Child\n    {\n        key \"value\"\n    }\n}";

        assert_eq!(format(input).unwrap(), expected);
    }

    #[test]
    fn preserves_blank_lines_and_reindents_comments() {
        let input = "// top\n\"Root\"\n{\n// inner\nkey    value    // tail\n\n}\n";

        let expected = "// top\nRoot\n{\n    // inner\n    key \"value\" // tail\n\n}\n";

        assert_eq!(format(input).unwrap(), expected);
    }

    #[test]
    fn normalizes_newlines() {
        let input = "\"Root\"\r\n{\rkey    value\r\n}\r\n";

        let expected = "Root\n{\n    key \"value\"\n}\n";

        assert_eq!(format(input).unwrap(), expected);
    }

    #[test]
    fn reports_unterminated_quoted_tokens() {
        let err = format("\"Root").unwrap_err();

        assert!(err.to_string().contains("unterminated quoted token"));
    }

    #[test]
    fn reports_unmatched_closing_braces() {
        let err = format("}").unwrap_err();

        assert!(err.to_string().contains("unmatched closing brace"));
    }

    #[test]
    fn reports_unclosed_opening_braces() {
        let err = format("\"Root\"\n{").unwrap_err();

        assert!(err.to_string().contains("unclosed opening brace"));
    }

    #[test]
    fn aligns_values_for_adjacent_pairs() {
        let input = "Root\n{\na 1\nlong_key 2\n\nnext 3\n}\n";

        let expected = "Root\n{\n    a        \"1\"\n    long_key \"2\"\n\n    next \"3\"\n}\n";

        assert_eq!(format(input).unwrap(), expected);
    }

    #[test]
    fn only_quotes_keys_when_required() {
        let input = "\"Root\"\n{\n\"simple\" 1\n\"with space\" 2\n\"has//comment\" 3\n}\n";

        let expected = "Root\n{\n    simple         \"1\"\n    \"with space\"   \"2\"\n    \"has//comment\" \"3\"\n}\n";

        assert_eq!(format(input).unwrap(), expected);
    }

    #[test]
    fn strips_trailing_whitespace_and_blank_line_whitespace() {
        let input = "Root   \n{\nkey value   \n   \n}\n";

        let expected = "Root\n{\n    key \"value\"\n\n}\n";

        assert_eq!(format(input).unwrap(), expected);
    }

    #[test]
    fn strips_extra_blank_lines_at_eof() {
        let input = "Root\n{\nkey value\n}\n\n\n";
        let expected = "Root\n{\n    key \"value\"\n}\n";

        assert_eq!(format(input).unwrap(), expected);

        let input = "Root\n{\nkey value\n}\n\n";
        let expected = "Root\n{\n    key \"value\"\n}\n";

        assert_eq!(format(input).unwrap(), expected);
    }

    #[test]
    fn aligns_comments_for_adjacent_lines() {
        let input = "Root\n{\nx 1 // one\ny 22 // two\n\nz 333 // three\n}\n";

        let expected =
            "Root\n{\n    x \"1\"  // one\n    y \"22\" // two\n\n    z \"333\" // three\n}\n";

        assert_eq!(format(input).unwrap(), expected);
    }

    #[test]
    fn quotes_all_values() {
        let input = "Root\n{\nint \"42\"\nneg \"-7\"\ndecimal \"3.14\"\nleading_decimal \".5\"\nnegative_decimal \"-.5\"\ntrailing_decimal \"1.\"\nyes \"true\"\nno \"false\"\nword bare\ncapital \"True\"\nmany one two\n}\n";

        let expected = "Root\n{\n    int              \"42\"\n    neg              \"-7\"\n    decimal          \"3.14\"\n    leading_decimal  \".5\"\n    negative_decimal \"-.5\"\n    trailing_decimal \"1.\"\n    yes              \"true\"\n    no               \"false\"\n    word             \"bare\"\n    capital          \"True\"\n    many             \"one two\"\n}\n";

        assert_eq!(format(input).unwrap(), expected);
    }

    #[test]
    fn leaves_literal_values_unquoted_when_enabled() {
        let input = "Root\n{\nint \"42\"\nneg -7\ndecimal \"3.14\"\nleading_decimal \".5\"\nnegative_decimal \"-.5\"\ntrailing_decimal \"1.\"\nyes \"true\"\nno false\nword bare\ncapital \"True\"\nmany one two\n}\n";

        let expected = "Root\n{\n    int              42\n    neg              -7\n    decimal          3.14\n    leading_decimal  .5\n    negative_decimal -.5\n    trailing_decimal 1.\n    yes              true\n    no               false\n    word             \"bare\"\n    capital          \"True\"\n    many             \"one two\"\n}\n";

        assert_eq!(
            format_with_options(
                input,
                Options {
                    bare_literals: true,
                    ..Options::default()
                },
            )
            .unwrap(),
            expected
        );
    }

    #[test]
    fn keeps_commented_pairs_unchanged_by_default() {
        let input = "Root\n{\n//FakeReorderPct                           \"0.05\"\n//FakeJitter                               \"low\"\n}\n";

        let expected = "Root\n{\n    //FakeReorderPct                           \"0.05\"\n    //FakeJitter                               \"low\"\n}\n";

        assert_eq!(format(input).unwrap(), expected);
    }

    #[test]
    fn reflows_commented_pairs_when_enabled() {
        let input = "Root\n{\n//FakeReorderPct                           \"0.05\"\n//FakeReorderDelay                         \"10\"\n//FakeJitter                               \"low\"\n// Turning off fake jitter for now\n// IN TESTING\n//- Boot\n//Sound debugging\n//cl_interp \"0.01\" // Client interpolation\n//r_pipeline_stats_flush_before_sleeping true\n//not_a_pair bare\n//\"cl_aggregate_particles\" \"true\"\n}\n";

        let expected = "Root\n{\n    // FakeReorderPct   \"0.05\"\n    // FakeReorderDelay \"10\"\n    // FakeJitter       \"low\"\n    // Turning off fake jitter for now\n    // IN TESTING\n    //- Boot\n    //Sound debugging\n    // cl_interp                              \"0.01\" // Client interpolation\n    // r_pipeline_stats_flush_before_sleeping \"true\"\n    //not_a_pair bare\n    // cl_aggregate_particles \"true\"\n}\n";

        assert_eq!(
            format_with_options(
                input,
                Options {
                    reflow_comments: true,
                    ..Options::default()
                },
            )
            .unwrap(),
            expected
        );
    }

    #[test]
    fn reflowed_comments_share_active_pair_axis() {
        let input = "Root\n{\nactive_key 1\n//disabled_key \"2\"\n//long_disabled_key \"3\"\nnext_key 4\n}\n";

        let expected = "Root\n{\n    active_key           \"1\"\n    // disabled_key      \"2\"\n    // long_disabled_key \"3\"\n    next_key             \"4\"\n}\n";

        assert_eq!(
            format_with_options(
                input,
                Options {
                    reflow_comments: true,
                    ..Options::default()
                },
            )
            .unwrap(),
            expected
        );
    }

    #[test]
    fn bare_literals_apply_to_reflowed_comments() {
        let input =
            "Root\n{\nactive_key \"1\"\n//disabled_key \"2\"\n//flag true\n//name \"bare\"\n}\n";

        let expected = "Root\n{\n    active_key      1\n    // disabled_key 2\n    // flag         true\n    // name         \"bare\"\n}\n";

        assert_eq!(
            format_with_options(
                input,
                Options {
                    reflow_comments: true,
                    bare_literals: true,
                },
            )
            .unwrap(),
            expected
        );
    }
}
