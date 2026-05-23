//! Markdown writer.
//!
//! Converts a [`DocumentModel`] into a Markdown string.

use std::collections::HashMap;

use s1_model::{AttributeKey, AttributeValue, DocumentModel, ListFormat, NodeId, NodeType};

/// Write a document model to a Markdown string.
pub fn write(doc: &DocumentModel) -> String {
    let body_id = match doc.body_id() {
        Some(id) => id,
        None => return String::new(),
    };

    let mut out = String::new();
    let body = match doc.node(body_id) {
        Some(n) => n,
        None => return String::new(),
    };

    let mut list_counters: HashMap<(u32, u8), u32> = HashMap::new();

    let children: Vec<NodeId> = body.children.clone();
    for (i, &child_id) in children.iter().enumerate() {
        if i > 0 {
            let prev_is_list = children
                .get(i - 1)
                .and_then(|&id| doc.node(id))
                .map(|n| {
                    n.node_type == NodeType::Paragraph
                        && n.attributes.contains(&AttributeKey::ListInfo)
                })
                .unwrap_or(false);
            let cur_is_list = doc
                .node(child_id)
                .map(|n| {
                    n.node_type == NodeType::Paragraph
                        && n.attributes.contains(&AttributeKey::ListInfo)
                })
                .unwrap_or(false);
            if !(prev_is_list && cur_is_list) {
                out.push('\n');
            }
        }
        write_block(doc, child_id, &mut out, &mut list_counters);
    }

    out
}

/// Extract heading level from StyleId (e.g. "Heading1" -> 1).
fn heading_level(style_id: &str) -> Option<u8> {
    style_id.strip_prefix("Heading")?.parse::<u8>().ok()
}

/// Write a block-level node.
fn write_block(
    doc: &DocumentModel,
    node_id: NodeId,
    out: &mut String,
    list_counters: &mut HashMap<(u32, u8), u32>,
) {
    let node = match doc.node(node_id) {
        Some(n) => n,
        None => return,
    };

    match node.node_type {
        NodeType::Paragraph => {
            // Fenced code block: paragraph with StyleId starting with "CodeBlock"
            let is_code_block = node
                .attributes
                .get_string(&AttributeKey::StyleId)
                .map(|s| s.starts_with("CodeBlock"))
                .unwrap_or(false);
            if is_code_block {
                let sid = node
                    .attributes
                    .get_string(&AttributeKey::StyleId)
                    .unwrap_or("CodeBlock");
                let lang = sid.strip_prefix("CodeBlock").unwrap_or("").to_lowercase();
                out.push_str("```");
                out.push_str(&lang);
                out.push('\n');
                // Collect plain text of children, ignoring run formatting
                let mut body = String::new();
                let children: Vec<NodeId> = node.children.clone();
                for &child_id in &children {
                    if let Some(c) = doc.node(child_id) {
                        if c.node_type == NodeType::LineBreak {
                            body.push('\n');
                            continue;
                        }
                    }
                    write_inline_text(doc, child_id, &mut body);
                }
                out.push_str(&body);
                if !body.ends_with('\n') {
                    out.push('\n');
                }
                out.push_str("```\n");
                return;
            }

            // Check for heading via StyleId
            if let Some(style_id) = node.attributes.get_string(&AttributeKey::StyleId) {
                if let Some(level) = heading_level(style_id) {
                    for _ in 0..level {
                        out.push('#');
                    }
                    out.push(' ');
                }
            }

            // Check for list item
            if let Some(AttributeValue::ListInfo(info)) =
                node.attributes.get(&AttributeKey::ListInfo)
            {
                // MD reader sets level = 1 for top-level items (depth from
                // root list). Indent only nested items.
                let depth = info.level.saturating_sub(1) as usize;
                if depth > 0 {
                    out.push_str(&"  ".repeat(depth));
                }
                match info.num_format {
                    ListFormat::Decimal
                    | ListFormat::LowerAlpha
                    | ListFormat::UpperAlpha
                    | ListFormat::LowerRoman
                    | ListFormat::UpperRoman => {
                        let key = (info.num_id, info.level);
                        let counter = list_counters
                            .entry(key)
                            .or_insert_with(|| info.start.unwrap_or(1).saturating_sub(1));
                        *counter += 1;
                        out.push_str(&format!("{}. ", counter));
                    }
                    _ => out.push_str("- "),
                }
            } else {
                // Non-list paragraph resets counters
                list_counters.clear();
            }

            // Check for thematic break (PageBreakBefore on empty paragraph)
            if node.attributes.get_bool(&AttributeKey::PageBreakBefore) == Some(true)
                && node.children.is_empty()
            {
                out.push_str("---\n");
                return;
            }

            // Write inline content — run-aware so shared formatting (e.g.
            // bold spanning three runs with one italic in the middle) emits
            // markers once around the span rather than per-run.
            write_paragraph_runs(doc, node_id, out);
            out.push('\n');
        }

        NodeType::Table => {
            write_table(doc, node_id, out);
        }

        NodeType::TableOfContents => {
            let children: Vec<NodeId> = node.children.clone();
            for &child_id in &children {
                if let Some(child) = doc.node(child_id) {
                    if child.node_type == NodeType::Paragraph {
                        let mut text = String::new();
                        let para_children: Vec<NodeId> = child.children.clone();
                        for &inline_id in &para_children {
                            write_inline_text(doc, inline_id, &mut text);
                        }
                        out.push_str(&text);
                        out.push('\n');
                    }
                }
            }
        }

        NodeType::Section | NodeType::Body | NodeType::Document => {
            let children: Vec<NodeId> = node.children.clone();
            for &child_id in &children {
                write_block(doc, child_id, out, list_counters);
            }
        }

        _ => {
            let mut text = String::new();
            write_inline_text(doc, node_id, &mut text);
            if !text.is_empty() {
                out.push_str(&text);
                out.push('\n');
            }
        }
    }
}

/// Write a paragraph's inline children with run-spanning formatting.
///
/// When several adjacent runs share a formatting attribute (e.g. all bold),
/// the marker is emitted once at the span boundary rather than around each
/// run. This avoids the per-run wrapping that produces broken output like
/// `**bold *****italic***** inside**` for source `**bold *italic* inside**`.
fn write_paragraph_runs(doc: &DocumentModel, para_id: NodeId, out: &mut String) {
    let para = match doc.node(para_id) {
        Some(n) => n,
        None => return,
    };

    // Collect inline items in order, classifying each as a formatted run, a
    // link, a code span, or other (line breaks etc.).
    enum Inline<'a> {
        Run {
            text: String,
            bold: bool,
            italic: bool,
            strike: bool,
            code: bool,
            url: Option<&'a str>,
        },
        LineBreak,
        Image {
            alt: String,
            url: String,
        },
        Other,
    }

    let mut items: Vec<Inline> = Vec::new();
    for &child_id in &para.children {
        let n = match doc.node(child_id) {
            Some(n) => n,
            None => continue,
        };
        match n.node_type {
            NodeType::LineBreak => items.push(Inline::LineBreak),
            NodeType::Image => {
                let alt = n
                    .attributes
                    .get_string(&AttributeKey::ImageAltText)
                    .unwrap_or("")
                    .to_string();
                let url = n
                    .attributes
                    .get_string(&AttributeKey::HyperlinkUrl)
                    .unwrap_or("")
                    .to_string();
                items.push(Inline::Image { alt, url });
            }
            NodeType::Run => {
                let bold = n.attributes.get_bool(&AttributeKey::Bold) == Some(true);
                let italic = n.attributes.get_bool(&AttributeKey::Italic) == Some(true);
                let strike = n.attributes.get_bool(&AttributeKey::Strikethrough) == Some(true);
                let code = n
                    .attributes
                    .get_string(&AttributeKey::FontFamily)
                    .map(|f| f == "monospace")
                    .unwrap_or(false);
                let url = n.attributes.get_string(&AttributeKey::HyperlinkUrl);
                let mut text = String::new();
                for &cid in &n.children {
                    write_inline_text(doc, cid, &mut text);
                }
                if text.is_empty() {
                    continue;
                }
                items.push(Inline::Run {
                    text,
                    bold,
                    italic,
                    strike,
                    code,
                    url,
                });
            }
            _ => items.push(Inline::Other),
        }
    }

    // Markers tracked as a LIFO stack: opening pushes, closing pops in
    // reverse order. This preserves proper Markdown nesting when a span
    // opens before another and must close after.
    #[derive(Clone, Copy, PartialEq)]
    enum Mark {
        Bold,
        Italic,
        Strike,
    }
    fn open_marker(m: Mark, out: &mut String) {
        match m {
            Mark::Bold => out.push_str("**"),
            Mark::Italic => out.push('*'),
            Mark::Strike => out.push_str("~~"),
        }
    }
    fn close_marker(m: Mark, out: &mut String) {
        match m {
            Mark::Bold => out.push_str("**"),
            Mark::Italic => out.push('*'),
            Mark::Strike => out.push_str("~~"),
        }
    }

    let mut stack: Vec<Mark> = Vec::new();
    let close_through = |stack: &mut Vec<Mark>, target: Mark, out: &mut String| {
        // Pop until the target is removed; re-push anything else that
        // shouldn't have been closed. Returns the list that needs reopening.
        let mut reopen: Vec<Mark> = Vec::new();
        while let Some(top) = stack.last().copied() {
            close_marker(top, out);
            stack.pop();
            if top == target {
                break;
            }
            reopen.push(top);
        }
        // Reopen in original order (reopen is in pop order = reverse of open
        // order, so re-push as-is and re-emit; the stack order is preserved
        // because reopen[last] was just below target).
        for m in reopen.into_iter().rev() {
            open_marker(m, out);
            stack.push(m);
        }
    };

    let close_all = |stack: &mut Vec<Mark>, out: &mut String| {
        while let Some(top) = stack.pop() {
            close_marker(top, out);
        }
    };

    let target_set = |bold: bool, italic: bool, strike: bool| -> Vec<Mark> {
        let mut v = Vec::new();
        if bold {
            v.push(Mark::Bold);
        }
        if strike {
            v.push(Mark::Strike);
        }
        if italic {
            v.push(Mark::Italic);
        }
        v
    };

    for item in &items {
        match item {
            Inline::LineBreak => {
                close_all(&mut stack, out);
                out.push_str("  \n");
            }
            Inline::Image { alt, url } => {
                close_all(&mut stack, out);
                out.push_str("![");
                out.push_str(alt);
                out.push_str("](");
                out.push_str(url);
                out.push(')');
            }
            Inline::Other => {}
            Inline::Run {
                text,
                bold,
                italic,
                strike,
                code,
                url,
            } => {
                if url.is_some() || *code {
                    close_all(&mut stack, out);
                    if let Some(href) = url {
                        if *code {
                            out.push('[');
                            out.push('`');
                            out.push_str(text);
                            out.push('`');
                            out.push_str("](");
                            out.push_str(href);
                            out.push(')');
                        } else {
                            out.push('[');
                            push_formatted(out, text, *bold, *italic, *strike);
                            out.push_str("](");
                            out.push_str(href);
                            out.push(')');
                        }
                    } else {
                        out.push('`');
                        out.push_str(text);
                        out.push('`');
                    }
                    continue;
                }

                let target = target_set(*bold, *italic, *strike);

                // Close any open marker that isn't in target.
                let mut to_close: Vec<Mark> = stack
                    .iter()
                    .filter(|m| !target.contains(m))
                    .copied()
                    .collect();
                for m in to_close.drain(..).rev() {
                    close_through(&mut stack, m, out);
                }

                // Open any target marker not already open.
                for m in &target {
                    if !stack.contains(m) {
                        open_marker(*m, out);
                        stack.push(*m);
                    }
                }

                out.push_str(text);
            }
        }
    }

    close_all(&mut stack, out);
}

/// Write inline content with Markdown formatting markers.
fn write_inline(doc: &DocumentModel, node_id: NodeId, out: &mut String) {
    let node = match doc.node(node_id) {
        Some(n) => n,
        None => return,
    };

    match node.node_type {
        NodeType::Text => {
            if let Some(text) = &node.text_content {
                out.push_str(text);
            }
        }
        NodeType::LineBreak => {
            out.push_str("  \n");
        }
        NodeType::Run => {
            let bold = node.attributes.get_bool(&AttributeKey::Bold) == Some(true);
            let italic = node.attributes.get_bool(&AttributeKey::Italic) == Some(true);
            let strike = node.attributes.get_bool(&AttributeKey::Strikethrough) == Some(true);
            let code = node
                .attributes
                .get_string(&AttributeKey::FontFamily)
                .map(|f| f == "monospace")
                .unwrap_or(false);
            let url = node.attributes.get_string(&AttributeKey::HyperlinkUrl);

            // Collect inner text
            let mut inner = String::new();
            let children: Vec<NodeId> = node.children.clone();
            for &child_id in &children {
                write_inline_text(doc, child_id, &mut inner);
            }

            if inner.is_empty() {
                return;
            }

            // Hyperlink wrapping
            if let Some(href) = url {
                if code {
                    out.push('[');
                    out.push('`');
                    out.push_str(&inner);
                    out.push('`');
                    out.push_str("](");
                    out.push_str(href);
                    out.push(')');
                } else {
                    out.push('[');
                    // Apply formatting inside the link text
                    push_formatted(out, &inner, bold, italic, strike);
                    out.push_str("](");
                    out.push_str(href);
                    out.push(')');
                }
            } else if code {
                out.push('`');
                out.push_str(&inner);
                out.push('`');
            } else {
                push_formatted(out, &inner, bold, italic, strike);
            }
        }
        _ => {
            let children: Vec<NodeId> = node.children.clone();
            for &child_id in &children {
                write_inline(doc, child_id, out);
            }
        }
    }
}

/// Push text with bold/italic/strikethrough markers.
fn push_formatted(out: &mut String, text: &str, bold: bool, italic: bool, strike: bool) {
    if bold && italic {
        out.push_str("***");
    } else if bold {
        out.push_str("**");
    } else if italic {
        out.push('*');
    }
    if strike {
        out.push_str("~~");
    }

    out.push_str(text);

    if strike {
        out.push_str("~~");
    }
    if bold && italic {
        out.push_str("***");
    } else if bold {
        out.push_str("**");
    } else if italic {
        out.push('*');
    }
}

/// Extract plain text from inline nodes (no formatting markers).
fn write_inline_text(doc: &DocumentModel, node_id: NodeId, out: &mut String) {
    let node = match doc.node(node_id) {
        Some(n) => n,
        None => return,
    };

    match node.node_type {
        NodeType::Text => {
            if let Some(text) = &node.text_content {
                out.push_str(text);
            }
        }
        _ => {
            let children: Vec<NodeId> = node.children.clone();
            for &child_id in &children {
                write_inline_text(doc, child_id, out);
            }
        }
    }
}

/// Write a table in GFM format.
fn write_table(doc: &DocumentModel, table_id: NodeId, out: &mut String) {
    let table = match doc.node(table_id) {
        Some(n) => n,
        None => return,
    };

    let rows: Vec<NodeId> = table.children.clone();
    // Collect per-column alignment from the header row's cell paragraphs.
    let header_alignments: Vec<Option<s1_model::Alignment>> = rows
        .first()
        .and_then(|&id| doc.node(id))
        .map(|row| {
            row.children
                .iter()
                .map(|&cell_id| {
                    doc.node(cell_id)
                        .and_then(|cell| cell.children.first().copied())
                        .and_then(|para_id| doc.node(para_id))
                        .and_then(|para| para.attributes.get_alignment(&AttributeKey::Alignment))
                })
                .collect()
        })
        .unwrap_or_default();

    for (row_idx, &row_id) in rows.iter().enumerate() {
        let row = match doc.node(row_id) {
            Some(n) => n,
            None => continue,
        };

        let cells: Vec<NodeId> = row.children.clone();
        out.push('|');
        for &cell_id in &cells {
            out.push(' ');
            let mut cell_text = String::new();
            write_cell_inline(doc, cell_id, &mut cell_text);
            let trimmed = cell_text.trim();
            // Escape any '|' inside cell text per GFM spec
            for ch in trimmed.chars() {
                if ch == '|' {
                    out.push('\\');
                    out.push('|');
                } else if ch == '\n' || ch == '\r' {
                    out.push(' ');
                } else {
                    out.push(ch);
                }
            }
            out.push_str(" |");
        }
        out.push('\n');

        // After header row, add alignment-aware separator row.
        if row_idx == 0 {
            out.push('|');
            for idx in 0..cells.len() {
                let sep = match header_alignments.get(idx).copied().flatten() {
                    Some(s1_model::Alignment::Left) => ":---|",
                    Some(s1_model::Alignment::Center) => ":---:|",
                    Some(s1_model::Alignment::Right) => "---:|",
                    _ => "---|",
                };
                out.push_str(sep);
            }
            out.push('\n');
        }
    }
}

/// Render cell content with inline Markdown formatting (bold, italic, links, code).
fn write_cell_inline(doc: &DocumentModel, node_id: NodeId, out: &mut String) {
    let node = match doc.node(node_id) {
        Some(n) => n,
        None => return,
    };

    match node.node_type {
        NodeType::TableCell | NodeType::Paragraph => {
            let children: Vec<NodeId> = node.children.clone();
            for &child_id in &children {
                write_cell_inline(doc, child_id, out);
            }
        }
        NodeType::Run => {
            // Reuse the inline writer's formatting logic by piping it through a
            // local buffer; this preserves bold/italic/code/link markers.
            write_inline(doc, node_id, out);
        }
        NodeType::Text => {
            if let Some(text) = &node.text_content {
                out.push_str(text);
            }
        }
        _ => {
            let children: Vec<NodeId> = node.children.clone();
            for &child_id in &children {
                write_cell_inline(doc, child_id, out);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use s1_model::{ListInfo, Node};

    fn make_para_doc(lines: &[&str]) -> DocumentModel {
        let mut doc = DocumentModel::new();
        let body_id = doc.body_id().unwrap();

        for (i, line) in lines.iter().enumerate() {
            let para_id = doc.next_id();
            doc.insert_node(body_id, i, Node::new(para_id, NodeType::Paragraph))
                .unwrap();

            if !line.is_empty() {
                let run_id = doc.next_id();
                doc.insert_node(para_id, 0, Node::new(run_id, NodeType::Run))
                    .unwrap();
                let text_id = doc.next_id();
                doc.insert_node(run_id, 0, Node::text(text_id, *line))
                    .unwrap();
            }
        }
        doc
    }

    #[test]
    fn write_empty() {
        let doc = DocumentModel::new();
        assert_eq!(write(&doc), "");
    }

    #[test]
    fn write_paragraph() {
        let doc = make_para_doc(&["Hello world"]);
        let md = write(&doc);
        assert!(md.contains("Hello world"));
    }

    #[test]
    fn write_heading_levels() {
        let mut doc = DocumentModel::new();
        let body_id = doc.body_id().unwrap();

        for (i, level) in [1u8, 2, 3].iter().enumerate() {
            let para_id = doc.next_id();
            let mut para = Node::new(para_id, NodeType::Paragraph);
            para.attributes.set(
                AttributeKey::StyleId,
                AttributeValue::String(format!("Heading{}", level)),
            );
            doc.insert_node(body_id, i, para).unwrap();

            let run_id = doc.next_id();
            doc.insert_node(para_id, 0, Node::new(run_id, NodeType::Run))
                .unwrap();
            let text_id = doc.next_id();
            doc.insert_node(run_id, 0, Node::text(text_id, format!("H{}", level)))
                .unwrap();
        }

        let md = write(&doc);
        assert!(md.contains("# H1"));
        assert!(md.contains("## H2"));
        assert!(md.contains("### H3"));
    }

    #[test]
    fn write_bold() {
        let mut doc = DocumentModel::new();
        let body_id = doc.body_id().unwrap();

        let para_id = doc.next_id();
        doc.insert_node(body_id, 0, Node::new(para_id, NodeType::Paragraph))
            .unwrap();

        let run_id = doc.next_id();
        let mut run = Node::new(run_id, NodeType::Run);
        run.attributes
            .set(AttributeKey::Bold, AttributeValue::Bool(true));
        doc.insert_node(para_id, 0, run).unwrap();
        let text_id = doc.next_id();
        doc.insert_node(run_id, 0, Node::text(text_id, "bold"))
            .unwrap();

        assert!(write(&doc).contains("**bold**"));
    }

    #[test]
    fn write_italic() {
        let mut doc = DocumentModel::new();
        let body_id = doc.body_id().unwrap();

        let para_id = doc.next_id();
        doc.insert_node(body_id, 0, Node::new(para_id, NodeType::Paragraph))
            .unwrap();

        let run_id = doc.next_id();
        let mut run = Node::new(run_id, NodeType::Run);
        run.attributes
            .set(AttributeKey::Italic, AttributeValue::Bool(true));
        doc.insert_node(para_id, 0, run).unwrap();
        let text_id = doc.next_id();
        doc.insert_node(run_id, 0, Node::text(text_id, "italic"))
            .unwrap();

        assert!(write(&doc).contains("*italic*"));
    }

    #[test]
    fn write_bold_italic() {
        let mut doc = DocumentModel::new();
        let body_id = doc.body_id().unwrap();

        let para_id = doc.next_id();
        doc.insert_node(body_id, 0, Node::new(para_id, NodeType::Paragraph))
            .unwrap();

        let run_id = doc.next_id();
        let mut run = Node::new(run_id, NodeType::Run);
        run.attributes
            .set(AttributeKey::Bold, AttributeValue::Bool(true));
        run.attributes
            .set(AttributeKey::Italic, AttributeValue::Bool(true));
        doc.insert_node(para_id, 0, run).unwrap();
        let text_id = doc.next_id();
        doc.insert_node(run_id, 0, Node::text(text_id, "both"))
            .unwrap();

        assert!(write(&doc).contains("***both***"));
    }

    #[test]
    fn write_strikethrough() {
        let mut doc = DocumentModel::new();
        let body_id = doc.body_id().unwrap();

        let para_id = doc.next_id();
        doc.insert_node(body_id, 0, Node::new(para_id, NodeType::Paragraph))
            .unwrap();

        let run_id = doc.next_id();
        let mut run = Node::new(run_id, NodeType::Run);
        run.attributes
            .set(AttributeKey::Strikethrough, AttributeValue::Bool(true));
        doc.insert_node(para_id, 0, run).unwrap();
        let text_id = doc.next_id();
        doc.insert_node(run_id, 0, Node::text(text_id, "struck"))
            .unwrap();

        assert!(write(&doc).contains("~~struck~~"));
    }

    #[test]
    fn write_hyperlink() {
        let mut doc = DocumentModel::new();
        let body_id = doc.body_id().unwrap();

        let para_id = doc.next_id();
        doc.insert_node(body_id, 0, Node::new(para_id, NodeType::Paragraph))
            .unwrap();

        let run_id = doc.next_id();
        let mut run = Node::new(run_id, NodeType::Run);
        run.attributes.set(
            AttributeKey::HyperlinkUrl,
            AttributeValue::String("https://example.com".into()),
        );
        doc.insert_node(para_id, 0, run).unwrap();
        let text_id = doc.next_id();
        doc.insert_node(run_id, 0, Node::text(text_id, "Link"))
            .unwrap();

        assert!(write(&doc).contains("[Link](https://example.com)"));
    }

    #[test]
    fn write_unordered_list() {
        let mut doc = DocumentModel::new();
        let body_id = doc.body_id().unwrap();

        for i in 0..2 {
            let para_id = doc.next_id();
            let mut para = Node::new(para_id, NodeType::Paragraph);
            para.attributes.set(
                AttributeKey::ListInfo,
                AttributeValue::ListInfo(ListInfo {
                    level: 1,
                    num_format: ListFormat::Bullet,
                    num_id: 1,
                    start: None,
                }),
            );
            doc.insert_node(body_id, i, para).unwrap();

            let run_id = doc.next_id();
            doc.insert_node(para_id, 0, Node::new(run_id, NodeType::Run))
                .unwrap();
            let text_id = doc.next_id();
            doc.insert_node(run_id, 0, Node::text(text_id, format!("Item {}", i + 1)))
                .unwrap();
        }

        let md = write(&doc);
        assert!(md.contains("- Item 1"));
        assert!(md.contains("- Item 2"));
    }

    #[test]
    fn write_ordered_list() {
        let mut doc = DocumentModel::new();
        let body_id = doc.body_id().unwrap();

        for i in 0..2 {
            let para_id = doc.next_id();
            let mut para = Node::new(para_id, NodeType::Paragraph);
            para.attributes.set(
                AttributeKey::ListInfo,
                AttributeValue::ListInfo(ListInfo {
                    level: 1,
                    num_format: ListFormat::Decimal,
                    num_id: 1,
                    start: None,
                }),
            );
            doc.insert_node(body_id, i, para).unwrap();

            let run_id = doc.next_id();
            doc.insert_node(para_id, 0, Node::new(run_id, NodeType::Run))
                .unwrap();
            let text_id = doc.next_id();
            doc.insert_node(run_id, 0, Node::text(text_id, format!("Item {}", i + 1)))
                .unwrap();
        }

        let md = write(&doc);
        assert!(md.contains("1. Item 1"), "md: {md}");
        assert!(md.contains("2. Item 2"), "md: {md}");
    }

    #[test]
    fn write_table() {
        let mut doc = DocumentModel::new();
        let body_id = doc.body_id().unwrap();

        let table_id = doc.next_id();
        doc.insert_node(body_id, 0, Node::new(table_id, NodeType::Table))
            .unwrap();

        for (row_idx, row_data) in [["A", "B"], ["1", "2"]].iter().enumerate() {
            let row_id = doc.next_id();
            doc.insert_node(table_id, row_idx, Node::new(row_id, NodeType::TableRow))
                .unwrap();
            for (j, text) in row_data.iter().enumerate() {
                let cell_id = doc.next_id();
                doc.insert_node(row_id, j, Node::new(cell_id, NodeType::TableCell))
                    .unwrap();
                let p_id = doc.next_id();
                doc.insert_node(cell_id, 0, Node::new(p_id, NodeType::Paragraph))
                    .unwrap();
                let r_id = doc.next_id();
                doc.insert_node(p_id, 0, Node::new(r_id, NodeType::Run))
                    .unwrap();
                let t_id = doc.next_id();
                doc.insert_node(r_id, 0, Node::text(t_id, *text)).unwrap();
            }
        }

        let md = write(&doc);
        assert!(md.contains("| A | B |"));
        assert!(md.contains("|---|---|"));
        assert!(md.contains("| 1 | 2 |"));
    }

    #[test]
    fn write_line_break() {
        let mut doc = DocumentModel::new();
        let body_id = doc.body_id().unwrap();

        let para_id = doc.next_id();
        doc.insert_node(body_id, 0, Node::new(para_id, NodeType::Paragraph))
            .unwrap();

        let run1_id = doc.next_id();
        doc.insert_node(para_id, 0, Node::new(run1_id, NodeType::Run))
            .unwrap();
        let t1_id = doc.next_id();
        doc.insert_node(run1_id, 0, Node::text(t1_id, "Line 1"))
            .unwrap();

        let br_id = doc.next_id();
        doc.insert_node(para_id, 1, Node::new(br_id, NodeType::LineBreak))
            .unwrap();

        let run2_id = doc.next_id();
        doc.insert_node(para_id, 2, Node::new(run2_id, NodeType::Run))
            .unwrap();
        let t2_id = doc.next_id();
        doc.insert_node(run2_id, 0, Node::text(t2_id, "Line 2"))
            .unwrap();

        assert!(write(&doc).contains("Line 1  \nLine 2"));
    }

    #[test]
    fn write_unicode() {
        let doc = make_para_doc(&["こんにちは", "café"]);
        let md = write(&doc);
        assert!(md.contains("こんにちは"));
        assert!(md.contains("café"));
    }
}
