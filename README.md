# formula-refs

A command line tool that reads a spreadsheet formula and lists every cell
and range it depends on.

When you're about to edit or delete a row in a large workbook, "what does
this formula actually read?" is a question that's surprisingly hard to
answer just by staring at a nested `IF(SUMIFS(...))`. This tool parses the
formula text (Excel-style A1 notation, with or without sheet qualifiers)
and prints out the references it finds, without evaluating anything.

It does not open `.xlsx` files or connect to a spreadsheet program. It
operates on the formula text you give it, which you can copy out of a
cell's formula bar.

## Usage

```
formula-refs '=SUM(A1:A10)+Sheet2!B1'
```

```
A1:A10    10 cells
B1         1 cell (sheet: Sheet2)

2 references, 11 cells total
```

You can also pipe a formula in on stdin:

```
echo '=IF(A1>0,B1,C1)' | formula-refs
```

### JSON output

Pass `--json` for machine-readable output, useful for feeding into a
script that audits a whole workbook export:

```
formula-refs --json '=SUM(A1:A10)+Sheet2!B1'
```

```json
{
  "formula": "=SUM(A1:A10)+Sheet2!B1",
  "references": [
    {
      "reference": "A1:A10",
      "sheet": null,
      "kind": "cell",
      "start": { "col": "A", "row": 1 },
      "end": { "col": "A", "row": 10 },
      "is_range": true,
      "cell_count": 10
    },
    {
      "reference": "B1",
      "sheet": "Sheet2",
      "kind": "cell",
      "start": { "col": "B", "row": 1 },
      "end": null,
      "is_range": false,
      "cell_count": 1
    }
  ]
}
```

A reference to a defined name instead comes out with `"kind": "named"`,
with no `start`/`end`/`cell_count` fields since the tool has no access to
the workbook's name table and can't say how many cells the name resolves
to:

```json
{
  "reference": "TaxRate",
  "sheet": null,
  "kind": "named",
  "name": "TaxRate"
}
```

## What it handles

- Plain cell references (`A1`) and ranges (`A1:B10`)
- Absolute markers (`$A$1`, `A$1`, `$A1`)
- Sheet-qualified references, quoted and unquoted (`Sheet1!A1`,
  `'Q1 Budget'!A1:B2`)
- String literals in the formula are skipped, so `"A1:B2"` as literal text
  is not mistaken for a reference
- Named ranges (e.g. `TaxRate` used in place of a cell reference), including
  sheet-qualified ones (`Sheet1!TaxRate`). Since the tool never sees the
  workbook's actual name table, it identifies these by elimination: a bare
  word that isn't a cell reference and isn't immediately followed by `(`
  (which would make it a function call) is treated as a name.

## What it doesn't handle yet

- 3D references spanning a sheet range (`Sheet1:Sheet3!A1`)
- Structured table references (`Table1[Column]`)

See the roadmap for what's planned.

## Building

Standard library only, no external crates:

```
cargo build --release
```

## License

MIT, see [LICENSE](LICENSE).
