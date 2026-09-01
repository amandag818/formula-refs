mod parser;

use std::env;
use std::io::{self, Read};
use std::process::ExitCode;

struct Args {
    formula: Option<String>,
    json: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut formula = None;
    let mut json = false;

    for arg in env::args().skip(1) {
        match arg.as_str() {
            "--json" => json = true,
            "-h" | "--help" => {
                print_help();
                std::process::exit(0);
            }
            other if other.starts_with('-') && other != "-" => {
                return Err(format!("unrecognized flag: {other}"));
            }
            other => {
                if formula.is_some() {
                    return Err("only one formula argument is allowed".to_string());
                }
                formula = Some(other.to_string());
            }
        }
    }

    Ok(Args { formula, json })
}

fn print_help() {
    println!(
        "formula-refs - list the cell and range references a spreadsheet formula depends on\n\n\
         USAGE:\n\
         \x20   formula-refs [--json] '<formula>'\n\
         \x20   echo '<formula>' | formula-refs [--json]\n\n\
         OPTIONS:\n\
         \x20   --json      print machine-readable JSON instead of the human summary\n\
         \x20   -h, --help  print this message\n\n\
         EXAMPLES:\n\
         \x20   formula-refs '=SUM(A1:A10)+Sheet2!B1'\n\
         \x20   formula-refs --json '=IF(A1>0,B1,C1)'"
    );
}

fn read_formula(args: &Args) -> io::Result<String> {
    match &args.formula {
        Some(f) => Ok(f.clone()),
        None => {
            let mut buf = String::new();
            io::stdin().read_to_string(&mut buf)?;
            Ok(buf.trim().to_string())
        }
    }
}

fn escape_json(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn print_json(formula: &str, refs: &[parser::Reference]) {
    println!("{{");
    println!("  \"formula\": \"{}\",", escape_json(formula));
    println!("  \"references\": [");
    for (idx, r) in refs.iter().enumerate() {
        let sheet = match &r.sheet {
            Some(s) => format!("\"{}\"", escape_json(s)),
            None => "null".to_string(),
        };
        let end = match &r.end {
            Some(e) => format!(
                "{{ \"col\": \"{}\", \"row\": {} }}",
                parser::col_to_letters(e.col),
                e.row
            ),
            None => "null".to_string(),
        };
        println!("    {{");
        println!("      \"reference\": \"{}\",", escape_json(&r.to_a1()));
        println!("      \"sheet\": {sheet},");
        println!(
            "      \"start\": {{ \"col\": \"{}\", \"row\": {} }},",
            parser::col_to_letters(r.start.col),
            r.start.row
        );
        println!("      \"end\": {end},");
        println!("      \"is_range\": {},", r.end.is_some());
        println!("      \"cell_count\": {}", r.cell_count());
        let comma = if idx + 1 < refs.len() { "," } else { "" };
        println!("    }}{comma}");
    }
    println!("  ]");
    println!("}}");
}

fn print_human(refs: &[parser::Reference]) {
    if refs.is_empty() {
        println!("no cell references found");
        return;
    }

    let width = refs.iter().map(|r| r.to_a1().len()).max().unwrap_or(0);
    let mut total_cells: u64 = 0;
    for r in refs {
        let sheet_note = match &r.sheet {
            Some(s) => format!(" (sheet: {s})"),
            None => String::new(),
        };
        let cells = r.cell_count();
        total_cells += cells;
        let label = if cells == 1 { "cell" } else { "cells" };
        println!(
            "{:width$}  {cells} {label}{sheet_note}",
            r.to_a1(),
            width = width
        );
    }
    println!();
    let ref_label = if refs.len() == 1 { "reference" } else { "references" };
    println!("{} {ref_label}, {total_cells} cells total", refs.len());
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("error: {e}");
            eprintln!("try 'formula-refs --help'");
            return ExitCode::FAILURE;
        }
    };

    let formula = match read_formula(&args) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("error reading formula: {e}");
            return ExitCode::FAILURE;
        }
    };

    if formula.trim().is_empty() {
        eprintln!("error: no formula given");
        eprintln!("try 'formula-refs --help'");
        return ExitCode::FAILURE;
    }

    let refs = parser::extract(&formula);

    if args.json {
        print_json(&formula, &refs);
    } else {
        print_human(&refs);
    }

    ExitCode::SUCCESS
}
