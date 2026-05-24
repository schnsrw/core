//! Markdown reader using pulldown-cmark.
//!
//! Converts a Markdown string into a [`DocumentModel`].

use pulldown_cmark::{Alignment as CmAlignment, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use s1_model::{
    Alignment, AttributeKey, AttributeMap, AttributeValue, BorderSide, BorderStyle, Borders, Color,
    DocumentModel, ListFormat, ListInfo, Node, NodeId, NodeType, Style, StyleType,
};

use crate::MdError;

/// Read a Markdown string into a [`DocumentModel`].
pub fn read(input: &str) -> Result<DocumentModel, MdError> {
    let mut doc = DocumentModel::new();
    install_default_styles(&mut doc);
    let body_id = doc
        .body_id()
        .ok_or_else(|| MdError::Model("no body".into()))?;

    let opts = Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TABLES
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES;
    let parser = Parser::new_ext(input, opts);

    let mut ctx = ReadContext {
        body_id,
        body_child_index: 0,
        container_stack: Vec::new(),
        bold: false,
        italic: false,
        strikethrough: false,
        code: false,
        link_url: None,
        link_title: None,
        list_stack: Vec::new(),
        numbering_counter: 0,
        in_table: false,
        table_id: None,
        table_row_id: None,
        table_child_index: 0,
        row_child_index: 0,
        cell_para_id: None,
        cell_child_index: 0,
        table_alignments: Vec::new(),
        in_code_block: false,
        blockquote_depth: 0,
    };

    for event in parser {
        process_event(&mut doc, &mut ctx, event)?;
    }

    Ok(doc)
}

struct ReadContext {
    body_id: NodeId,
    body_child_index: usize,
    container_stack: Vec<(NodeId, usize)>,
    bold: bool,
    italic: bool,
    strikethrough: bool,
    code: bool,
    link_url: Option<String>,
    link_title: Option<String>,
    list_stack: Vec<ListState>,
    numbering_counter: u32,
    in_table: bool,
    table_id: Option<NodeId>,
    table_row_id: Option<NodeId>,
    table_child_index: usize,
    row_child_index: usize,
    cell_para_id: Option<NodeId>,
    cell_child_index: usize,
    table_alignments: Vec<CmAlignment>,
    in_code_block: bool,
    blockquote_depth: u32,
}

struct ListState {
    num_id: u32,
    ordered: bool,
}

fn process_event(
    doc: &mut DocumentModel,
    ctx: &mut ReadContext,
    event: Event<'_>,
) -> Result<(), MdError> {
    match event {
        Event::Start(tag) => match tag {
            Tag::Paragraph => {
                if !ctx.in_table {
                    let para_id = doc.next_id();
                    insert_node(
                        doc,
                        ctx.body_id,
                        ctx.body_child_index,
                        para_id,
                        NodeType::Paragraph,
                    )?;
                    if ctx.blockquote_depth > 0 {
                        if let Some(node) = doc.node_mut(para_id) {
                            // Encode blockquote depth in the style ID so DOCX
                            // preserves it via pStyle. e.g. "Quote2".
                            node.attributes.set(
                                AttributeKey::StyleId,
                                AttributeValue::String(format!("Quote{}", ctx.blockquote_depth)),
                            );
                        }
                    }
                    ctx.body_child_index += 1;
                    ctx.container_stack.push((para_id, 0));
                }
            }
            Tag::Heading { level, .. } => {
                let para_id = doc.next_id();
                let mut para = Node::new(para_id, NodeType::Paragraph);
                let level_num = heading_level_to_u8(level);
                para.attributes.set(
                    AttributeKey::StyleId,
                    AttributeValue::String(format!("Heading{}", level_num)),
                );
                doc.insert_node(ctx.body_id, ctx.body_child_index, para)
                    .map_err(|e| MdError::Model(e.to_string()))?;
                ctx.body_child_index += 1;
                ctx.container_stack.push((para_id, 0));
            }
            Tag::Emphasis => {
                ctx.italic = true;
            }
            Tag::Strong => {
                ctx.bold = true;
            }
            Tag::Strikethrough => {
                ctx.strikethrough = true;
            }
            Tag::CodeBlock(kind) => {
                let para_id = doc.next_id();
                insert_node(
                    doc,
                    ctx.body_id,
                    ctx.body_child_index,
                    para_id,
                    NodeType::Paragraph,
                )?;
                if let Some(node) = doc.node_mut(para_id) {
                    // Single canonical paragraph style for all fenced
                    // code blocks; the fence's language hint lives in
                    // the separate CodeLanguage attribute. Earlier
                    // versions encoded language into the styleId (e.g.
                    // "CodeBlockRust"), but that referenced styles
                    // that don't exist in word/styles.xml, leaving the
                    // block unstyled in Word.
                    node.attributes.set(
                        AttributeKey::StyleId,
                        AttributeValue::String("CodeBlock".into()),
                    );
                    if let pulldown_cmark::CodeBlockKind::Fenced(lang) = &kind {
                        if !lang.is_empty() {
                            node.attributes.set(
                                AttributeKey::CodeLanguage,
                                AttributeValue::String(lang.to_string()),
                            );
                        }
                    }
                }
                ctx.body_child_index += 1;
                ctx.container_stack.push((para_id, 0));
                ctx.code = true;
                ctx.in_code_block = true;
            }
            Tag::Link {
                dest_url, title, ..
            } => {
                ctx.link_url = Some(dest_url.to_string());
                ctx.link_title = if title.is_empty() {
                    None
                } else {
                    Some(title.to_string())
                };
            }
            Tag::Image {
                dest_url, title, ..
            } => {
                // Store image reference as attributes on a placeholder image node
                if let Some((parent_id, _)) = ctx.container_stack.last().copied() {
                    let img_id = doc.next_id();
                    let mut img = Node::new(img_id, NodeType::Image);
                    img.attributes.set(
                        AttributeKey::ImageAltText,
                        AttributeValue::String(title.to_string()),
                    );
                    // Store source URL in a generic attribute
                    img.attributes.set(
                        AttributeKey::HyperlinkUrl,
                        AttributeValue::String(dest_url.to_string()),
                    );
                    let child_idx = ctx.container_stack.last().map(|c| c.1).unwrap_or(0);
                    doc.insert_node(parent_id, child_idx, img)
                        .map_err(|e| MdError::Model(e.to_string()))?;
                    if let Some(last) = ctx.container_stack.last_mut() {
                        last.1 += 1;
                    }
                }
            }
            Tag::List(first_item) => {
                ctx.numbering_counter += 1;
                let num_id = ctx.numbering_counter;
                let ordered = first_item.is_some();
                // Register a numbering definition so the DOCX side preserves
                // bullet vs. decimal through round-trip. Without this the
                // DOCX writer emits w:numId references with no abstractNum
                // backing, and re-parse falls back to decimal.
                register_numbering(doc, num_id, ordered);
                ctx.list_stack.push(ListState { num_id, ordered });
            }
            Tag::Item => {
                let para_id = doc.next_id();
                let mut para = Node::new(para_id, NodeType::Paragraph);

                if let Some(list_state) = ctx.list_stack.last() {
                    // 0-based level matches the rest of the codebase: DOCX
                    // <w:ilvl w:val="0"/> for top-level items, TXT writer
                    // uses `level` directly as indent depth. The MD reader
                    // previously emitted 1-based levels, which made the
                    // DOCX writer offset every list one level too deep
                    // (top bullets ended up at 1440 twips / 1″ instead of
                    // 720 twips / 0.5″).
                    let level = (ctx.list_stack.len() as u8).saturating_sub(1);
                    let num_format = if list_state.ordered {
                        ListFormat::Decimal
                    } else {
                        ListFormat::Bullet
                    };
                    para.attributes.set(
                        AttributeKey::ListInfo,
                        AttributeValue::ListInfo(ListInfo {
                            level,
                            num_format,
                            num_id: list_state.num_id,
                            start: None,
                        }),
                    );
                }

                doc.insert_node(ctx.body_id, ctx.body_child_index, para)
                    .map_err(|e| MdError::Model(e.to_string()))?;
                ctx.body_child_index += 1;
                ctx.container_stack.push((para_id, 0));
            }
            Tag::BlockQuote(_) => {
                ctx.container_stack
                    .push((ctx.body_id, ctx.body_child_index));
                ctx.blockquote_depth += 1;
            }
            Tag::HtmlBlock => {
                // Open a paragraph to receive the inner Event::Html text so
                // the block-level HTML survives the round-trip.
                let para_id = doc.next_id();
                insert_node(
                    doc,
                    ctx.body_id,
                    ctx.body_child_index,
                    para_id,
                    NodeType::Paragraph,
                )?;
                ctx.body_child_index += 1;
                ctx.container_stack.push((para_id, 0));
            }
            Tag::FootnoteDefinition(label) => {
                // Emit the definition as a paragraph whose first text run is
                // `[^label]: `. This keeps the markdown source round-tripping
                // even though we don't model footnotes structurally.
                let para_id = doc.next_id();
                insert_node(
                    doc,
                    ctx.body_id,
                    ctx.body_child_index,
                    para_id,
                    NodeType::Paragraph,
                )?;
                ctx.body_child_index += 1;
                ctx.container_stack.push((para_id, 0));

                // Prepend the marker as a regular run so it survives DOCX.
                let run_id = doc.next_id();
                doc.insert_node(para_id, 0, Node::new(run_id, NodeType::Run))
                    .map_err(|e| MdError::Model(e.to_string()))?;
                let text_id = doc.next_id();
                doc.insert_node(run_id, 0, Node::text(text_id, format!("[^{label}]: ")))
                    .map_err(|e| MdError::Model(e.to_string()))?;
                if let Some(last) = ctx.container_stack.last_mut() {
                    last.1 += 1;
                }
            }
            Tag::Table(alignments) => {
                let table_id = doc.next_id();
                insert_node(
                    doc,
                    ctx.body_id,
                    ctx.body_child_index,
                    table_id,
                    NodeType::Table,
                )?;
                // CommonMark / GFM tables have no border syntax, but Word
                // renders unbordered tables as invisible grids — UX-hostile.
                // Force the industry-default 0.5pt black single-line border
                // on all six sides (outer + inside-H + inside-V) so the
                // converted DOCX looks like a normal table when opened.
                if let Some(table_node) = doc.node_mut(table_id) {
                    table_node.attributes.set(
                        AttributeKey::TableBorders,
                        AttributeValue::Borders(default_md_table_borders()),
                    );
                }
                ctx.body_child_index += 1;
                ctx.in_table = true;
                ctx.table_id = Some(table_id);
                ctx.table_child_index = 0;
                ctx.table_alignments = alignments;
            }
            Tag::TableHead => {
                if let Some(table_id) = ctx.table_id {
                    let row_id = doc.next_id();
                    insert_node(
                        doc,
                        table_id,
                        ctx.table_child_index,
                        row_id,
                        NodeType::TableRow,
                    )?;
                    ctx.table_child_index += 1;
                    ctx.table_row_id = Some(row_id);
                    ctx.row_child_index = 0;
                }
            }
            Tag::TableRow => {
                if let Some(table_id) = ctx.table_id {
                    let row_id = doc.next_id();
                    insert_node(
                        doc,
                        table_id,
                        ctx.table_child_index,
                        row_id,
                        NodeType::TableRow,
                    )?;
                    ctx.table_child_index += 1;
                    ctx.table_row_id = Some(row_id);
                    ctx.row_child_index = 0;
                }
            }
            Tag::TableCell => {
                if let Some(row_id) = ctx.table_row_id {
                    let col_idx = ctx.row_child_index;
                    let cell_id = doc.next_id();
                    insert_node(doc, row_id, col_idx, cell_id, NodeType::TableCell)?;
                    ctx.row_child_index += 1;

                    let para_id = doc.next_id();
                    insert_node(doc, cell_id, 0, para_id, NodeType::Paragraph)?;
                    // Apply column alignment (set on header row only)
                    if let Some(cm_align) = ctx.table_alignments.get(col_idx) {
                        let align = match cm_align {
                            CmAlignment::Left => Some(Alignment::Left),
                            CmAlignment::Center => Some(Alignment::Center),
                            CmAlignment::Right => Some(Alignment::Right),
                            CmAlignment::None => None,
                        };
                        if let Some(a) = align {
                            if let Some(node) = doc.node_mut(para_id) {
                                node.attributes
                                    .set(AttributeKey::Alignment, AttributeValue::Alignment(a));
                            }
                        }
                    }
                    ctx.cell_para_id = Some(para_id);
                    ctx.cell_child_index = 0;
                }
            }
            _ => {}
        },

        Event::End(tag_end) => match tag_end {
            TagEnd::Paragraph => {
                if !ctx.in_table {
                    ctx.container_stack.pop();
                }
            }
            TagEnd::Heading(_) => {
                ctx.container_stack.pop();
            }
            TagEnd::Emphasis => {
                ctx.italic = false;
            }
            TagEnd::Strong => {
                ctx.bold = false;
            }
            TagEnd::Strikethrough => {
                ctx.strikethrough = false;
            }
            TagEnd::CodeBlock => {
                ctx.code = false;
                ctx.in_code_block = false;
                ctx.container_stack.pop();
            }
            TagEnd::Link => {
                ctx.link_url = None;
                ctx.link_title = None;
            }
            TagEnd::Image => {}
            TagEnd::List(_) => {
                ctx.list_stack.pop();
            }
            TagEnd::Item => {
                ctx.container_stack.pop();
            }
            TagEnd::BlockQuote(_) => {
                ctx.container_stack.pop();
                if ctx.blockquote_depth > 0 {
                    ctx.blockquote_depth -= 1;
                }
            }
            TagEnd::FootnoteDefinition => {
                ctx.container_stack.pop();
            }
            TagEnd::HtmlBlock => {
                ctx.container_stack.pop();
            }
            TagEnd::Table => {
                if let Some(table_id) = ctx.table_id {
                    apply_content_sized_table_widths(doc, table_id);
                }
                ctx.in_table = false;
                ctx.table_id = None;
            }
            TagEnd::TableHead | TagEnd::TableRow => {
                ctx.table_row_id = None;
            }
            TagEnd::TableCell => {
                ctx.cell_para_id = None;
            }
            _ => {}
        },

        Event::Text(text) => {
            emit_text(doc, ctx, &text)?;
        }

        Event::Html(html) | Event::InlineHtml(html) => {
            // Pass HTML through as literal text so it survives the
            // round-trip. We don't parse or render HTML structurally — by
            // CommonMark default it's an opaque pass-through.
            emit_text(doc, ctx, &html)?;
        }

        Event::TaskListMarker(checked) => {
            // Real Unicode glyphs render in any font; "[x]"/"[ ]" text
            // would round-trip but doesn't look like a checkbox in Word.
            let marker = if checked { "\u{2611} " } else { "\u{2610} " };
            emit_text(doc, ctx, marker)?;
        }

        Event::FootnoteReference(label) => {
            // pulldown-cmark consumes "[^label]" references; emit them back
            // as literal text so they survive the round-trip. We don't model
            // footnotes structurally in MD — DOCX's footnoteReference would
            // be the place to wire them in but that's a deeper change.
            let marker = format!("[^{label}]");
            emit_text(doc, ctx, &marker)?;
        }

        Event::Code(code) => {
            let old_code = ctx.code;
            ctx.code = true;
            emit_text(doc, ctx, &code)?;
            ctx.code = old_code;
        }

        Event::SoftBreak => {
            emit_text(doc, ctx, " ")?;
        }

        Event::HardBreak => {
            if let Some(&(parent_id, child_idx)) = ctx.container_stack.last() {
                let br_id = doc.next_id();
                insert_node(doc, parent_id, child_idx, br_id, NodeType::LineBreak)?;
                if let Some(last) = ctx.container_stack.last_mut() {
                    last.1 += 1;
                }
            }
        }

        Event::Rule => {
            // CommonMark's `---` is a thematic break — a thin horizontal
            // divider, NOT a page break. Use the HorizontalRule paragraph
            // style (thin paragraph with a bottom border, see
            // install_default_styles).
            let para_id = doc.next_id();
            let mut para = Node::new(para_id, NodeType::Paragraph);
            para.attributes.set(
                AttributeKey::StyleId,
                AttributeValue::String("HorizontalRule".into()),
            );
            doc.insert_node(ctx.body_id, ctx.body_child_index, para)
                .map_err(|e| MdError::Model(e.to_string()))?;
            ctx.body_child_index += 1;
        }

        _ => {}
    }

    Ok(())
}

/// Emit text content into the current container as a Run node.
fn emit_text(doc: &mut DocumentModel, ctx: &mut ReadContext, text: &str) -> Result<(), MdError> {
    let (parent_id, child_idx) = if ctx.in_table {
        if let Some(para_id) = ctx.cell_para_id {
            (para_id, ctx.cell_child_index)
        } else {
            return Ok(());
        }
    } else if let Some(&(parent_id, child_idx)) = ctx.container_stack.last() {
        (parent_id, child_idx)
    } else {
        return Ok(());
    };

    let run_id = doc.next_id();
    let mut run = Node::new(run_id, NodeType::Run);
    if ctx.bold {
        run.attributes
            .set(AttributeKey::Bold, AttributeValue::Bool(true));
    }
    if ctx.italic {
        run.attributes
            .set(AttributeKey::Italic, AttributeValue::Bool(true));
    }
    if ctx.strikethrough {
        run.attributes
            .set(AttributeKey::Strikethrough, AttributeValue::Bool(true));
    }
    if ctx.code {
        // Apply the `Code` character style — that style carries the
        // Consolas/10pt + light gray shading so Word renders the run
        // as a recognisable code span. Don't also set FontFamily, since
        // explicit run rPr would override the style's font.
        run.attributes
            .set(AttributeKey::StyleId, AttributeValue::String("Code".into()));
    }
    if let Some(ref url) = ctx.link_url {
        run.attributes.set(
            AttributeKey::HyperlinkUrl,
            AttributeValue::String(url.clone()),
        );
        if let Some(ref title) = ctx.link_title {
            run.attributes.set(
                AttributeKey::HyperlinkTooltip,
                AttributeValue::String(title.clone()),
            );
        }
        // Word renders unstyled hyperlinks as plain black text. Apply
        // the `Hyperlink` character style so the link shows up blue +
        // underlined like users expect — unless we've already applied
        // a more specific style (e.g. `Code` on inline code inside
        // a link, which would be unusual but possible).
        if !run.attributes.contains(&AttributeKey::StyleId) {
            run.attributes.set(
                AttributeKey::StyleId,
                AttributeValue::String("Hyperlink".into()),
            );
        }
    }

    doc.insert_node(parent_id, child_idx, run)
        .map_err(|e| MdError::Model(e.to_string()))?;

    let text_id = doc.next_id();
    doc.insert_node(run_id, 0, Node::text(text_id, text))
        .map_err(|e| MdError::Model(e.to_string()))?;

    if ctx.in_table {
        ctx.cell_child_index += 1;
    } else if let Some(last) = ctx.container_stack.last_mut() {
        last.1 += 1;
    }

    Ok(())
}

fn insert_node(
    doc: &mut DocumentModel,
    parent_id: NodeId,
    child_index: usize,
    node_id: NodeId,
    node_type: NodeType,
) -> Result<(), MdError> {
    doc.insert_node(parent_id, child_index, Node::new(node_id, node_type))
        .map_err(|e| MdError::Model(e.to_string()))
}

/// Stamp body defaults and the full set of styles a Markdown-as-Word
/// document needs so the resulting DOCX opens with Word-friendly
/// formatting on every construct: headings, code, blockquotes,
/// hyperlinks, horizontal rules.
///
/// Each style is the one a Markdown reader would expect Pandoc/Marked
/// to produce, scaled to look right in Word:
///   - `Normal` — body default (Calibri 11pt, 1.15 line, 8pt-after).
///   - `Heading1..6` — bold, sized 18→11pt, before-spacing 24→8pt.
///   - `Code` — inline code character style (Consolas 10pt + shading).
///   - `CodeBlock` — paragraph style for fenced blocks (Consolas 10pt,
///     no spacing, light shading, kept together).
///   - `Quote1..Quote5` — blockquote levels with left indent + bar.
///   - `Hyperlink` — character style for links (blue + underlined).
///   - `HorizontalRule` — thin paragraph with a bottom border.
fn install_default_styles(doc: &mut DocumentModel) {
    {
        let defaults = doc.doc_defaults_mut();
        defaults.font_family.get_or_insert_with(|| "Calibri".into());
        defaults.font_size.get_or_insert(11.0);
        defaults.space_after.get_or_insert(8.0);
        defaults.line_spacing_multiple.get_or_insert(1.15);
    }

    let mut normal = Style::new("Normal", "Normal", StyleType::Paragraph);
    normal.is_default = true;
    doc.set_style(normal);

    // ── Headings ────────────────────────────────────────────────────
    // (level, font_size_pt, space_before_pt, space_after_pt)
    let heading_spec: [(u8, f64, f64, f64); 6] = [
        (1, 18.0, 24.0, 6.0),
        (2, 15.0, 18.0, 6.0),
        (3, 13.0, 12.0, 4.0),
        (4, 12.0, 10.0, 4.0),
        (5, 11.0, 8.0, 2.0),
        (6, 11.0, 8.0, 2.0),
    ];
    for (lvl, size, before, after) in heading_spec {
        let style_id = format!("Heading{lvl}");
        let style_name = format!("heading {lvl}");
        let mut s = Style::new(&style_id, &style_name, StyleType::Paragraph);
        s.parent_id = Some("Normal".into());
        s.next_style_id = Some("Normal".into());
        let mut attrs = AttributeMap::new().bold(true).font_size(size);
        attrs.set(AttributeKey::SpacingBefore, AttributeValue::Float(before));
        attrs.set(AttributeKey::SpacingAfter, AttributeValue::Float(after));
        if lvl >= 5 {
            attrs = attrs.italic(true);
        }
        s.attributes = attrs;
        doc.set_style(s);
    }

    // ── Inline code (character style) ───────────────────────────────
    // Consolas is shipped with every Windows / Office build and most
    // recent macOS versions; falls back gracefully to Courier New.
    let mut code = Style::new("Code", "Code", StyleType::Character);
    code.attributes = AttributeMap::new().font_family("Consolas").font_size(10.0);
    code.attributes.set(
        AttributeKey::HighlightColor,
        AttributeValue::Color(Color::from_hex("F4F4F4").unwrap_or(Color::WHITE)),
    );
    doc.set_style(code);

    // ── Fenced code block (paragraph style) ─────────────────────────
    let mut code_block = Style::new("CodeBlock", "Code Block", StyleType::Paragraph);
    code_block.parent_id = Some("Normal".into());
    code_block.next_style_id = Some("Normal".into());
    let mut cb_attrs = AttributeMap::new().font_family("Consolas").font_size(10.0);
    cb_attrs.set(AttributeKey::SpacingBefore, AttributeValue::Float(6.0));
    cb_attrs.set(AttributeKey::SpacingAfter, AttributeValue::Float(6.0));
    cb_attrs.set(
        AttributeKey::LineSpacing,
        AttributeValue::LineSpacing(s1_model::LineSpacing::Single),
    );
    cb_attrs.set(
        AttributeKey::ParagraphBorders,
        AttributeValue::Borders(Borders {
            top: Some(code_border()),
            bottom: Some(code_border()),
            left: Some(code_border()),
            right: Some(code_border()),
            ..Default::default()
        }),
    );
    code_block.attributes = cb_attrs;
    doc.set_style(code_block);

    // ── Blockquotes ────────────────────────────────────────────────
    // CommonMark nests blockquotes; the MD reader encodes depth into
    // QuoteN. Define styles up through Quote5 (rare to nest deeper).
    for depth in 1u32..=5 {
        let style_id = format!("Quote{depth}");
        let mut q = Style::new(&style_id, &style_id, StyleType::Paragraph);
        q.parent_id = Some("Normal".into());
        q.next_style_id = Some("Normal".into());
        let mut q_attrs = AttributeMap::new().italic(true);
        let indent_pt = 18.0 * depth as f64;
        q_attrs.set(AttributeKey::IndentLeft, AttributeValue::Float(indent_pt));
        q_attrs.set(AttributeKey::SpacingBefore, AttributeValue::Float(6.0));
        q_attrs.set(AttributeKey::SpacingAfter, AttributeValue::Float(6.0));
        // Vertical bar on the left, matching the visual style of a
        // pull-quote in a typical Markdown renderer.
        q_attrs.set(
            AttributeKey::ParagraphBorders,
            AttributeValue::Borders(Borders {
                left: Some(BorderSide {
                    style: BorderStyle::Thick,
                    width: 2.0,
                    color: Color::from_hex("CCCCCC").unwrap_or(Color::BLACK),
                    spacing: 4.0,
                }),
                ..Default::default()
            }),
        );
        q.attributes = q_attrs;
        doc.set_style(q);
    }

    // ── Hyperlinks ─────────────────────────────────────────────────
    let mut link = Style::new("Hyperlink", "Hyperlink", StyleType::Character);
    let mut link_attrs = AttributeMap::new();
    link_attrs.set(
        AttributeKey::Color,
        AttributeValue::Color(Color::from_hex("0563C1").unwrap_or(Color::BLACK)),
    );
    link_attrs.set(
        AttributeKey::Underline,
        AttributeValue::UnderlineStyle(s1_model::UnderlineStyle::Single),
    );
    link.attributes = link_attrs;
    doc.set_style(link);

    // ── Horizontal rule ────────────────────────────────────────────
    // A thin paragraph with a 1pt bottom border draws the divider line
    // CommonMark's `---` is meant to produce. Page-break-before would
    // wipe the page; we emit a divider rule instead.
    let mut hr = Style::new("HorizontalRule", "Horizontal Rule", StyleType::Paragraph);
    hr.parent_id = Some("Normal".into());
    hr.next_style_id = Some("Normal".into());
    let mut hr_attrs = AttributeMap::new().font_size(2.0);
    hr_attrs.set(AttributeKey::SpacingBefore, AttributeValue::Float(6.0));
    hr_attrs.set(AttributeKey::SpacingAfter, AttributeValue::Float(6.0));
    hr_attrs.set(
        AttributeKey::ParagraphBorders,
        AttributeValue::Borders(Borders {
            bottom: Some(BorderSide {
                style: BorderStyle::Single,
                width: 1.0,
                color: Color::from_hex("BFBFBF").unwrap_or(Color::BLACK),
                spacing: 1.0,
            }),
            ..Default::default()
        }),
    );
    hr.attributes = hr_attrs;
    doc.set_style(hr);
}

/// A subtle gray border used to box fenced code blocks.
fn code_border() -> BorderSide {
    BorderSide {
        style: BorderStyle::Single,
        width: 0.5,
        color: Color::from_hex("E0E0E0").unwrap_or(Color::BLACK),
        spacing: 4.0,
    }
}

/// Distribute the page-body width across a Markdown table's columns in
/// proportion to the longest content found in each column. Word still
/// auto-fits at render time, but the initial widths give it a sensible
/// starting layout instead of all-equal columns.
///
/// Sets the table's `TableWidth` to `Auto` (pandoc convention) and
/// stamps a `TableColumnWidths` string in points; the DOCX writer picks
/// these up to emit `<w:tblGrid>` with proportional `<w:gridCol>` widths.
fn apply_content_sized_table_widths(doc: &mut DocumentModel, table_id: NodeId) {
    // Body width on US Letter with 1" margins = 6.5" = 468pt. Sum
    // column widths to this so the table fits a default page.
    const PAGE_BODY_PT: f64 = 468.0;
    const MIN_COL_PT: f64 = 40.0;

    let col_lengths = column_text_lengths(doc, table_id);
    if col_lengths.is_empty() {
        return;
    }
    let total: usize = col_lengths.iter().sum();
    // Ensure every column gets a minimum width so a column of empty
    // cells doesn't collapse to zero in Word.
    let n = col_lengths.len() as f64;
    let min_total = n * MIN_COL_PT;
    let scale_pool = (PAGE_BODY_PT - min_total).max(0.0);
    let weights: Vec<f64> = if total == 0 {
        vec![1.0 / n; col_lengths.len()]
    } else {
        col_lengths
            .iter()
            .map(|&l| l as f64 / total as f64)
            .collect()
    };

    let widths: Vec<String> = weights
        .iter()
        .map(|w| {
            let pts = MIN_COL_PT + w * scale_pool;
            format!("{pts:.1}pt")
        })
        .collect();
    let widths_str = widths.join(",");

    if let Some(table) = doc.node_mut(table_id) {
        table.attributes.set(
            AttributeKey::TableWidth,
            AttributeValue::TableWidth(s1_model::TableWidth::Auto),
        );
        table.attributes.set(
            AttributeKey::TableColumnWidths,
            AttributeValue::String(widths_str),
        );
    }
}

/// Compute the maximum character-count seen in each column across all
/// rows of a table. Returns one entry per column.
fn column_text_lengths(doc: &DocumentModel, table_id: NodeId) -> Vec<usize> {
    let table = match doc.node(table_id) {
        Some(n) => n,
        None => return Vec::new(),
    };

    let mut col_max: Vec<usize> = Vec::new();
    for &row_id in &table.children {
        let row = match doc.node(row_id) {
            Some(n) if n.node_type == NodeType::TableRow => n,
            _ => continue,
        };
        for (col, &cell_id) in row.children.iter().enumerate() {
            if doc.node(cell_id).map(|n| n.node_type) != Some(NodeType::TableCell) {
                continue;
            }
            let len = node_text_char_count(doc, cell_id);
            if col >= col_max.len() {
                col_max.resize(col + 1, 0);
            }
            if len > col_max[col] {
                col_max[col] = len;
            }
        }
    }
    col_max
}

/// Recursive character count across all text descendants of a node.
fn node_text_char_count(doc: &DocumentModel, node_id: NodeId) -> usize {
    let node = match doc.node(node_id) {
        Some(n) => n,
        None => return 0,
    };
    let mut len = node.text_content.as_ref().map_or(0, |t| t.chars().count());
    for &child in &node.children {
        len += node_text_char_count(doc, child);
    }
    len
}

/// Default border decoration applied to every Markdown table on the way
/// into DOCX. GFM tables don't carry border info, but unbordered tables
/// render as invisible grids in Word — visually broken. A thin single
/// black line on all six edges matches what almost every Word table
/// uses by default.
fn default_md_table_borders() -> Borders {
    let side = BorderSide {
        style: BorderStyle::Single,
        width: 0.5,
        color: Color::BLACK,
        spacing: 0.0,
    };
    Borders {
        top: Some(side.clone()),
        bottom: Some(side.clone()),
        left: Some(side.clone()),
        right: Some(side.clone()),
        inside_h: Some(side.clone()),
        inside_v: Some(side),
    }
}

#[cfg(test)]
mod md_table_border_tests {
    use super::*;

    #[test]
    fn md_table_carries_default_borders() {
        let md = "| A | B |\n|---|---|\n| 1 | 2 |\n";
        let doc = read(md).unwrap();
        let body_id = doc.body_id().unwrap();
        let body = doc.node(body_id).unwrap();
        let table_id = body
            .children
            .iter()
            .find(|&&id| doc.node(id).map(|n| n.node_type) == Some(NodeType::Table))
            .copied()
            .expect("table");
        let table = doc.node(table_id).unwrap();
        let borders = match table.attributes.get(&AttributeKey::TableBorders) {
            Some(AttributeValue::Borders(b)) => b,
            _ => panic!("expected TableBorders on MD table"),
        };
        assert!(borders.top.is_some(), "top");
        assert!(borders.bottom.is_some(), "bottom");
        assert!(borders.left.is_some(), "left");
        assert!(borders.right.is_some(), "right");
        assert!(borders.inside_h.is_some(), "insideH");
        assert!(borders.inside_v.is_some(), "insideV");
        let top = borders.top.as_ref().unwrap();
        assert_eq!(top.color, Color::BLACK);
        assert_eq!(top.style, BorderStyle::Single);
    }
}

/// Register a numbering definition (abstract + instance) for a Markdown list.
fn register_numbering(doc: &mut DocumentModel, num_id: u32, ordered: bool) {
    use s1_model::{AbstractNumbering, NumberingInstance, NumberingLevel};

    let abstract_num_id = num_id; // simplest 1:1 mapping
    let format = if ordered {
        ListFormat::Decimal
    } else {
        ListFormat::Bullet
    };

    let mut levels = Vec::new();
    for lvl in 0..9u8 {
        levels.push(NumberingLevel {
            level: lvl,
            num_format: format,
            level_text: if ordered {
                format!("%{}.", lvl + 1)
            } else {
                "\u{2022}".into()
            },
            start: 1,
            indent_left: Some(36.0 * (lvl as f64 + 1.0)),
            indent_hanging: Some(18.0),
            alignment: None,
            bullet_font: if ordered { None } else { Some("Symbol".into()) },
        });
    }

    let numbering = doc.numbering_mut();
    numbering.abstract_nums.push(AbstractNumbering {
        abstract_num_id,
        name: None,
        levels,
    });
    numbering.instances.push(NumberingInstance {
        num_id,
        abstract_num_id,
        level_overrides: vec![],
    });
}

fn heading_level_to_u8(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_empty() {
        let doc = read("").unwrap();
        assert_eq!(doc.to_plain_text(), "");
    }

    #[test]
    fn read_single_paragraph() {
        let doc = read("Hello world").unwrap();
        assert_eq!(doc.to_plain_text(), "Hello world");
    }

    #[test]
    fn read_multiple_paragraphs() {
        let doc = read("First\n\nSecond\n\nThird").unwrap();
        let text = doc.to_plain_text();
        assert!(text.contains("First"));
        assert!(text.contains("Second"));
        assert!(text.contains("Third"));
    }

    #[test]
    fn read_heading_levels() {
        let doc = read("# H1\n\n## H2\n\n### H3").unwrap();
        let body_id = doc.body_id().unwrap();
        let body = doc.node(body_id).unwrap();

        let h1 = doc.node(body.children[0]).unwrap();
        assert_eq!(
            h1.attributes.get_string(&AttributeKey::StyleId),
            Some("Heading1")
        );

        let h2 = doc.node(body.children[1]).unwrap();
        assert_eq!(
            h2.attributes.get_string(&AttributeKey::StyleId),
            Some("Heading2")
        );
    }

    #[test]
    fn read_bold() {
        let doc = read("**bold text**").unwrap();
        let body_id = doc.body_id().unwrap();
        let body = doc.node(body_id).unwrap();
        let para = doc.node(body.children[0]).unwrap();
        let run = doc.node(para.children[0]).unwrap();
        assert_eq!(run.attributes.get_bool(&AttributeKey::Bold), Some(true));
    }

    #[test]
    fn read_italic() {
        let doc = read("*italic text*").unwrap();
        let body_id = doc.body_id().unwrap();
        let body = doc.node(body_id).unwrap();
        let para = doc.node(body.children[0]).unwrap();
        let run = doc.node(para.children[0]).unwrap();
        assert_eq!(run.attributes.get_bool(&AttributeKey::Italic), Some(true));
    }

    #[test]
    fn read_bold_italic() {
        let doc = read("***bold italic***").unwrap();
        let body_id = doc.body_id().unwrap();
        let body = doc.node(body_id).unwrap();
        let para = doc.node(body.children[0]).unwrap();
        let run = doc.node(para.children[0]).unwrap();
        assert_eq!(run.attributes.get_bool(&AttributeKey::Bold), Some(true));
        assert_eq!(run.attributes.get_bool(&AttributeKey::Italic), Some(true));
    }

    #[test]
    fn read_strikethrough() {
        let doc = read("~~struck~~").unwrap();
        let body_id = doc.body_id().unwrap();
        let body = doc.node(body_id).unwrap();
        let para = doc.node(body.children[0]).unwrap();
        let run = doc.node(para.children[0]).unwrap();
        assert_eq!(
            run.attributes.get_bool(&AttributeKey::Strikethrough),
            Some(true)
        );
    }

    #[test]
    fn read_inline_code() {
        let doc = read("`code`").unwrap();
        let body_id = doc.body_id().unwrap();
        let body = doc.node(body_id).unwrap();
        let para = doc.node(body.children[0]).unwrap();
        let run = doc.node(para.children[0]).unwrap();
        // The MD reader tags inline-code runs with the `Code` character
        // style; the actual monospace font + light shading live on the
        // style definition, not on the run, so styles.xml stays the
        // single source of truth for code formatting.
        assert_eq!(
            run.attributes.get_string(&AttributeKey::StyleId),
            Some("Code")
        );
    }

    #[test]
    fn read_code_block() {
        let doc = read("```\nfn main() {}\n```").unwrap();
        let text = doc.to_plain_text();
        assert!(text.contains("fn main()"));
    }

    #[test]
    fn read_hyperlink() {
        let doc = read("[Click here](https://example.com)").unwrap();
        let body_id = doc.body_id().unwrap();
        let body = doc.node(body_id).unwrap();
        let para = doc.node(body.children[0]).unwrap();
        let run = doc.node(para.children[0]).unwrap();
        assert_eq!(
            run.attributes.get_string(&AttributeKey::HyperlinkUrl),
            Some("https://example.com")
        );
        assert_eq!(doc.to_plain_text(), "Click here");
    }

    #[test]
    fn read_unordered_list() {
        let doc = read("- Item 1\n- Item 2").unwrap();
        let body_id = doc.body_id().unwrap();
        let body = doc.node(body_id).unwrap();
        assert!(body.children.len() >= 2);

        let item1 = doc.node(body.children[0]).unwrap();
        match item1.attributes.get(&AttributeKey::ListInfo) {
            Some(AttributeValue::ListInfo(info)) => {
                assert_eq!(info.num_format, ListFormat::Bullet);
            }
            other => panic!("Expected ListInfo, got {:?}", other),
        }
    }

    #[test]
    fn read_ordered_list() {
        let doc = read("1. First\n2. Second").unwrap();
        let body_id = doc.body_id().unwrap();
        let body = doc.node(body_id).unwrap();

        let item1 = doc.node(body.children[0]).unwrap();
        match item1.attributes.get(&AttributeKey::ListInfo) {
            Some(AttributeValue::ListInfo(info)) => {
                assert_eq!(info.num_format, ListFormat::Decimal);
            }
            other => panic!("Expected ListInfo, got {:?}", other),
        }
    }

    #[test]
    fn read_nested_list() {
        // Levels are 0-based: top-level items have level=0, the first
        // nested level has level=1.
        let doc = read("- Outer\n  - Inner").unwrap();
        let body_id = doc.body_id().unwrap();
        let body = doc.node(body_id).unwrap();

        let mut found_top = false;
        let mut found_nested = false;
        for &child_id in &body.children {
            if let Some(node) = doc.node(child_id) {
                if let Some(AttributeValue::ListInfo(info)) =
                    node.attributes.get(&AttributeKey::ListInfo)
                {
                    if info.level == 0 {
                        found_top = true;
                    }
                    if info.level >= 1 {
                        found_nested = true;
                    }
                }
            }
        }
        assert!(found_top, "expected a top-level item at level 0");
        assert!(found_nested, "expected a nested item at level >= 1");
    }

    #[test]
    fn read_gfm_table() {
        let doc = read("| A | B |\n|---|---|\n| 1 | 2 |").unwrap();
        let body_id = doc.body_id().unwrap();
        let body = doc.node(body_id).unwrap();

        let table = doc.node(body.children[0]).unwrap();
        assert_eq!(table.node_type, NodeType::Table);
        assert!(table.children.len() >= 2);
    }

    #[test]
    fn read_line_break() {
        let doc = read("Line 1  \nLine 2").unwrap();
        let body_id = doc.body_id().unwrap();
        let body = doc.node(body_id).unwrap();
        let para = doc.node(body.children[0]).unwrap();
        let has_break = para.children.iter().any(|&id| {
            doc.node(id)
                .map(|n| n.node_type == NodeType::LineBreak)
                .unwrap_or(false)
        });
        assert!(has_break, "Expected a LineBreak node");
    }

    #[test]
    fn read_thematic_break() {
        let doc = read("Before\n\n---\n\nAfter").unwrap();
        let body_id = doc.body_id().unwrap();
        let body = doc.node(body_id).unwrap();

        // The thematic break is marked with the HorizontalRule style so
        // the DOCX writer emits a thin bottom-border paragraph (and the
        // MD writer round-trips it back to `---`). Earlier versions used
        // PageBreakBefore, which actually made Word force-break to a
        // new page — visually broken.
        let has_rule = body.children.iter().any(|&id| {
            doc.node(id)
                .map(|n| n.attributes.get_string(&AttributeKey::StyleId) == Some("HorizontalRule"))
                .unwrap_or(false)
        });
        assert!(has_rule, "Expected a HorizontalRule paragraph");
    }

    #[test]
    fn read_mixed_formatting() {
        let doc = read("Normal **bold** and *italic*").unwrap();
        let text = doc.to_plain_text();
        assert!(text.contains("Normal"));
        assert!(text.contains("bold"));
        assert!(text.contains("italic"));
    }

    #[test]
    fn read_unicode() {
        let doc = read("こんにちは **世界**").unwrap();
        let text = doc.to_plain_text();
        assert!(text.contains("こんにちは"));
        assert!(text.contains("世界"));
    }
}
