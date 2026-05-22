/// Best-effort EMF (Enhanced Metafile) → SVG transcoder.
///
/// Covers the record types that appear in DOCX fixtures:
/// geometric primitives (lines, ellipses, rectangles), GDI object
/// management (pens, brushes, fonts), text, and embedded DIB bitmaps.
/// Unknown records are silently skipped so parsing never fails hard.
///
/// Returns `None` only if the bytes don't start with the EMF magic
/// `01 00 00 00` or if the header is too short to be valid.

use std::collections::HashMap;

// ── EMR record type constants ──────────────────────────────────────────────

const EMR_HEADER: u32 = 1;
const EMR_EOF: u32 = 14;
const EMR_SETBKMODE: u32 = 18;
const EMR_SETTEXTALIGN: u32 = 22;
const EMR_SETTEXTCOLOR: u32 = 24;
const EMR_SETBKCOLOR: u32 = 25;
const EMR_MOVETOEX: u32 = 27;
const EMR_INTERSECTCLIPRECT: u32 = 30;
const EMR_SETWORLDTRANSFORM: u32 = 35;
const EMR_MODIFYWORLDTRANSFORM: u32 = 36;
const EMR_SELECTOBJECT: u32 = 37;
const EMR_CREATEPEN: u32 = 38;
const EMR_CREATEBRUSHINDIRECT: u32 = 39;
const EMR_DELETEOBJECT: u32 = 40;
const EMR_ANGLEARC: u32 = 41;
const EMR_ELLIPSE: u32 = 42;
const EMR_RECTANGLE: u32 = 43;
const EMR_BEGINPATH: u32 = 59;
const EMR_ENDPATH: u32 = 60;
const EMR_CLOSEFIGURE: u32 = 61;
const EMR_FILLPATH: u32 = 62;
const EMR_STROKEANDFILLPATH: u32 = 63;
const EMR_STROKEPATH: u32 = 64;
const EMR_BITBLT: u32 = 76;
const EMR_STRETCHBLT: u32 = 77;
const EMR_STRETCHDIBITS: u32 = 81;
const EMR_EXTCREATEFONTINDIRECTW: u32 = 82;
const EMR_EXTTEXTOUTA: u32 = 83;
const EMR_EXTTEXTOUTW: u32 = 84;
const EMR_POLYLINE16: u32 = 87;
const EMR_POLYBEZIERTO16: u32 = 88;
const EMR_POLYLINETO16: u32 = 89;
const EMR_POLYPOLYGON16: u32 = 91;
const EMR_EXTCREATEPEN: u32 = 95;
const EMR_LINETO: u32 = 54;

// Stock object handles (>= 0x80000000)
const STOCK_WHITE_BRUSH: u32 = 0x80000000;
const STOCK_LTGRAY_BRUSH: u32 = 0x80000001;
const STOCK_GRAY_BRUSH: u32 = 0x80000002;
const STOCK_BLACK_BRUSH: u32 = 0x80000004;
const STOCK_NULL_BRUSH: u32 = 0x80000005;
const STOCK_WHITE_PEN: u32 = 0x80000006;
const STOCK_BLACK_PEN: u32 = 0x80000007;
const STOCK_NULL_PEN: u32 = 0x80000008;

// ── GDI object table ───────────────────────────────────────────────────────

#[derive(Clone)]
enum GdiObject {
    Pen {
        style: u32, // PS_SOLID=0 PS_DASH=1 PS_DOT=2 PS_NULL=5
        width: u32, // logical units
        colorref: u32,
    },
    Brush {
        style: u32, // BS_SOLID=0 BS_NULL=1
        colorref: u32,
    },
    Font {
        face: String,
        height: i32, // negative = char height in logical units
        bold: bool,
        italic: bool,
    },
    Empty,
}

impl Default for GdiObject {
    fn default() -> Self {
        Self::Empty
    }
}

fn stock_pen(handle: u32) -> GdiObject {
    match handle {
        STOCK_WHITE_PEN => GdiObject::Pen { style: 0, width: 1, colorref: 0x00FFFFFF },
        STOCK_NULL_PEN => GdiObject::Pen { style: 5, width: 0, colorref: 0 },
        _ => GdiObject::Pen { style: 0, width: 1, colorref: 0x00000000 }, // BLACK_PEN
    }
}

fn stock_brush(handle: u32) -> GdiObject {
    match handle {
        STOCK_WHITE_BRUSH | STOCK_LTGRAY_BRUSH => {
            GdiObject::Brush { style: 0, colorref: 0x00FFFFFF }
        }
        STOCK_GRAY_BRUSH => GdiObject::Brush { style: 0, colorref: 0x00808080 },
        STOCK_BLACK_BRUSH => GdiObject::Brush { style: 0, colorref: 0x00000000 },
        STOCK_NULL_BRUSH => GdiObject::Brush { style: 1, colorref: 0 },
        _ => GdiObject::Brush { style: 1, colorref: 0 },
    }
}

/// COLORREF (0x00BBGGRR) → CSS "#rrggbb"
fn colorref_to_css(c: u32) -> String {
    let r = c & 0xFF;
    let g = (c >> 8) & 0xFF;
    let b = (c >> 16) & 0xFF;
    format!("#{r:02x}{g:02x}{b:02x}")
}

// ── Drawing state ──────────────────────────────────────────────────────────

struct EmfState {
    objects: HashMap<u32, GdiObject>,
    pen_idx: u32,
    brush_idx: u32,
    text_color: u32,
    bk_color: u32,
    bk_mode: u32, // 1=TRANSPARENT 2=OPAQUE
    cur_x: f64,
    cur_y: f64,
    // Accumulated SVG elements
    elements: Vec<String>,
    // Path accumulation for path-bracket records
    in_path: bool,
    path_cmds: Vec<String>,
    // Embedded bitmaps: (data, width, height) in order found
    bitmaps: Vec<Vec<u8>>,
}

impl EmfState {
    fn new() -> Self {
        let mut s = Self {
            objects: HashMap::new(),
            pen_idx: STOCK_BLACK_PEN,
            brush_idx: STOCK_NULL_BRUSH,
            text_color: 0,
            bk_color: 0x00FFFFFF,
            bk_mode: 2,
            cur_x: 0.0,
            cur_y: 0.0,
            elements: Vec::new(),
            in_path: false,
            path_cmds: Vec::new(),
            bitmaps: Vec::new(),
        };
        // Pre-populate stock objects
        s.objects.insert(STOCK_WHITE_BRUSH, GdiObject::Brush { style: 0, colorref: 0x00FFFFFF });
        s.objects.insert(STOCK_LTGRAY_BRUSH, GdiObject::Brush { style: 0, colorref: 0x00C0C0C0 });
        s.objects.insert(STOCK_GRAY_BRUSH, GdiObject::Brush { style: 0, colorref: 0x00808080 });
        s.objects.insert(STOCK_BLACK_BRUSH, GdiObject::Brush { style: 0, colorref: 0x00000000 });
        s.objects.insert(STOCK_NULL_BRUSH, GdiObject::Brush { style: 1, colorref: 0 });
        s.objects.insert(STOCK_WHITE_PEN, GdiObject::Pen { style: 0, width: 1, colorref: 0x00FFFFFF });
        s.objects.insert(STOCK_BLACK_PEN, GdiObject::Pen { style: 0, width: 1, colorref: 0x00000000 });
        s.objects.insert(STOCK_NULL_PEN, GdiObject::Pen { style: 5, width: 0, colorref: 0 });
        s
    }

    fn pen(&self) -> Option<&GdiObject> {
        self.objects.get(&self.pen_idx)
    }

    fn brush(&self) -> Option<&GdiObject> {
        self.objects.get(&self.brush_idx)
    }

    fn stroke_attrs(&self) -> String {
        match self.pen() {
            Some(GdiObject::Pen { style, width, colorref }) if *style != 5 => {
                let color = colorref_to_css(*colorref);
                let w = (*width as f64).max(1.0);
                let dash = match style {
                    1 => " stroke-dasharray=\"6 2\"",
                    2 => " stroke-dasharray=\"2 2\"",
                    3 => " stroke-dasharray=\"6 2 2 2\"",
                    _ => "",
                };
                format!("stroke=\"{color}\" stroke-width=\"{w}\"{dash}")
            }
            _ => "stroke=\"none\"".to_string(),
        }
    }

    fn fill_attrs(&self) -> String {
        match self.brush() {
            Some(GdiObject::Brush { style, colorref }) if *style == 0 => {
                format!("fill=\"{}\"", colorref_to_css(*colorref))
            }
            _ => "fill=\"none\"".to_string(),
        }
    }

    fn shape_attrs(&self) -> String {
        format!("{} {}", self.fill_attrs(), self.stroke_attrs())
    }

    fn push(&mut self, s: String) {
        if self.in_path {
            self.path_cmds.push(s);
        } else {
            self.elements.push(s);
        }
    }
}

// ── Helper readers ─────────────────────────────────────────────────────────

fn read_i32(data: &[u8], off: usize) -> i32 {
    if off + 4 > data.len() {
        return 0;
    }
    i32::from_le_bytes([data[off], data[off + 1], data[off + 2], data[off + 3]])
}

fn read_u32(data: &[u8], off: usize) -> u32 {
    if off + 4 > data.len() {
        return 0;
    }
    u32::from_le_bytes([data[off], data[off + 1], data[off + 2], data[off + 3]])
}

fn read_i16(data: &[u8], off: usize) -> i16 {
    if off + 2 > data.len() {
        return 0;
    }
    i16::from_le_bytes([data[off], data[off + 1]])
}

fn read_u16(data: &[u8], off: usize) -> u16 {
    if off + 2 > data.len() {
        return 0;
    }
    u16::from_le_bytes([data[off], data[off + 1]])
}

// ── Record handlers ────────────────────────────────────────────────────────

fn handle_createpen(state: &mut EmfState, rec: &[u8], handle_idx: u32) {
    // EMR_CREATEPEN: iType(4) nSize(4) lopn.lopnStyle(4) lopn.lopnWidth.cx(4) cy(4) lopn.lopnColor(4)
    if rec.len() < 24 {
        return;
    }
    let style = read_u32(rec, 8);
    let width = read_u32(rec, 12) as u32;
    let colorref = read_u32(rec, 20);
    state
        .objects
        .insert(handle_idx, GdiObject::Pen { style, width: width.max(1), colorref });
}

fn handle_extcreatepen(state: &mut EmfState, rec: &[u8], handle_idx: u32) {
    // EMR_EXTCREATEPEN: iType(4) nSize(4) ihPen(4) offBmi(4) cbBmi(4) offBits(4) cbBits(4)
    //   elp.elpPenStyle(4) elp.elpWidth(4) elp.elpBrushStyle(4) elp.elpColor(4) ...
    if rec.len() < 36 {
        return;
    }
    let style = read_u32(rec, 28);
    let width = read_u32(rec, 32).max(1);
    let colorref = read_u32(rec, 40);
    let pen_style = style & 0x0F; // lower nibble is PS_ style
    state
        .objects
        .insert(handle_idx, GdiObject::Pen { style: pen_style, width, colorref });
}

fn handle_createbrush(state: &mut EmfState, rec: &[u8], handle_idx: u32) {
    // EMR_CREATEBRUSHINDIRECT: iType(4) nSize(4) lb.lbStyle(4) lb.lbColor(4) lb.lbHatch(4)
    if rec.len() < 20 {
        return;
    }
    let style = read_u32(rec, 8);
    let colorref = read_u32(rec, 12);
    state.objects.insert(handle_idx, GdiObject::Brush { style, colorref });
}

fn handle_extcreatefont(state: &mut EmfState, rec: &[u8], handle_idx: u32) {
    // LOGFONTW starts at offset 8: height(4), width(4), escapement(4), orientation(4),
    //   weight(4), italic(1), underline(1), strikeout(1), charset(1), ...
    //   faceName: 64 bytes (32 UTF-16LE chars) at offset 8+28=36
    if rec.len() < 100 {
        return;
    }
    let height = read_i32(rec, 8);
    let weight = read_u32(rec, 24);
    let italic = rec[32] != 0;
    // face name: 32 UTF-16LE chars at offset 8+28 = 36
    let face_bytes = &rec[36..36 + 64];
    let mut face = String::new();
    for i in (0..64).step_by(2) {
        let ch = u16::from_le_bytes([face_bytes[i], face_bytes[i + 1]]);
        if ch == 0 {
            break;
        }
        if let Some(c) = char::from_u32(ch as u32) {
            face.push(c);
        }
    }
    state.objects.insert(
        handle_idx,
        GdiObject::Font { face, height, bold: weight >= 700, italic },
    );
}

fn handle_selectobject(state: &mut EmfState, rec: &[u8]) {
    if rec.len() < 12 {
        return;
    }
    let handle = read_u32(rec, 8);
    // Determine what type it is and update current pen/brush
    match state.objects.get(&handle) {
        Some(GdiObject::Pen { .. }) => state.pen_idx = handle,
        Some(GdiObject::Brush { .. }) => state.brush_idx = handle,
        Some(GdiObject::Font { .. }) => {} // font selection tracked elsewhere
        _ => {
            // Stock object — classify by range
            if handle == STOCK_NULL_PEN
                || handle == STOCK_WHITE_PEN
                || handle == STOCK_BLACK_PEN
            {
                state.pen_idx = handle;
            } else if handle >= STOCK_WHITE_BRUSH && handle <= STOCK_NULL_BRUSH {
                state.brush_idx = handle;
            }
        }
    }
}

fn handle_rectangle(state: &mut EmfState, rec: &[u8]) {
    // iType(4) nSize(4) rclBox.left(4) top(4) right(4) bottom(4)
    if rec.len() < 24 {
        return;
    }
    let x1 = read_i32(rec, 8) as f64;
    let y1 = read_i32(rec, 12) as f64;
    let x2 = read_i32(rec, 16) as f64;
    let y2 = read_i32(rec, 20) as f64;
    let (x, y, w, h) = (x1, y1, (x2 - x1).abs(), (y2 - y1).abs());
    let attrs = state.shape_attrs();
    state.push(format!("<rect x=\"{x:.2}\" y=\"{y:.2}\" width=\"{w:.2}\" height=\"{h:.2}\" {attrs}/>"));
}

fn handle_ellipse(state: &mut EmfState, rec: &[u8]) {
    if rec.len() < 24 {
        return;
    }
    let x1 = read_i32(rec, 8) as f64;
    let y1 = read_i32(rec, 12) as f64;
    let x2 = read_i32(rec, 16) as f64;
    let y2 = read_i32(rec, 20) as f64;
    let cx = (x1 + x2) / 2.0;
    let cy = (y1 + y2) / 2.0;
    let rx = (x2 - x1).abs() / 2.0;
    let ry = (y2 - y1).abs() / 2.0;
    let attrs = state.shape_attrs();
    state.push(format!("<ellipse cx=\"{cx:.2}\" cy=\"{cy:.2}\" rx=\"{rx:.2}\" ry=\"{ry:.2}\" {attrs}/>"));
}

fn handle_lineto(state: &mut EmfState, rec: &[u8]) {
    if rec.len() < 16 {
        return;
    }
    let x = read_i32(rec, 8) as f64;
    let y = read_i32(rec, 12) as f64;
    let x0 = state.cur_x;
    let y0 = state.cur_y;
    let stroke = state.stroke_attrs();
    state.push(format!("<line x1=\"{x0:.2}\" y1=\"{y0:.2}\" x2=\"{x:.2}\" y2=\"{y:.2}\" fill=\"none\" {stroke}/>"));
    state.cur_x = x;
    state.cur_y = y;
}

fn handle_movetoex(state: &mut EmfState, rec: &[u8]) {
    if rec.len() < 16 {
        return;
    }
    state.cur_x = read_i32(rec, 8) as f64;
    state.cur_y = read_i32(rec, 12) as f64;
}

fn handle_polyline16(state: &mut EmfState, rec: &[u8]) {
    // iType(4) nSize(4) rclBounds(16) cpts(4) apts[]: {x:i16, y:i16}
    if rec.len() < 32 {
        return;
    }
    let count = read_u32(rec, 24) as usize;
    let pts_off = 28;
    if rec.len() < pts_off + count * 4 {
        return;
    }
    if count < 2 {
        return;
    }
    let mut pts = Vec::with_capacity(count);
    for i in 0..count {
        let x = read_i16(rec, pts_off + i * 4) as f64;
        let y = read_i16(rec, pts_off + i * 4 + 2) as f64;
        pts.push((x, y));
    }
    let stroke = state.stroke_attrs();
    let fill = state.fill_attrs();
    let d: String = pts
        .iter()
        .enumerate()
        .map(|(i, (x, y))| {
            if i == 0 {
                format!("M{x:.2},{y:.2}")
            } else {
                format!("L{x:.2},{y:.2}")
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    state.push(format!("<path d=\"{d}\" {fill} {stroke}/>"));
}

fn handle_polylineto16(state: &mut EmfState, rec: &[u8]) {
    // Like polyline16 but starts from current position
    if rec.len() < 32 {
        return;
    }
    let count = read_u32(rec, 24) as usize;
    let pts_off = 28;
    if rec.len() < pts_off + count * 4 {
        return;
    }
    if count == 0 {
        return;
    }
    let stroke = state.stroke_attrs();
    let mut d = format!("M{:.2},{:.2}", state.cur_x, state.cur_y);
    for i in 0..count {
        let x = read_i16(rec, pts_off + i * 4) as f64;
        let y = read_i16(rec, pts_off + i * 4 + 2) as f64;
        d.push_str(&format!(" L{x:.2},{y:.2}"));
        if i + 1 == count {
            state.cur_x = x;
            state.cur_y = y;
        }
    }
    state.push(format!("<path d=\"{d}\" fill=\"none\" {stroke}/>"));
}

fn handle_polybezierto16(state: &mut EmfState, rec: &[u8]) {
    // Like polylineto16 but pairs are Bezier control / endpoint
    if rec.len() < 32 {
        return;
    }
    let count = read_u32(rec, 24) as usize;
    let pts_off = 28;
    if rec.len() < pts_off + count * 4 || count % 3 != 0 {
        return;
    }
    let stroke = state.stroke_attrs();
    let mut d = format!("M{:.2},{:.2}", state.cur_x, state.cur_y);
    let mut i = 0;
    while i + 2 < count {
        let x1 = read_i16(rec, pts_off + i * 4) as f64;
        let y1 = read_i16(rec, pts_off + i * 4 + 2) as f64;
        let x2 = read_i16(rec, pts_off + (i + 1) * 4) as f64;
        let y2 = read_i16(rec, pts_off + (i + 1) * 4 + 2) as f64;
        let x3 = read_i16(rec, pts_off + (i + 2) * 4) as f64;
        let y3 = read_i16(rec, pts_off + (i + 2) * 4 + 2) as f64;
        d.push_str(&format!(" C{x1:.2},{y1:.2} {x2:.2},{y2:.2} {x3:.2},{y3:.2}"));
        state.cur_x = x3;
        state.cur_y = y3;
        i += 3;
    }
    state.push(format!("<path d=\"{d}\" fill=\"none\" {stroke}/>"));
}

fn handle_exttextoutw(state: &mut EmfState, rec: &[u8]) {
    // iType(4) nSize(4) rclBounds(16) iGraphicsMode(4) exScale(4) eyScale(4)
    // emrtext: ptlReference(8) nChars(4) offString(4) fOptions(4)
    //   rcl(16) offDx(4) — then at offString: UTF-16LE chars
    if rec.len() < 60 {
        return;
    }
    let base = 8 + 16 + 12; // after rclBounds, iGraphicsMode, exScale, eyScale
    let x = read_i32(rec, base) as f64;
    let y = read_i32(rec, base + 4) as f64;
    let n_chars = read_u32(rec, base + 8) as usize;
    let off_str = read_u32(rec, base + 12) as usize;
    if n_chars == 0 || off_str + n_chars * 2 > rec.len() {
        return;
    }
    // Decode UTF-16LE
    let mut text = String::new();
    for i in 0..n_chars {
        let ch = read_u16(rec, off_str + i * 2);
        if let Some(c) = char::from_u32(ch as u32) {
            if !c.is_control() {
                text.push(c);
            }
        }
    }
    if text.is_empty() {
        return;
    }
    // Escape XML special chars
    let escaped = text
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;");
    let color = colorref_to_css(state.text_color);
    // Approximate font size from current font if available; default 12
    let font_size = 12.0_f64;
    state.push(format!(
        "<text x=\"{x:.2}\" y=\"{y:.2}\" fill=\"{color}\" font-size=\"{font_size}\">{escaped}</text>"
    ));
}

fn handle_exttextouta(state: &mut EmfState, rec: &[u8]) {
    // Same layout but ASCII string
    if rec.len() < 60 {
        return;
    }
    let base = 8 + 16 + 12;
    let x = read_i32(rec, base) as f64;
    let y = read_i32(rec, base + 4) as f64;
    let n_chars = read_u32(rec, base + 8) as usize;
    let off_str = read_u32(rec, base + 12) as usize;
    if n_chars == 0 || off_str + n_chars > rec.len() {
        return;
    }
    let raw = &rec[off_str..off_str + n_chars];
    let text: String = raw
        .iter()
        .filter(|&&b| b >= 0x20 && b < 0x80)
        .map(|&b| b as char)
        .collect();
    if text.is_empty() {
        return;
    }
    let escaped = text
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    let color = colorref_to_css(state.text_color);
    state.push(format!(
        "<text x=\"{x:.2}\" y=\"{y:.2}\" fill=\"{color}\" font-size=\"12\">{escaped}</text>"
    ));
}

/// Try to extract an embedded DIB from a BITBLT/STRETCHDIBITS record
/// and return it as a BMP-wrapped Vec<u8> that the `image` crate can decode.
fn extract_dib_as_bmp(rec: &[u8]) -> Option<Vec<u8>> {
    // EMR_BITBLT layout (offsets from record start):
    //   8: rclBounds (16) | 24: xDest(4) yDest(4) cxDest(4) cyDest(4) |
    //   40: dwRop(4) | 44: xSrc(4) ySrc(4) |
    //   52: offBmiSrc(4) cbBmiSrc(4) offBitsSrc(4) cbBitsSrc(4)
    //
    // EMR_STRETCHDIBITS layout:
    //   8: rclBounds(16) | 24: xDest(4) yDest(4) xSrc(4) ySrc(4) cxSrc(4) cySrc(4)
    //   48: offBmiSrc(4) cbBmiSrc(4) offBitsSrc(4) cbBitsSrc(4)
    //   64: iUsageSrc(4) dwRop(4) cxDest(4) cyDest(4)
    //
    // For both, we try offBmiSrc/cbBmiSrc/offBitsSrc/cbBitsSrc.
    let rec_type = read_u32(rec, 0);
    let (off_bmi_field, off_bits_field) = match rec_type {
        EMR_BITBLT => (52usize, 60usize),
        EMR_STRETCHBLT => (52usize, 60usize),
        EMR_STRETCHDIBITS => (48usize, 56usize),
        _ => return None,
    };
    if rec.len() < off_bits_field + 8 {
        return None;
    }
    let off_bmi = read_u32(rec, off_bmi_field) as usize;
    let cb_bmi = read_u32(rec, off_bmi_field + 4) as usize;
    let off_bits = read_u32(rec, off_bits_field) as usize;
    let cb_bits = read_u32(rec, off_bits_field + 4) as usize;
    if cb_bmi == 0 || cb_bits == 0 {
        return None;
    }
    if off_bmi + cb_bmi > rec.len() || off_bits + cb_bits > rec.len() {
        return None;
    }
    let bmi = &rec[off_bmi..off_bmi + cb_bmi];
    let bits = &rec[off_bits..off_bits + cb_bits];
    // Compose a BMP file: 14-byte file header + BITMAPINFOHEADER + palette + bits
    let file_size = 14 + cb_bmi + cb_bits;
    // pixel data offset = 14 (file hdr) + 40 (BITMAPINFOHEADER) + palette
    // For simplicity use off_bits relative to BMP start:
    let pixel_offset = 14u32 + cb_bmi as u32;
    let mut bmp = Vec::with_capacity(file_size);
    // BMP file header
    bmp.extend_from_slice(b"BM");
    bmp.extend_from_slice(&(file_size as u32).to_le_bytes());
    bmp.extend_from_slice(&0u16.to_le_bytes()); // reserved1
    bmp.extend_from_slice(&0u16.to_le_bytes()); // reserved2
    bmp.extend_from_slice(&pixel_offset.to_le_bytes());
    bmp.extend_from_slice(bmi);
    bmp.extend_from_slice(bits);
    Some(bmp)
}

// ── Main entry point ───────────────────────────────────────────────────────

/// Convert EMF bytes to SVG markup.
///
/// Returns `None` if the data doesn't start with the EMF header magic
/// (`01 00 00 00`) or the header is too short.
pub fn emf_to_svg(data: &[u8]) -> Option<String> {
    // Minimum: 8 bytes for type + size
    if data.len() < 8 {
        return None;
    }
    let rec0_type = read_u32(data, 0);
    if rec0_type != EMR_HEADER {
        return None;
    }
    let header_size = read_u32(data, 4) as usize;
    if data.len() < header_size.max(40) {
        return None;
    }

    // rclBounds: bounding box in device units (logical coords of the drawing)
    let b_left = read_i32(data, 8) as f64;
    let b_top = read_i32(data, 12) as f64;
    let b_right = read_i32(data, 16) as f64;
    let b_bottom = read_i32(data, 20) as f64;

    // rclFrame: bounding box in 0.01mm units (physical size)
    let f_left = read_i32(data, 24) as f64;
    let f_top = read_i32(data, 28) as f64;
    let f_right = read_i32(data, 32) as f64;
    let f_bottom = read_i32(data, 36) as f64;

    // Physical size in mm
    let phys_w_mm = (f_right - f_left).abs() * 0.01;
    let phys_h_mm = (f_bottom - f_top).abs() * 0.01;

    // Fallback if frame is empty
    let (svg_w_mm, svg_h_mm) = if phys_w_mm > 0.01 && phys_h_mm > 0.01 {
        (phys_w_mm, phys_h_mm)
    } else {
        (100.0, 50.0)
    };

    // viewBox in logical (device) units
    let vb_x = b_left;
    let vb_y = b_top;
    let vb_w = (b_right - b_left).abs().max(1.0);
    let vb_h = (b_bottom - b_top).abs().max(1.0);

    let mut state = EmfState::new();
    // Handle allocation index: objects are allocated in a 1-based table.
    // We track the next free slot by scanning records for ihXxx.
    let mut next_handle: u32 = 1;

    // Walk records
    let mut offset = 0usize;
    loop {
        if offset + 8 > data.len() {
            break;
        }
        let rec_type = read_u32(data, offset);
        let rec_size = read_u32(data, offset + 4) as usize;
        if rec_size < 8 || offset + rec_size > data.len() {
            break;
        }
        let rec = &data[offset..offset + rec_size];

        match rec_type {
            EMR_EOF => break,
            EMR_HEADER => {} // already parsed above

            EMR_CREATEPEN => {
                handle_createpen(&mut state, rec, next_handle);
                next_handle += 1;
            }
            EMR_EXTCREATEPEN => {
                handle_extcreatepen(&mut state, rec, next_handle);
                next_handle += 1;
            }
            EMR_CREATEBRUSHINDIRECT => {
                handle_createbrush(&mut state, rec, next_handle);
                next_handle += 1;
            }
            EMR_EXTCREATEFONTINDIRECTW => {
                handle_extcreatefont(&mut state, rec, next_handle);
                next_handle += 1;
            }
            EMR_SELECTOBJECT => handle_selectobject(&mut state, rec),
            EMR_DELETEOBJECT => {
                if rec.len() >= 12 {
                    let h = read_u32(rec, 8);
                    state.objects.remove(&h);
                }
            }

            EMR_SETTEXTCOLOR => {
                if rec.len() >= 12 {
                    state.text_color = read_u32(rec, 8);
                }
            }
            EMR_SETBKCOLOR => {
                if rec.len() >= 12 {
                    state.bk_color = read_u32(rec, 8);
                }
            }
            EMR_SETBKMODE => {
                if rec.len() >= 12 {
                    state.bk_mode = read_u32(rec, 8);
                }
            }
            EMR_SETTEXTALIGN | EMR_INTERSECTCLIPRECT | EMR_SETWORLDTRANSFORM
            | EMR_MODIFYWORLDTRANSFORM => {}

            EMR_MOVETOEX => handle_movetoex(&mut state, rec),
            EMR_LINETO => handle_lineto(&mut state, rec),
            EMR_RECTANGLE => handle_rectangle(&mut state, rec),
            EMR_ELLIPSE => handle_ellipse(&mut state, rec),
            EMR_POLYLINE16 => handle_polyline16(&mut state, rec),
            EMR_POLYLINETO16 => handle_polylineto16(&mut state, rec),
            EMR_POLYBEZIERTO16 => handle_polybezierto16(&mut state, rec),

            EMR_BEGINPATH => {
                state.in_path = true;
                state.path_cmds.clear();
            }
            EMR_ENDPATH | EMR_CLOSEFIGURE => {}
            EMR_FILLPATH | EMR_STROKEANDFILLPATH | EMR_STROKEPATH => {
                // Flush accumulated path commands as a <path> element
                if !state.path_cmds.is_empty() {
                    let cmds = state.path_cmds.join("\n");
                    let attrs = state.shape_attrs();
                    state.elements.push(format!("<g {attrs}>{cmds}</g>"));
                    state.path_cmds.clear();
                }
                state.in_path = false;
            }

            EMR_EXTTEXTOUTW => handle_exttextoutw(&mut state, rec),
            EMR_EXTTEXTOUTA => handle_exttextouta(&mut state, rec),

            EMR_BITBLT | EMR_STRETCHBLT | EMR_STRETCHDIBITS => {
                if let Some(bmp) = extract_dib_as_bmp(rec) {
                    state.bitmaps.push(bmp);
                }
            }

            _ => {} // skip unknown records
        }

        offset += rec_size;
    }

    // If we found embedded bitmaps, represent the first as a data URI
    let bitmap_el = if !state.bitmaps.is_empty() {
        let b64 = base64_encode(&state.bitmaps[0]);
        let img_x = vb_x;
        let img_y = vb_y;
        format!(
            "<image x=\"{img_x:.2}\" y=\"{img_y:.2}\" width=\"{vb_w:.2}\" height=\"{vb_h:.2}\" \
             preserveAspectRatio=\"xMidYMid meet\" \
             href=\"data:image/bmp;base64,{b64}\"/>"
        )
    } else {
        String::new()
    };

    // Compose SVG
    let body = state.elements.join("\n  ");
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" \
         width=\"{svg_w_mm:.2}mm\" height=\"{svg_h_mm:.2}mm\" \
         viewBox=\"{vb_x:.2} {vb_y:.2} {vb_w:.2} {vb_h:.2}\">\
         <rect x=\"{vb_x:.2}\" y=\"{vb_y:.2}\" width=\"{vb_w:.2}\" height=\"{vb_h:.2}\" fill=\"white\"/>\
         {bitmap_el}{body}</svg>"
    );
    Some(svg)
}

/// Minimal base-64 encoder — avoids pulling in the `base64` crate.
fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    let mut i = 0;
    while i + 2 < data.len() {
        let b = ((data[i] as u32) << 16) | ((data[i + 1] as u32) << 8) | (data[i + 2] as u32);
        out.push(CHARS[((b >> 18) & 63) as usize] as char);
        out.push(CHARS[((b >> 12) & 63) as usize] as char);
        out.push(CHARS[((b >> 6) & 63) as usize] as char);
        out.push(CHARS[(b & 63) as usize] as char);
        i += 3;
    }
    let rem = data.len() - i;
    if rem == 1 {
        let b = (data[i] as u32) << 16;
        out.push(CHARS[((b >> 18) & 63) as usize] as char);
        out.push(CHARS[((b >> 12) & 63) as usize] as char);
        out.push('=');
        out.push('=');
    } else if rem == 2 {
        let b = ((data[i] as u32) << 16) | ((data[i + 1] as u32) << 8);
        out.push(CHARS[((b >> 18) & 63) as usize] as char);
        out.push(CHARS[((b >> 12) & 63) as usize] as char);
        out.push(CHARS[((b >> 6) & 63) as usize] as char);
        out.push('=');
    }
    out
}
