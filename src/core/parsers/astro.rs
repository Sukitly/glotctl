use anyhow::Result;
use std::sync::Arc;
use swc_common::SourceMap;

use crate::core::parsers::jsx::{DetachedComment, ParsedJSX, parse_module_source};

/// Parse an Astro source file by converting its frontmatter and template into TSX.
///
/// This blanks Astro-only syntax that SWC cannot parse directly (frontmatter
/// delimiters, HTML comments, and raw script/style contents), then wraps the
/// template in a fragment so the existing TSX pipeline can analyze it.
pub fn parse_astro_source(
    code: String,
    file_path: &str,
    source_map: Arc<SourceMap>,
) -> Result<ParsedJSX> {
    let transformed = transform_astro_to_tsx(&code);
    let mut parsed = parse_module_source(transformed.code, file_path, source_map, true, "astro")?;
    parsed.comments.detached = transformed.detached_comments;
    parsed.astro_template_start_line = Some(transformed.template_start_line);
    Ok(parsed)
}

struct AstroTransformResult {
    code: String,
    template_start_line: usize,
    detached_comments: Vec<DetachedComment>,
}

struct SplitAstroSource {
    prelude: String,
    template: String,
}

fn transform_astro_to_tsx(source: &str) -> AstroTransformResult {
    let split = split_frontmatter(source);
    let template_start_line = split.prelude.lines().count() + 1;
    let (sanitized_template, detached_comments) =
        sanitize_template(&split.template, template_start_line);

    AstroTransformResult {
        code: format!(
            "{}{}",
            split.prelude,
            wrap_template_in_fragment(&sanitized_template)
        ),
        template_start_line,
        detached_comments,
    }
}

fn wrap_template_in_fragment(template: &str) -> String {
    if let Some(stripped) = template.strip_suffix("\r\n") {
        format!("<>{}</>\r\n", stripped)
    } else if let Some(stripped) = template.strip_suffix('\n') {
        format!("<>{}</>\n", stripped)
    } else {
        format!("<>{}</>", template)
    }
}

fn split_frontmatter(source: &str) -> SplitAstroSource {
    let mut lines = source.split_inclusive('\n');
    let Some(first_line) = lines.next() else {
        return SplitAstroSource {
            prelude: String::new(),
            template: String::new(),
        };
    };

    if trim_line_ending(first_line) != "---" {
        return SplitAstroSource {
            prelude: String::new(),
            template: source.to_string(),
        };
    }

    let mut prelude = blank_preserve_bytes(first_line);
    let mut consumed = first_line.len();

    for line in lines {
        consumed += line.len();

        if trim_line_ending(line) == "---" {
            prelude.push_str(&blank_preserve_bytes(line));
            return SplitAstroSource {
                prelude,
                template: source[consumed..].to_string(),
            };
        }

        prelude.push_str(line);
    }

    SplitAstroSource {
        prelude: String::new(),
        template: source.to_string(),
    }
}

fn trim_line_ending(line: &str) -> &str {
    line.trim_end_matches(['\r', '\n'])
}

fn sanitize_template(template: &str, base_line: usize) -> (String, Vec<DetachedComment>) {
    let mut result = String::with_capacity(template.len());
    let mut detached_comments = Vec::new();
    let mut index = 0;
    let mut current_line = base_line;

    while index < template.len() {
        let rest = &template[index..];

        if rest.starts_with("<!--") {
            let end = rest
                .find("-->")
                .map(|pos| index + pos + 3)
                .unwrap_or(template.len());
            let comment = &template[index..end];
            if let Some(inner) = comment
                .strip_prefix("<!--")
                .and_then(|text| text.strip_suffix("-->"))
            {
                detached_comments.push(DetachedComment {
                    line: current_line,
                    text: inner.trim().to_string(),
                });
            }
            result.push_str(&blank_preserve_bytes(comment));
            current_line += count_newlines(comment);
            index = end;
            continue;
        }

        if rest.starts_with("<!") {
            let end = rest
                .find('>')
                .map(|pos| index + pos + 1)
                .unwrap_or(template.len());
            let chunk = &template[index..end];
            result.push_str(&blank_preserve_bytes(chunk));
            current_line += count_newlines(chunk);
            index = end;
            continue;
        }

        if let Some(tag_name) = raw_text_tag_name(rest)
            && let Some(open_rel) = rest.find('>')
        {
            let open_end = index + open_rel + 1;
            let opening = &template[index..open_end];
            let closing_tag = format!("</{}>", tag_name);

            if let Some(close_rel) = template[open_end..].find(&closing_tag) {
                let close_start = open_end + close_rel;
                let raw_body = &template[open_end..close_start];
                result.push_str(opening);
                result.push_str(&blank_preserve_bytes(raw_body));
                current_line += count_newlines(opening);
                current_line += count_newlines(raw_body);
                index = close_start;
                continue;
            }
        }

        let ch = rest.chars().next().expect("slice should not be empty");
        result.push(ch);
        if ch == '\n' {
            current_line += 1;
        }
        index += ch.len_utf8();
    }

    (result, detached_comments)
}

fn raw_text_tag_name(rest: &str) -> Option<&'static str> {
    if rest.starts_with("<script") {
        Some("script")
    } else if rest.starts_with("<style") {
        Some("style")
    } else {
        None
    }
}

fn blank_preserve_bytes(source: &str) -> String {
    let mut bytes = source.as_bytes().to_vec();
    for byte in &mut bytes {
        if *byte != b'\n' && *byte != b'\r' {
            *byte = b' ';
        }
    }
    String::from_utf8(bytes).expect("blanked source should stay valid UTF-8")
}

fn count_newlines(source: &str) -> usize {
    source.bytes().filter(|byte| *byte == b'\n').count()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use swc_common::SourceMap;

    use super::{parse_astro_source, transform_astro_to_tsx};

    #[test]
    fn test_transform_astro_preserves_line_count() {
        let source = r#"---
const value = 1;
---
<div>{value}</div>
"#;

        let transformed = transform_astro_to_tsx(source);

        assert_eq!(transformed.code.lines().count(), source.lines().count());
        assert!(transformed.code.contains("const value = 1;"));
        assert!(transformed.code.contains("<><div>{value}</div>"));
        assert_eq!(transformed.template_start_line, 4);
    }

    #[test]
    fn test_parse_astro_with_html_comment_and_script() {
        let source = r#"---
const value = 1;
---
<!-- comment -->
<div>{value}</div>
<script>
  const node = document.querySelector<HTMLDivElement>("div");
</script>
"#;

        let parsed = parse_astro_source(
            source.to_string(),
            "component.astro",
            Arc::new(SourceMap::default()),
        )
        .unwrap();

        assert_eq!(parsed.astro_template_start_line, Some(4));
        assert_eq!(parsed.comments.detached.len(), 1);
        assert_eq!(parsed.comments.detached[0].line, 4);
        assert_eq!(parsed.comments.detached[0].text, "comment");
    }
}
