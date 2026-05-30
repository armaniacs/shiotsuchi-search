// Coordinate system: pdfium-render returns bottom-left origin (Y large = page top).
// Sort Y-descending (b.y0.partial_cmp(&a.y0)) for top-to-bottom reading order.

use std::path::Path;

#[derive(Debug, Clone)]
pub struct RawChar {
    pub text: String,
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
    pub font_size: f32,
}

#[derive(Debug, Clone)]
pub struct TextLine {
    pub text: String,
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
    pub font_size: f32,
}

#[derive(Debug, thiserror::Error)]
pub enum PdfError {
    #[error("pdfium init failed: {0}")]
    Init(String),
    #[error("open failed: {0}")]
    Open(String),
}

/// Aggregates PDFium's fine-grained RawChar output into TextLine units.
/// Characters whose y0 difference is within font_size * 0.5 are on the same line.
/// Within each line, characters are kept in left-to-right order (input order preserved).
/// Final lines are sorted Y-descending (bottom-left origin: large y0 = page top = reading first).
pub fn cluster_to_lines(chars: &[RawChar]) -> Vec<TextLine> {
    if chars.is_empty() {
        return vec![];
    }

    let mut lines: Vec<TextLine> = Vec::new();

    // Process chars in input order (left-to-right from PDF)
    for ch in chars {
        // Try to find an existing line within Y-threshold
        let mut found = false;
        for line in &mut lines {
            let y_threshold = line.font_size * 0.5;
            if (ch.y0 - line.y0).abs() <= y_threshold {
                // Same line: append char
                line.text.push_str(&ch.text);
                line.x1 = line.x1.max(ch.x1);
                line.y0 = line.y0.min(ch.y0); // bottom of line (smaller y)
                line.y1 = line.y1.max(ch.y1); // top of line (larger y)
                found = true;
                break;
            }
        }
        if !found {
            // New line
            lines.push(TextLine {
                text: ch.text.clone(),
                x0: ch.x0,
                y0: ch.y0,
                x1: ch.x1,
                y1: ch.y1,
                font_size: ch.font_size,
            });
        }
    }

    // Sort lines Y-descending (top of page first)
    lines.sort_by(|a, b| b.y0.partial_cmp(&a.y0).unwrap_or(std::cmp::Ordering::Equal));
    lines
}

/// Applies XY-Cut layout analysis to TextLines and returns reading-order Markdown text.
/// bottom-left origin: Y-descending (large y0 = page top = reading first).
pub fn xycut_to_text(lines: &[TextLine], page_width: f32) -> String {
    if lines.is_empty() {
        return String::new();
    }

    // Pre-mask: separate full-width lines (>= 80% of page width) from body
    let threshold = page_width * 0.8;
    let mut full_width: Vec<&TextLine> = lines.iter()
        .filter(|l| (l.x1 - l.x0) >= threshold).collect();
    let mut body: Vec<&TextLine> = lines.iter()
        .filter(|l| (l.x1 - l.x0) < threshold).collect();

    // Sort full-width lines Y-descending
    full_width.sort_by(|a, b| b.y0.partial_cmp(&a.y0).unwrap_or(std::cmp::Ordering::Equal));

    let ordered = xycut_recursive(&mut body);

    // Merge full-width titles with body in Y-descending order (page top first)
    let mut result: Vec<&TextLine> = Vec::with_capacity(full_width.len() + ordered.len());
    let mut fw = full_width.iter().peekable();
    let mut ord = ordered.iter().peekable();
    loop {
        match (fw.peek(), ord.peek()) {
            (Some(f), Some(o)) => {
                if f.y0 >= o.y0 { result.push(fw.next().unwrap()); }
                else { result.push(ord.next().unwrap()); }
            }
            (Some(_), None) => { result.extend(fw); break; }
            (None, Some(_)) => { result.extend(ord); break; }
            (None, None) => break,
        }
    }

    let body_font_size = mode_font_size(lines);
    lines_to_markdown(&result, body_font_size)
}

fn xycut_recursive<'a>(lines: &mut Vec<&'a TextLine>) -> Vec<&'a TextLine> {
    if lines.len() <= 1 {
        return lines.clone();
    }

    // Y-descending sort (top of page first)
    lines.sort_by(|a, b| b.y0.partial_cmp(&a.y0).unwrap_or(std::cmp::Ordering::Equal));

    if let Some(split_y) = find_max_gap_y(lines) {
        // top = page top = larger y0 values
        let mut top: Vec<&TextLine> = lines.iter().copied().filter(|l| l.y0 >= split_y).collect();
        // bottom = page bottom = smaller y values
        let mut bottom: Vec<&TextLine> = lines.iter().copied().filter(|l| l.y1 <= split_y).collect();
        let mut r = xycut_recursive(&mut top);
        r.extend(xycut_recursive(&mut bottom));
        return r;
    }

    // Try vertical (X) cut for multi-column detection
    lines.sort_by(|a, b| a.x0.partial_cmp(&b.x0).unwrap_or(std::cmp::Ordering::Equal));

    if let Some(split_x) = find_max_gap_x(lines) {
        let mut left: Vec<&TextLine> = lines.iter().copied().filter(|l| l.x1 <= split_x).collect();
        let mut right: Vec<&TextLine> = lines.iter().copied().filter(|l| l.x0 >= split_x).collect();
        let mut r = xycut_recursive(&mut left);
        r.extend(xycut_recursive(&mut right));
        return r;
    }

    // No split possible: return as-is in Y-descending order
    lines.sort_by(|a, b| b.y0.partial_cmp(&a.y0).unwrap_or(std::cmp::Ordering::Equal));
    lines.clone()
}

/// Find the largest vertical gap between lines (Y-descending sorted input).
/// Returns the midpoint Y of the largest gap if gap >= 2.0pt.
fn find_max_gap_y(desc_sorted: &[&TextLine]) -> Option<f32> {
    let mut max_gap = 0.0_f32;
    let mut split_y = None;
    // In bottom-left origin with Y-descending sort:
    // desc_sorted[0] has the largest y0 (page top)
    // We track the minimum y0 seen so far as we go down the page
    let mut prev_bottom = desc_sorted[0].y0; // will be updated to track lowest y seen
    for l in &desc_sorted[1..] {
        // gap = previous line's bottom edge (y0) - current line's top edge (y1)
        // In bottom-left origin: line's top edge = y1 (larger), bottom edge = y0 (smaller)
        let gap = prev_bottom - l.y1;
        if gap > max_gap {
            max_gap = gap;
            split_y = Some(l.y1 + gap / 2.0);
        }
        prev_bottom = prev_bottom.min(l.y0);
    }
    if max_gap >= 2.0 { split_y } else { None }
}

/// Find the largest horizontal gap between lines (X-ascending sorted input).
fn find_max_gap_x(sorted: &[&TextLine]) -> Option<f32> {
    let mut max_gap = 0.0_f32;
    let mut split_x = None;
    let mut prev_x1 = sorted[0].x1;
    for l in &sorted[1..] {
        let gap = l.x0 - prev_x1;
        if gap > max_gap {
            max_gap = gap;
            split_x = Some(prev_x1 + gap / 2.0);
        }
        prev_x1 = prev_x1.max(l.x1);
    }
    if max_gap >= 5.0 { split_x } else { None }
}

/// Returns the most common font size in the given lines (mode).
fn mode_font_size(lines: &[TextLine]) -> f32 {
    if lines.is_empty() { return 12.0; }
    let mut counts: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
    for l in lines {
        let key = (l.font_size * 2.0).round() as u32;
        *counts.entry(key).or_insert(0) += 1;
    }
    let mode_key = counts.into_iter().max_by_key(|(_, v)| *v).map(|(k, _)| k).unwrap_or(24);
    mode_key as f32 / 2.0
}

/// Converts reading-order TextLines to Markdown using relative font-size ratios.
/// body_size is the mode font size (most common = body text).
/// ratio >= 1.5 → H1, ratio >= 1.2 → H2, otherwise body.
fn lines_to_markdown(lines: &[&TextLine], body_size: f32) -> String {
    let mut parts: Vec<String> = Vec::new();

    for l in lines {
        let text = l.text.trim();
        if text.is_empty() { continue; }

        let ratio = if body_size > 0.0 { l.font_size / body_size } else { 1.0 };
        let is_heading = ratio >= 1.2;

        let formatted = if ratio >= 1.5 {
            format!("# {}", text)
        } else if ratio >= 1.2 {
            format!("## {}", text)
        } else {
            text.to_string()
        };

        if is_heading && !parts.is_empty() {
            parts.push(String::new());
        }
        parts.push(formatted);
        if is_heading {
            parts.push(String::new());
        }
    }

    parts.join("\n")
}

#[cfg(feature = "pdf")]
pub fn extract_text(path: &Path) -> Result<String, PdfError> {
    let pdfium = pdfium_auto::bind_pdfium_silent()
        .map_err(|e| PdfError::Init(e.to_string()))?;

    let doc = pdfium
        .load_pdf_from_file(path, None)
        .map_err(|e| PdfError::Open(e.to_string()))?;

    let mut all_text = String::new();

    for page in doc.pages().iter() {
        let page_width = page.width().value;
        let mut raw_chars: Vec<RawChar> = Vec::new();

        if let Ok(text_page) = page.text() {
            for ch in text_page.chars().iter() {
                let ch_text = ch.unicode_string().unwrap_or_default();
                if ch_text.trim().is_empty() { continue; }
                if let Ok(bounds) = ch.loose_bounds() {
                    raw_chars.push(RawChar {
                        text: ch_text,
                        x0: bounds.left().value,
                        y0: bounds.bottom().value,
                        x1: bounds.right().value,
                        y1: bounds.top().value,
                        font_size: ch.unscaled_font_size().value,
                    });
                }
            }
        }

        if raw_chars.is_empty() { continue; }

        let lines = cluster_to_lines(&raw_chars);
        let page_text = xycut_to_text(&lines, page_width);

        if !page_text.is_empty() {
            if !all_text.is_empty() { all_text.push('\n'); }
            all_text.push_str(&page_text);
        }
    }

    Ok(all_text)
}

#[cfg(not(feature = "pdf"))]
pub fn extract_text(_path: &Path) -> Result<String, PdfError> {
    Ok(String::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(text: &str, x0: f32, y0: f32, x1: f32, y1: f32, font_size: f32) -> RawChar {
        RawChar { text: text.to_string(), x0, y0, x1, y1, font_size }
    }

    #[test]
    fn test_cluster_merges_chars_on_same_line() {
        // bottom-left origin: y0=100 is higher on page than y0=80 (Y large = up)
        // same line: y0 difference <= font_size * 0.5 = 6.0
        let chars = vec![
            raw("H", 10.0, 100.0, 16.0, 112.0, 12.0),
            raw("i", 16.5, 100.5, 20.0, 112.5, 12.0),  // y0 diff 0.5 < 6.0 → same line
            raw("!", 20.5, 100.0, 26.0, 112.0, 12.0),
        ];
        let lines = cluster_to_lines(&chars);
        assert_eq!(lines.len(), 1, "should merge into 1 line, got: {:?}", lines);
        assert_eq!(lines[0].text, "Hi!");
        assert!((lines[0].x0 - 10.0).abs() < 0.1, "x0 should be 10.0");
        assert!((lines[0].x1 - 26.0).abs() < 0.1, "x1 should be 26.0");
    }

    fn tline(text: &str, x0: f32, y0: f32, x1: f32, y1: f32, font_size: f32) -> TextLine {
        TextLine { text: text.to_string(), x0, y0, x1, y1, font_size }
    }

    #[test]
    fn test_xycut_single_column_top_to_bottom() {
        // bottom-left origin: y0=40 is highest, y0=10 is lowest
        // reading order: first (y0=40) → second (y0=25) → third (y0=10)
        let lines = vec![
            tline("first",  10.0, 40.0, 100.0, 50.0, 12.0),
            tline("second", 10.0, 25.0, 100.0, 35.0, 12.0),
            tline("third",  10.0, 10.0, 100.0, 20.0, 12.0),
        ];
        let text = xycut_to_text(&lines, 200.0);
        assert!(text.contains("first") && text.contains("second") && text.contains("third"),
            "should contain all lines, got: {:?}", text);
        let p_first  = text.find("first").unwrap();
        let p_second = text.find("second").unwrap();
        let p_third  = text.find("third").unwrap();
        assert!(p_first < p_second, "first should come before second");
        assert!(p_second < p_third, "second should come before third");
    }

    #[test]
    fn test_xycut_two_column_left_before_right() {
        // bottom-left origin: lines at same Y range, left col x: 0-90, right col x: 110-200
        let lines = vec![
            tline("left1",  0.0,   20.0, 90.0,  30.0, 12.0),
            tline("right1", 110.0, 20.0, 200.0, 30.0, 12.0),
            tline("left2",  0.0,   10.0, 90.0,  20.0, 12.0),
            tline("right2", 110.0, 10.0, 200.0, 20.0, 12.0),
        ];
        let text = xycut_to_text(&lines, 200.0);
        let p_left1  = text.find("left1").unwrap();
        let p_left2  = text.find("left2").unwrap();
        let p_right1 = text.find("right1").unwrap();
        let p_right2 = text.find("right2").unwrap();
        assert!(p_left1  < p_right1, "left1 before right1, got: {:?}", text);
        assert!(p_left2  < p_right1, "left2 before right1, got: {:?}", text);
        assert!(p_right1 < p_right2, "right1 before right2, got: {:?}", text);
    }

    #[test]
    fn test_xycut_full_width_title_comes_first() {
        // bottom-left origin: title y0=90 (page top), body y0=10-40 (page bottom)
        let lines = vec![
            tline("left_body",  0.0,   20.0, 90.0,  40.0, 12.0),
            tline("right_body", 110.0, 20.0, 200.0, 40.0, 12.0),
            tline("title",      0.0,   90.0, 200.0, 100.0, 18.0),  // full-width + large font
        ];
        let text = xycut_to_text(&lines, 200.0);
        let p_title = text.find("title").unwrap();
        let p_body  = text.find("left_body").unwrap();
        assert!(p_title < p_body, "title should come before body, got: {:?}", text);
    }

    #[cfg(feature = "pdf")]
    #[test]
    fn test_extract_text_returns_hello_from_fixture() {
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/hello.pdf");
        let text = extract_text(&fixture).expect("extract_text should not fail");
        assert!(!text.is_empty(), "should extract non-empty text");
        assert!(
            text.contains("Hello"),
            "should contain 'Hello', got: {:?}", text
        );
    }

    #[test]
    fn test_cluster_separates_different_lines() {
        // bottom-left origin: y0=100 and y0=80 differ by 20 > 6.0 → separate lines
        let chars = vec![
            raw("A", 10.0, 100.0, 16.0, 112.0, 12.0),  // upper line (larger y0)
            raw("B", 10.0,  80.0, 16.0,  92.0, 12.0),  // lower line (smaller y0)
        ];
        let lines = cluster_to_lines(&chars);
        assert_eq!(lines.len(), 2, "should be 2 separate lines");
        // Y-descending sort: y0=100 comes first (page top = reading order first)
        assert_eq!(lines[0].text, "A", "upper line (y0=100) should come first");
        assert_eq!(lines[1].text, "B", "lower line (y0=80) should come second");
    }
}
