//! Render a DOCX or ODT fixture to PDF for manual fidelity inspection.
//!
//! Usage:
//!   cargo run -p s1engine --features pdf --example render_pdf -- <input> <output.pdf>

use s1engine::{Engine, Format};
use std::env;
use std::fs;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: render_pdf <input.{{docx,odt}}> <output.pdf>");
        return ExitCode::from(64);
    }
    let input = &args[1];
    let output = &args[2];

    let bytes = match fs::read(input) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("read {input}: {e}");
            return ExitCode::from(66);
        }
    };

    let engine = Engine::new();
    let format = if input.ends_with(".odt") {
        Format::Odt
    } else {
        Format::Docx
    };
    let doc = match engine.open_as(&bytes, format) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("parse {input}: {e}");
            return ExitCode::from(65);
        }
    };

    eprintln!("parsed: {} paragraphs", doc.paragraph_count());

    let pdf = match doc.export(Format::Pdf) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("export PDF: {e}");
            return ExitCode::from(70);
        }
    };

    eprintln!("PDF size: {} bytes ({} KB)", pdf.len(), pdf.len() / 1024);

    if let Err(e) = fs::write(output, &pdf) {
        eprintln!("write {output}: {e}");
        return ExitCode::from(73);
    }

    eprintln!("wrote {output}");
    ExitCode::SUCCESS
}
