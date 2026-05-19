# Golden behavior harness (R0 safety net)

This is the regression net for the architecture rewrite. It freezes the
**observable behavior of the current engine** so every later phase (R1–R4) can
prove it did not change document output by accident.

## What it does

`run.ps1` drives the reference engine through the full pipeline using the
self-contained fixtures in `fixtures/`:

1. `render` `sample-render-spec.json` → docx
2. `render` `sample-chem-render-spec.json` → docx
3. `analyze` the rendered docx → analysis JSON
4. `apply` `sample-decision.json` onto the rendered docx → docx

Each produced artifact is normalized — unzip the docx, canonicalize every XML
part, and scrub data that legitimately changes every run (timestamps, OOXML
rsids, GUIDs, random relationship ids, the package core-properties part name,
absolute paths). The normalized text is hashed into `snapshots/`.

Scrubbing is why a mismatch means a **real behavior change**, not zip noise.
Relationship ids are remapped by order of first appearance, so cross-references
between parts are still verified — only the literal random value is neutralized.

## Usage

```powershell
# Build the reference engine first (only needed once / after C# changes):
dotnet build engine/src/docx-auto-template-engine.csproj -c Release -o engine/runtime

# Freeze the baseline (do this once, on untouched main behavior):
pwsh tests/golden/run.ps1 -Mode record

# After any rewrite step, prove behavior is unchanged:
pwsh tests/golden/run.ps1 -Mode verify
```

`verify` exits non-zero and prints the first differing line per case on drift.

## Rules during the rewrite

- **Phases R1 (renderer split):** `verify` MUST stay green. Any drift is a
  refactor bug — fix it, do not re-record.
- **Phases R2–R4 (intentional contract/CLI changes):** behavior may change on
  purpose. Re-record **only** after the diff has been reviewed line by line and
  the change is confirmed intended, and note why in the commit message.
- `snapshots/` and `fixtures/` are committed. `.work/` is scratch (gitignored).
