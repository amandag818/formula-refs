// Scans a formula string for A1-style cell references without evaluating
// the formula. We deliberately don't build a full expression grammar here:
// dependency extraction only needs to find reference tokens and skip over
// string literals and quoted sheet names correctly.

#[derive(Debug, Clone, PartialEq)]
pub struct Cell {
    pub col: u32, // 1-based, A = 1
    pub row: u32, // 1-based
    pub col_absolute: bool,
    pub row_absolute: bool,
}

impl Cell {
    pub fn to_a1(&self) -> String {
        let mut s = String::new();
        if self.col_absolute {
            s.push('$');
        }
        s.push_str(&col_to_letters(self.col));
        if self.row_absolute {
            s.push('$');
        }
        s.push_str(&self.row.to_string());
        s
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CellRange {
    pub sheet: Option<String>,
    pub start: Cell,
    pub end: Option<Cell>,
}

impl CellRange {
    pub fn to_a1(&self) -> String {
        let mut s = String::new();
        if let Some(sheet) = &self.sheet {
            s.push_str(&quote_sheet_name(sheet));
            s.push('!');
        }
        s.push_str(&self.start.to_a1());
        if let Some(end) = &self.end {
            s.push(':');
            s.push_str(&end.to_a1());
        }
        s
    }

    pub fn cell_count(&self) -> u64 {
        match &self.end {
            None => 1,
            Some(end) => {
                let cols = (self.start.col as i64 - end.col as i64).unsigned_abs() + 1;
                let rows = (self.start.row as i64 - end.row as i64).unsigned_abs() + 1;
                cols * rows
            }
        }
    }
}

// A defined name (workbook- or sheet-scoped) used where a cell or range
// reference could otherwise appear, e.g. `TaxRate` in `=A1*TaxRate`. We
// have no access to the workbook's actual name table, so we can't know
// what it resolves to - only that it isn't a cell reference or a function
// call, which is all a dependency audit needs.
#[derive(Debug, Clone, PartialEq)]
pub struct NamedRange {
    pub sheet: Option<String>,
    pub name: String,
}

impl NamedRange {
    pub fn to_a1(&self) -> String {
        match &self.sheet {
            Some(sheet) => format!("{}!{}", quote_sheet_name(sheet), self.name),
            None => self.name.clone(),
        }
    }
}

// A structured reference into an Excel table, e.g. `Table1[Column1]` or
// `Table1[[#Headers],[Column1]]`. `specifier` is whatever was inside the
// outer brackets, kept verbatim - we don't need to understand `#Headers`,
// `#Totals`, `@`, or column lists to report that the formula depends on
// (some part of) the table.
#[derive(Debug, Clone, PartialEq)]
pub struct TableRef {
    pub table: String,
    pub specifier: String,
}

impl TableRef {
    pub fn to_a1(&self) -> String {
        format!("{}[{}]", self.table, self.specifier)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Reference {
    Cell(CellRange),
    Named(NamedRange),
    Table(TableRef),
}

impl Reference {
    pub fn to_a1(&self) -> String {
        match self {
            Reference::Cell(c) => c.to_a1(),
            Reference::Named(n) => n.to_a1(),
            Reference::Table(t) => t.to_a1(),
        }
    }

    pub fn sheet(&self) -> Option<&str> {
        match self {
            Reference::Cell(c) => c.sheet.as_deref(),
            Reference::Named(n) => n.sheet.as_deref(),
            Reference::Table(_) => None,
        }
    }

    // None for named ranges and table references: without the workbook's
    // name table (or the table's row count) we can't say how many cells
    // either one resolves to.
    pub fn cell_count(&self) -> Option<u64> {
        match self {
            Reference::Cell(c) => Some(c.cell_count()),
            Reference::Named(_) | Reference::Table(_) => None,
        }
    }
}

pub fn col_to_letters(mut n: u32) -> String {
    let mut letters = Vec::new();
    while n > 0 {
        let rem = (n - 1) % 26;
        letters.push((b'A' + rem as u8) as char);
        n = (n - 1) / 26;
    }
    letters.reverse();
    letters.into_iter().collect()
}

fn letters_to_col(letters: &str) -> u32 {
    let mut n: u32 = 0;
    for c in letters.chars() {
        n = n * 26 + (c.to_ascii_uppercase() as u32 - 'A' as u32 + 1);
    }
    n
}

fn quote_sheet_name(name: &str) -> String {
    let needs_quotes = !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.');
    if !needs_quotes {
        return name.to_string();
    }
    let mut out = String::from("'");
    for c in name.chars() {
        if c == '\'' {
            out.push('\'');
        }
        out.push(c);
    }
    out.push('\'');
    out
}

fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '.'
}

// Tries to parse a cell reference starting at `chars[i]`. Returns the cell
// and the index just past it, or None if `chars[i]` isn't the start of one.
fn try_parse_cell(chars: &[char], i: usize) -> Option<(Cell, usize)> {
    let mut pos = i;
    let col_absolute = chars.get(pos) == Some(&'$');
    if col_absolute {
        pos += 1;
    }

    let letters_start = pos;
    while chars.get(pos).is_some_and(|c| c.is_ascii_alphabetic()) {
        pos += 1;
    }
    if pos == letters_start || pos - letters_start > 3 {
        return None;
    }
    let letters: String = chars[letters_start..pos].iter().collect();

    let row_absolute = chars.get(pos) == Some(&'$');
    if row_absolute {
        pos += 1;
    }

    let digits_start = pos;
    while chars.get(pos).is_some_and(|c| c.is_ascii_digit()) {
        pos += 1;
    }
    if pos == digits_start {
        return None;
    }
    let digits: String = chars[digits_start..pos].iter().collect();

    // Reject matches that are actually part of a longer identifier, e.g.
    // the "A1" inside "MyRangeA1Name".
    if chars.get(pos).is_some_and(|&c| is_word_char(c)) {
        return None;
    }

    let row: u32 = digits.parse().ok()?;
    if row == 0 {
        return None;
    }

    Some((
        Cell {
            col: letters_to_col(&letters),
            row,
            col_absolute,
            row_absolute,
        },
        pos,
    ))
}

// Tries to parse `Name!` (unquoted sheet name) starting at `chars[i]`.
fn try_parse_unquoted_sheet(chars: &[char], i: usize) -> Option<(String, usize)> {
    if !chars.get(i).is_some_and(|c| c.is_ascii_alphabetic() || *c == '_') {
        return None;
    }
    let mut pos = i;
    while chars.get(pos).is_some_and(|&c| is_word_char(c)) {
        pos += 1;
    }
    if chars.get(pos) == Some(&'!') {
        Some((chars[i..pos].iter().collect(), pos + 1))
    } else {
        None
    }
}

// Tries to parse `'Quoted Name'!` starting at `chars[i]` (chars[i] == '\'').
fn try_parse_quoted_sheet(chars: &[char], i: usize) -> Option<(String, usize)> {
    let mut pos = i + 1;
    let mut name = String::new();
    loop {
        match chars.get(pos) {
            None => return None,
            Some('\'') => {
                if chars.get(pos + 1) == Some(&'\'') {
                    name.push('\'');
                    pos += 2;
                } else {
                    pos += 1;
                    break;
                }
            }
            Some(c) => {
                name.push(*c);
                pos += 1;
            }
        }
    }
    if chars.get(pos) == Some(&'!') {
        Some((name, pos + 1))
    } else {
        None
    }
}

// Tries to parse a bare identifier starting at `chars[i]`: a letter or
// underscore followed by letters, digits, underscores, or dots. Excel
// names can't start with a digit, which is what keeps this from colliding
// with cell references (those are handled by try_parse_cell first and
// would already have matched). This only finds the word boundary - the
// caller decides whether the word is actually a named-range reference or
// something to skip (a function call, a boolean literal), since either
// way the whole word must be consumed to avoid rescanning a trailing
// fragment of it as its own token.
fn try_parse_name(chars: &[char], i: usize) -> Option<(String, usize)> {
    if !chars.get(i).is_some_and(|c| c.is_ascii_alphabetic() || *c == '_') {
        return None;
    }
    let mut pos = i;
    while chars.get(pos).is_some_and(|&c| is_word_char(c)) {
        pos += 1;
    }
    Some((chars[i..pos].iter().collect(), pos))
}

// Parses a balanced `[...]` structured-reference specifier starting at
// chars[i] (chars[i] == '['). Specifiers can nest brackets, e.g.
// `Table1[[#Headers],[Column1]]`, so this tracks depth rather than
// stopping at the first ']'. Returns the inner text (without the outer
// brackets) and the index just past the closing ']', or None if the
// brackets never balance before the formula ends.
fn try_parse_bracket_specifier(chars: &[char], i: usize) -> Option<(String, usize)> {
    let inner_start = i + 1;
    let mut depth = 0u32;
    let mut pos = i;
    loop {
        match chars.get(pos) {
            None => return None,
            Some('[') => {
                depth += 1;
                pos += 1;
            }
            Some(']') => {
                depth -= 1;
                pos += 1;
                if depth == 0 {
                    return Some((chars[inner_start..pos - 1].iter().collect(), pos));
                }
            }
            _ => pos += 1,
        }
    }
}

fn skip_string_literal(chars: &[char], i: usize) -> usize {
    let mut pos = i + 1;
    loop {
        match chars.get(pos) {
            None => return pos,
            Some('"') => {
                if chars.get(pos + 1) == Some(&'"') {
                    pos += 2;
                } else {
                    return pos + 1;
                }
            }
            _ => pos += 1,
        }
    }
}

pub fn extract(formula: &str) -> Vec<Reference> {
    let chars: Vec<char> = formula.chars().collect();
    let mut refs = Vec::new();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];

        if c == '"' {
            i = skip_string_literal(&chars, i);
            continue;
        }

        let mut sheet = None;
        let mut cell_start = i;

        if c == '\'' {
            if let Some((name, next)) = try_parse_quoted_sheet(&chars, i) {
                sheet = Some(name);
                cell_start = next;
            } else {
                i += 1;
                continue;
            }
        } else if c.is_ascii_alphabetic() || c == '_' {
            if let Some((name, next)) = try_parse_unquoted_sheet(&chars, i) {
                sheet = Some(name);
                cell_start = next;
            }
        }

        if let Some((start, mut pos)) = try_parse_cell(&chars, cell_start) {
            let mut end = None;
            if chars.get(pos) == Some(&':') {
                if let Some((end_cell, next)) = try_parse_cell(&chars, pos + 1) {
                    end = Some(end_cell);
                    pos = next;
                }
            }
            refs.push(Reference::Cell(CellRange { sheet, start, end }));
            i = pos;
            continue;
        }

        if let Some((name, next)) = try_parse_name(&chars, cell_start) {
            // A name immediately followed by '[' is a structured table
            // reference (`Table1[Column1]`), not a defined name.
            if chars.get(next) == Some(&'[') {
                if let Some((specifier, after)) = try_parse_bracket_specifier(&chars, next) {
                    refs.push(Reference::Table(TableRef { table: name, specifier }));
                    i = after;
                    continue;
                }
            }

            // A name immediately followed by '(' is a function call, and
            // TRUE/FALSE are boolean literals, not defined names. Either
            // way the word is consumed; only whether we record it differs.
            let is_function_call = chars.get(next) == Some(&'(');
            let is_boolean_literal =
                name.eq_ignore_ascii_case("TRUE") || name.eq_ignore_ascii_case("FALSE");
            if !is_function_call && !is_boolean_literal {
                refs.push(Reference::Named(NamedRange { sheet, name }));
            }
            i = next;
            continue;
        }

        if sheet.is_some() {
            // Sheet name wasn't followed by a valid reference; resume
            // scanning right after the '!' so we don't loop forever.
            i = cell_start;
            continue;
        }

        i += 1;
    }

    refs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_cell() {
        let refs = extract("=A1+1");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].to_a1(), "A1");
    }

    #[test]
    fn range() {
        let refs = extract("=SUM(A1:B10)");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].to_a1(), "A1:B10");
        assert_eq!(refs[0].cell_count(), Some(20));
    }

    #[test]
    fn absolute_markers() {
        let refs = extract("=$A$1");
        match &refs[0] {
            Reference::Cell(c) => {
                assert_eq!(c.start.col_absolute, true);
                assert_eq!(c.start.row_absolute, true);
            }
            other => panic!("expected a cell reference, got {other:?}"),
        }
    }

    #[test]
    fn quoted_sheet_name() {
        let refs = extract("='Budget 2024'!C3");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].sheet(), Some("Budget 2024"));
        assert_eq!(refs[0].to_a1(), "'Budget 2024'!C3");
    }

    #[test]
    fn unquoted_sheet_name() {
        let refs = extract("=Sheet1!A1:A5");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].sheet(), Some("Sheet1"));
        assert_eq!(refs[0].to_a1(), "Sheet1!A1:A5");
    }

    #[test]
    fn named_range() {
        let refs = extract("=TaxRate*2");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].to_a1(), "TaxRate");
        assert_eq!(refs[0].cell_count(), None);
        assert!(matches!(&refs[0], Reference::Named(_)));
    }

    #[test]
    fn sheet_qualified_named_range() {
        let refs = extract("=Sheet1!TaxRate");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].sheet(), Some("Sheet1"));
        assert_eq!(refs[0].to_a1(), "Sheet1!TaxRate");
    }

    #[test]
    fn function_calls_are_not_named_ranges() {
        let refs = extract("=ROUND(SUM(A1:A10), 2)");
        let a1: Vec<String> = refs.iter().map(|r| r.to_a1()).collect();
        assert_eq!(a1, vec!["A1:A10"]);
    }

    #[test]
    fn boolean_literals_are_not_named_ranges() {
        let refs = extract("=IF(TRUE,A1,B1)");
        let a1: Vec<String> = refs.iter().map(|r| r.to_a1()).collect();
        assert_eq!(a1, vec!["A1", "B1"]);
    }

    #[test]
    fn named_range_mixed_with_cells() {
        let refs = extract("=A1+TaxRate-Sheet2!Discount");
        let a1: Vec<String> = refs.iter().map(|r| r.to_a1()).collect();
        assert_eq!(a1, vec!["A1", "TaxRate", "Sheet2!Discount"]);
    }

    #[test]
    fn ignores_string_literals() {
        let refs = extract("=CONCAT(\"A1:B2\", C3)");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].to_a1(), "C3");
    }

    #[test]
    fn ignores_function_names() {
        let refs = extract("=ROUND(A1, 2)");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].to_a1(), "A1");
    }

    #[test]
    fn simple_table_column_reference() {
        let refs = extract("=SUM(Table1[Sales])");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].to_a1(), "Table1[Sales]");
        assert_eq!(refs[0].cell_count(), None);
        assert_eq!(refs[0].sheet(), None);
        match &refs[0] {
            Reference::Table(t) => {
                assert_eq!(t.table, "Table1");
                assert_eq!(t.specifier, "Sales");
            }
            other => panic!("expected a table reference, got {other:?}"),
        }
    }

    #[test]
    fn table_reference_with_nested_brackets() {
        let refs = extract("=Table1[[#Headers],[Column1]]");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].to_a1(), "Table1[[#Headers],[Column1]]");
    }

    #[test]
    fn table_reference_this_row() {
        let refs = extract("=Table1[@Sales]*Table1[@Price]");
        let a1: Vec<String> = refs.iter().map(|r| r.to_a1()).collect();
        assert_eq!(a1, vec!["Table1[@Sales]", "Table1[@Price]"]);
    }

    #[test]
    fn table_reference_mixed_with_cells() {
        let refs = extract("=A1+Table1[Sales]-B2");
        let a1: Vec<String> = refs.iter().map(|r| r.to_a1()).collect();
        assert_eq!(a1, vec!["A1", "Table1[Sales]", "B2"]);
    }

    #[test]
    fn unbalanced_table_brackets_do_not_hang() {
        // No closing ']', so this isn't parsed as a table reference at
        // all; the bracket is treated as ignorable punctuation and the
        // words on either side of it fall back to named-range handling.
        // The only thing this test really guards is that extraction
        // terminates instead of looping forever hunting for a ']'.
        let refs = extract("=Table1[Sales");
        let a1: Vec<String> = refs.iter().map(|r| r.to_a1()).collect();
        assert_eq!(a1, vec!["Table1", "Sales"]);
    }

    #[test]
    fn multiple_refs() {
        let refs = extract("=A1+B2*SUM(C1:C10)");
        let a1: Vec<String> = refs.iter().map(|r| r.to_a1()).collect();
        assert_eq!(a1, vec!["A1", "B2", "C1:C10"]);
    }
}
