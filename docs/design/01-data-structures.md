# Data structures

Every section opens with the types. The prose after each block says why and
names the upstream code it replaces. Signatures are contracts; bodies are
the coder's job. Where a packet already exists, its id is given; the packet's
`definitions[]` is authoritative if the two ever drift.

---

## 1. Text storage (`kate-text`) — PKT-0002

```rust
/// Position counted in Unicode scalar values (chars). ropey's native unit.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct Offset(pub usize);

/// Half-open [start, end). `new` orders its arguments; start <= end always holds.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct CharRange { start: Offset, end: Offset }

/// Derived view. `col` is chars from line start. Never stored, always computed.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct LineCol { pub line: u32, pub col: u32 }

/// Monotonic. Bumped by exactly one per applied Transaction. Never decreases, not even on undo.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct Revision(pub u64);

/// The one edit primitive. Insert: `old` empty. Delete: `new` empty.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Edit { pub start: Offset, pub old: String, pub new: String }

/// Sorted by start, pairwise non-overlapping, all against the same pre-state. Applied atomically.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Transaction { edits: Vec<Edit> }

pub struct TextBuffer {
    text: ropey::Rope,   // ropey 1.6, default-features = false, features = ["simd"]: only '\n' is a line break
    revision: Revision,
    log: EditLog,        // section 2
}

impl TextBuffer {
    pub fn apply(&mut self, tx: &Transaction) -> Result<Revision, ApplyError>;
    pub fn snapshot(&self) -> ropey::Rope;        // O(1); persistent structure
    pub fn line_col(&self, at: Offset) -> LineCol;
    pub fn offset(&self, at: LineCol) -> Offset;  // clamps col to the line
    pub fn line_count(&self) -> u32;              // >= 1 always
}

pub enum ApplyError {
    OutOfBounds { edit: usize, end: Offset, len: Offset },
    OldTextMismatch { edit: usize, expected: String, found: String },
}
```

**Why a rope.** KTextEditor stores a `std::vector<TextBlock>` of
`std::vector<TextLine>` with `BufferBlockSize = 64`
(`upstream/ktexteditor/src/buffer/katetextbuffer.h:34`,
`katetextblock.h:204`). That is a two-level line array: O(n/64) block
scanning, O(64) line shifting, and an enormous amount of bookkeeping in
`katetextblock.cpp` to keep cursors and ranges attached to blocks as lines
move between them. It works. It is also 1,100 lines of C++ for the buffer
and 630 for the block, before undo.

`ropey` gives O(log n) insert, delete and index conversion in both
directions (char, byte, line), handles multi-gigabyte files without a
special "large file mode", and is a persistent structure: `snapshot()` is a
pointer copy, which is what lets highlighting or search run on a thread while
the user keeps typing. Helix ships on it. We take it as it is; we do not wrap
it behind a storage trait (see the Gate 1 rejection example for what happens
to that idea).

**Why `\n` only.** KTextEditor's loader detects `\r\n`, `\r` and `\n` and
remembers an `EndOfLineMode` (`katetextbuffer.h:52`,
`katetextloader.h:413`); the buffer stores lines without terminators. We do
the same thing differently: `kate-fileio` normalizes to `\n` on load and
remembers `LineEnding`; the rope only ever sees `\n`, so `ropey`'s
`unicode_lines` and `cr_lines` features are off and a "line" means one
thing everywhere. The special cases live in exactly one place, the codec.

**Why flat char offsets, not (line, column).** Every anchor is one integer.
The transform rule for an edit is four comparisons (section 2). Line/column
pairs need two-step transforms when a newline is inserted or removed, which
is most of the complexity in `katetextblock.cpp`. `LineCol` exists for the
things that are genuinely line-shaped: display, marks, LSP, and the per-line
highlight cache, all of which call `line_col()` and pay O(log n) once.

Chars rather than bytes because `ropey`'s edit API is char-indexed and
because UTF-8 byte offsets are a trap for anything that steps through text
(a cursor between the bytes of one codepoint is not a cursor). Grapheme
clusters are a *movement* concern and live in `kate-core::movement`. UTF-16
is an LSP concern and lives in `kate-lsp`. Bytes are a regex concern and the
search crate converts at its boundary.

**Why one edit kind.** `UndoItem::UndoType` in
`upstream/ktexteditor/src/undo/kateundo.h:30` has seven variants:
`editInsertText`, `editRemoveText`, `editWrapLine`, `editUnWrapLine`,
`editInsertLine`, `editRemoveLine`, `editMarkLineAutoWrapped`. Seven
variants means seven `match` arms in undo, seven in redo, seven in cursor
transformation, seven in the swap file. `Edit { start, old, new }` is one
variant whose inverse is `swap(old, new)`. Wrapping a line is inserting
`"\n"`. There is nothing left to special-case.

A `Transaction` is the unit of atomicity: typing with forty cursors is one
transaction of forty edits, one revision bump, one undo entry. Edits are
expressed against the pre-state and applied back to front so earlier offsets
stay valid; `Transaction::new` sorts and rejects overlap up front so `apply`
cannot half-fail.

**Per-line metadata.** KTextEditor keeps highlighting state, attributes,
folding markers and modified/saved flags inside each `TextLine`
(`katetextline.h:64`, `:366`, `:371`). We keep text in the rope and per-line
state in parallel vectors owned by whoever needs it: the syntax cache
(section 8) owns `Vec<StateId>`; marks are anchors. `Vec` insertion is O(n)
on line count; at a million lines that is a sub-millisecond memmove on an
M-series chip, and it is behind a method, so a chunked vector can replace it
when a benchmark says so. Not before.

---

## 2. Anchors and the edit log (`kate-text`) — PKT-0003

```rust
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum Bias { Left, Right }

/// Plain data. Resolve it when you need an Offset. Refresh it when you want to let the log shrink.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct Anchor { pub offset: Offset, pub revision: Revision, pub bias: Bias }

pub struct EditLog {
    entries: VecDeque<Transaction>,          // entry i produced first_revision + i + 1
    first_revision: Revision,
    locks: BTreeMap<Revision, u32>,          // refcounts; log is pruned to the oldest lock
}

impl TextBuffer {
    pub fn anchor(&self, offset: Offset, bias: Bias) -> Anchor;
    pub fn resolve(&self, a: Anchor) -> Result<Offset, StaleAnchor>;
    pub fn refresh(&self, a: Anchor) -> Result<Anchor, StaleAnchor>;
    pub fn transform(&self, o: Offset, bias: Bias, from: Revision, to: Revision) -> Result<Offset, StaleAnchor>;
    pub fn lock_revision(&mut self, r: Revision);
    pub fn unlock_revision(&mut self, r: Revision);
}

/// For one Edit, pre-state offsets:
///   o <  start           -> o
///   o >  old_end         -> o + delta
///   start <= o <= old_end:
///       old empty (insert exactly at o): Left -> o, Right -> o + new_len
///       otherwise (inside replaced span): Left -> start, Right -> start + new_len
pub fn transform_offset(o: Offset, bias: Bias, edit: &Edit) -> Offset;
```

**What this replaces.** `KTextEditor::MovingCursor` and `MovingRange`
(`katetextcursor.h`, `katetextrange.h`) are heap objects registered with
their `TextBlock`; every edit walks the block's cursor list and mutates them
in place; ranges have `InsertBehaviors`, `EmptyBehavior`, feedback
interfaces, and a `View` filter. `TextHistory`
(`katetexthistory.h:23`) is a *second* mechanism for the same problem: a
revision-stamped list of entries with `transformCursor(line, col,
insertBehavior, fromRevision, toRevision)` and `lockRevision`/`unlockRevision`,
used by completion and LSP to map positions that were computed against an
older revision.

`TextHistory` is the good idea. We keep only it. An `Anchor` is a value; the
owner of a diagnostic, a search match, a mark or a fold holds a `Copy` struct
and asks the buffer for the current offset when it draws or acts. Nothing is
registered, nothing is notified, nothing is borrowed across an edit. The
borrow checker is happy and so is the cache.

**Cost model.** Resolution replays the log from `a.revision` to now: O(edits
since) per anchor. Consumers that hold many anchors (10,000 LSP diagnostics)
resolve only the visible ones per frame (tens) and `refresh` the rest in a
batch when they are next touched. Consumers that hold a few anchors (marks,
a search result list) do not care. The log is pruned to the oldest lock;
without locks it holds only the last transaction. An anchor older than the
log is `Err(StaleAnchor)`, not a silently wrong offset. Callers that need
guaranteed resolvability take a lock, exactly like KTextEditor's completion
code does today.

---

## 3. Undo and redo (`kate-text`) — PKT-0004

```rust
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum EditKind { Typing, Deletion, Paste, Replace, Indent, Programmatic }

pub struct HistoryEntry {
    pub forward: Transaction,
    pub inverse: Transaction,
    pub kind: EditKind,
    pub revision_before: Revision,
    pub revision_after: Revision,
    pub ranges_before: Vec<CharRange>,   // selections, primary first
    pub ranges_after: Vec<CharRange>,
}

pub struct History {
    undo: Vec<HistoryEntry>,
    redo: Vec<HistoryEntry>,
    group_open: bool,
    saved_depth: Option<usize>,          // undo.len() at last save; None once unreachable
}

impl History {
    pub fn record(&mut self, entry: HistoryEntry);   // coalesces; clears redo
    pub fn begin_group(&mut self); pub fn end_group(&mut self);
    pub fn undo(&mut self, buf: &mut TextBuffer) -> Option<Vec<CharRange>>;
    pub fn redo(&mut self, buf: &mut TextBuffer) -> Option<Vec<CharRange>>;
    pub fn mark_saved(&mut self);
    pub fn is_modified(&self) -> bool;               // saved_depth != Some(undo.len())
}
```

**What this replaces.** `KateUndoManager` + `KateUndoGroup` + `UndoItem`
(`kateundomanager.h`, `kateundo.h`): 1,400 lines. Groups of typed items,
`ModificationFlags` with eight bits tracking per-line modified/saved state
in both directions (`kateundo.h:41`), merge heuristics
(`KateUndoGroup::merge`), and cursor/selection restore including secondary
cursors (`kateundo.h:196`).

The inverse of a `Transaction` is a `Transaction`, so an entry is two
transactions and undo is `buf.apply(&entry.inverse)`. No per-type undo
logic. The revision keeps going up on undo because undo *is* an edit; anchors
and the LSP version counter both want that.

**Coalescing** is a data rule, not a heuristic: `Typing` coalesces into an
open `Typing` entry when the new edit's `start` equals the previous entry's
`forward` end; `Deletion` likewise in reverse; nothing else coalesces; a
group (`begin_group`/`end_group`) forces coalescing regardless of kind for
the duration. Kate additionally splits typing at word boundaries and on a
timer; we add the word boundary rule in `kate-core` where it belongs (it
needs grapheme and word logic) by closing the group, not by teaching
`History` about words.

**Modified state** is the undo-stack depth at the last save. Undo back to
that depth and the document is unmodified again, which is what users expect
and what Kate's eight flag bits are trying to achieve. If the user undoes past
the saved depth and then types, the saved state is unreachable and
`saved_depth` becomes `None`. Three lines, no flags.

---

## 4. Cursors, selections, multi-cursor (`kate-core`) — PKT-0011

```rust
/// anchor == head is a caret. head moves. goal_col survives vertical motion.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Selection { pub anchor: Offset, pub head: Offset, pub goal_col: Option<u32> }

/// Invariant: sorted by min(), pairwise non-overlapping, primary < len(). Always at least one.
pub struct Selections { ranges: Vec<Selection>, primary: usize }

impl Selections {
    pub fn single(s: Selection) -> Self;
    pub fn new(ranges: Vec<Selection>, primary: usize) -> Self;   // normalizes
    pub fn map(&mut self, f: impl FnMut(Selection) -> Selection); // then normalizes
    pub fn transformed(&self, tx: &Transaction) -> Selections;    // anchor Left, head Right
}

pub enum Movement { Left, Right, WordLeft, WordRight, Up, Down, LineStart, LineEnd, SmartHome, DocStart, DocEnd, Lines(i32) }
pub enum Extend { No, Yes }
pub struct MoveContext { pub tab_width: u8 }

pub fn move_all(buf: &TextBuffer, sel: &mut Selections, m: Movement, extend: Extend, ctx: &MoveContext);

/// One Transaction from one Edit per selection. Forty cursors, one undo entry.
pub fn fan_out(buf: &TextBuffer, sel: &Selections,
               f: impl FnMut(&TextBuffer, Selection) -> Option<Edit>)
               -> Result<(Transaction, Selections), TransactionError>;
```

**What this replaces.** `KTextEditor::ViewPrivate` has a primary cursor, a
primary selection range (a `MovingRange`), and a separate
`m_secondaryCursors` list added when multi-cursor arrived
(`kateview.h`, `kateviewinternal.cpp`, ~10,000 lines together with
helpers). Most editing commands have a "with secondary cursors" branch.

Here there is no primary cursor type. There is `Selections`, which has one
or more ranges, one of which is primary. Every command is written once, for
all selections, via `fan_out`. A single cursor is the `len() == 1` case of
the same code and gets the same tests. This is the "eliminate the special
case through data layout" rule applied to the feature where it pays the most.

Movement is grapheme-cluster aware (`unicode-segmentation`): `Right` over
`e\u{301}` is one step. Vertical movement is by *logical* line with a goal
column in tab-expanded cells, because `kate-core` does not know how the GUI
wraps lines. Visual-line Up/Down under dynamic word wrap is implemented in
`kate-ui`, which knows the layout, by computing the target `Offset` and
issuing `Command::MoveTo`. The core stays layout-free.

---

## 5. Syntax highlighting (`kate-syntax`) — PKT-0007, PKT-0008, PKT-0009

```rust
// definition.rs — the data (PKT-0007)
pub struct DefId(pub u32); pub struct ContextId(pub u32); pub struct FormatId(pub u16);
pub struct KeywordListId(pub u16); pub struct FoldId(pub u16);

pub struct Definition { pub name: String, pub section: String, pub version: u32, pub priority: i32, pub hidden: bool,
    pub extensions: Vec<String>, pub mime_types: Vec<String>, pub case_sensitive_keywords: bool,
    pub indenter: Option<String>, pub word_delimiters: WordDelimiters, pub comments: Vec<CommentStyle>,
    pub indentation_based_folding: bool, pub initial_context: ContextId,
    pub contexts: Range<u32>, pub formats: Range<u16>, pub license: String }

pub struct Context { pub name: String, pub attribute: FormatId, pub rules: Vec<Rule>,
    pub line_end: ContextSwitch, pub line_empty: ContextSwitch, pub fallthrough: Option<ContextSwitch>,
    pub no_indentation_based_folding: bool, pub stop_empty_line_context_switch_loop: bool }

pub enum ContextSwitch { Stay, Pop(u8), Push(ContextId), PopPush(u8, ContextId) }

pub struct Rule { pub kind: RuleKind, pub attribute: Option<FormatId>, pub switch: ContextSwitch,
    pub lookahead: bool, pub first_non_space: bool, pub column: Option<u32>,
    pub begin_region: Option<FoldId>, pub end_region: Option<FoldId>, pub dynamic: bool }

pub enum RuleKind { AnyChar(Vec<char>), DetectChar(char), Detect2Chars(char, char), DetectIdentifier, DetectSpaces,
    Float, Int, HlCChar, HlCHex, HlCOct, HlCStringChar, Keyword { list: KeywordListId, insensitive: bool },
    LineContinue(char), RangeDetect(char, char), RegExpr { pattern: String, insensitive: bool, minimal: bool },
    StringDetect { text: String, insensitive: bool }, WordDetect { word: String, insensitive: bool } }

pub struct Repository { definitions: Vec<Definition>, contexts: Vec<Context>, formats: Vec<Format>,
    keyword_lists: Vec<KeywordList>, fold_names: Vec<String>, by_name: HashMap<String, DefId> }

// state.rs / engine.rs — the machine (PKT-0008)
pub struct Frame { pub context: ContextId, pub captures: Option<Arc<[String]>> }
pub struct State { frames: SmallVec<[Frame; 8]> }          // the context stack at a line end
pub struct StateId(pub u32);                                // interned
pub struct Span { pub start: u32, pub len: u32, pub format: FormatId }
pub enum FoldEvent { Begin { col: u32, region: FoldId }, End { col: u32, region: FoldId } }
pub struct LineHighlight { pub spans: Vec<Span>, pub folds: Vec<FoldEvent>, pub end_state: StateId }

impl Highlighter<'_> {
    pub fn highlight_line(&mut self, line: &str, start: StateId, out: &mut LineHighlight);
}

// theme.rs (PKT-0009)
pub struct Style { pub fg: Option<Rgba>, pub bg: Option<Rgba>, pub selected_fg: Option<Rgba>, pub selected_bg: Option<Rgba>,
    pub bold: bool, pub italic: bool, pub underline: bool, pub strikeout: bool }
impl Theme { pub fn resolve(&self, definition: &str, format: &Format) -> Style; }
```

**The decision.** Kate's highlighting *is* KSyntaxHighlighting: 413 XML
definitions under `upstream/syntax-highlighting/data/syntax/`, 30 JSON themes,
and a ~10,000-line MIT-licensed C++ engine (`src/lib/`). The definitions are
the asset: twenty years of language coverage, maintained by other people,
with a schema (`data/schema/language.xsd`) and a golden test corpus
(`autotests/input` + `autotests/reference`, 381 files). Three options:

| Option | Parity with Kate | Cost | Verdict |
|---|---|---|---|
| **Port the engine, load the XML** | Identical output, 413 languages, same themes | ~3–4k lines of Rust; must handle PCRE-flavoured regex | **Do this.** The golden corpus makes it the best-tested crate in the project. |
| `syntect` (Sublime syntaxes) | Different grammars, different scopes, themes do not map | Low | Loses the one thing users recognize. No. |
| tree-sitter | Better structure (folding, indent, selection), incremental | C grammars per language, build complexity, different model; Kate itself does not use it | Not for v1. If ever, as an *additional* provider for folding and textobjects, not a replacement for highlighting. |

**Engine shape.** Exactly upstream's `AbstractHighlighter::highlightLine`
(`abstracthighlighter.cpp`): walk the rules of the top context left to right;
first match wins; apply `lookAhead`, `firstNonSpace`, `column`; emit a span
with the rule's attribute or the context's; apply the `ContextSwitch`; at end
of line apply `lineEndContext`, at an empty line `lineEmptyContext`; if no
rule matched and `fallthrough` is set, switch and retry. The state at the end
of a line is the context stack, interned to a `StateId` because almost every
line of a source file ends in one of a handful of states. That interned id
is what `kate-core`'s per-line cache stores, and incremental re-highlighting
stops at the first line whose new end state equals its cached one
(`SyntaxCache::ensure`, section 8). `Kate::TextLine::m_highlightingState`
does the same job in upstream.

**Regex.** Definitions use QRegularExpression (PCRE2): lookbehind,
backreferences, `\b` semantics, POSIX classes. The `regex` crate refuses
lookaround by design. `fancy-regex` wraps `regex` and falls back to a
backtracker only for patterns that need it; patterns are compiled once at
`Highlighter::new`, and a pattern `fancy-regex` cannot compile is reported
at load (`RegexError::Unsupported`) rather than panicking mid-line. Dynamic
rules (`%1`..`%9` from the pushing rule's captures) are expanded into the
pattern string and cached in a bounded LRU. This is one of two regex engines
in the project; see section 6 for why that is correct rather than lazy.

**Vendoring policy.** Definitions are copied as data into
`crates/kate-syntax/definitions/` with their headers, listed in `SOURCES.md`
with the upstream commit. Only files whose `license` attribute is MIT (151 of
413) are vendored before the owner decides the project license; the rest
(LGPL, GPL, mixed) follow the decision in `06-licensing.md`. Themes are all
MIT.

---

## 6. Search and replace (`kate-search`) — PKT-0010

```rust
pub enum Pattern { Plain(String), EscapeSequences(String), Regex(String) }

pub struct SearchQuery { pub pattern: Pattern, pub case_sensitive: bool, pub whole_words: bool, pub range: Option<CharRange> }
pub struct Match { pub range: CharRange, pub captures: Vec<Option<CharRange>> }

pub fn find_all(buf: &TextBuffer, q: &SearchQuery) -> Result<Vec<Match>, SearchError>;
pub fn find_next(buf: &TextBuffer, q: &SearchQuery, from: Offset, backwards: bool, wrap: bool) -> Result<Option<Match>, SearchError>;
pub fn replace_all(buf: &TextBuffer, q: &SearchQuery, replacement: &str) -> Result<(Transaction, usize), SearchError>;
```

**What this replaces.** `KatePlainTextSearch`, `KateRegExpSearch` (900
lines, including its own multi-line regex handling over `QString` lines),
`KateMatch`, and the model half of `KateSearchBar` (1,800 lines, mostly
widget). The three search modes in Kate's bar map to the three `Pattern`
variants; "whole words" is a flag because it composes with plain and escaped
search and is spelled `\b` in regex.

**Why `regex` + `regex-cursor` here and `fancy-regex` in highlighting.**
User-typed search patterns must not be able to hang the editor; the `regex`
crate's linear-time guarantee is the feature. `regex-cursor` runs that engine
over rope chunks without copying the buffer into a `String`, which matters at
a hundred megabytes. Highlighting patterns come from vetted definition files
and need PCRE features that `regex` refuses. Two engines, two threat models.
A single "regex abstraction" over both would have the guarantees of neither.

Replace-all returns one `Transaction` (so undo is one step and multi-cursor
edits and replace-all go through the same `apply`) and the count. Capture
expansion supports `$1`, `${name}`, `\1` and the usual escapes, matching
what Kate's bar accepts.

---

## 7. File I/O and encodings (`kate-fileio`) — PKT-0005, PKT-0006

```rust
pub enum LineEnding { Lf, CrLf, Cr }
pub type Encoding = &'static encoding_rs::Encoding;

pub struct DecodeOptions { pub fallback: Encoding, pub max_bytes: u64 }   // default WINDOWS_1252, 2 GiB
pub struct Decoded { pub text: Rope, pub encoding: Encoding, pub bom: bool, pub line_ending: LineEnding,
                     pub had_malformed: bool, pub probably_binary: bool }
pub fn decode(bytes: &[u8], opts: &DecodeOptions) -> Result<Decoded, DecodeError>;

pub struct Loaded { pub decoded: Decoded, pub size_bytes: u64, pub mtime: Option<SystemTime>, pub digest: [u8; 32] }
pub fn load(path: &Path, opts: &DecodeOptions) -> Result<Loaded, LoadError>;

pub struct EncodeOptions { pub encoding: Encoding, pub bom: bool, pub line_ending: LineEnding, pub ensure_trailing_newline: bool }
pub fn encode(text: &Rope, opts: &EncodeOptions) -> Result<Vec<u8>, EncodeError>;   // Unmappable { line, col, ch }

pub struct Saved { pub bytes_written: u64, pub digest: [u8; 32], pub mtime: Option<SystemTime> }
pub fn save(path: &Path, text: &Rope, opts: &EncodeOptions) -> Result<Saved, SaveError>;  // temp + fsync + mode + rename
```

**What this replaces.** `Kate::TextLoader` (`katetextloader.h`: BOM,
`QStringDecoder`, `KEncodingProber`, EOL detection, a line-length limit that
auto-wraps enormous lines, digest), `TextBuffer::save` with `QSaveFile`, and
`KateDocument`'s mod-on-disk machinery (`KDirWatch`, `m_modOnHdTimer`,
`katemwmodonhddialog`).

**Detection order:** BOM (UTF-8, UTF-16 LE/BE) → strict UTF-8 → the
configured fallback. No statistical prober in v1; Kate's `KEncodingProber`
guesses, and a guess that is wrong corrupts a file on save. The fallback is
a setting with a sane default (`windows-1252`, which is what "Latin-1" files
actually are) and `had_malformed` is surfaced in the UI so the user chooses.
The owner can ask for `chardetng` later as its own packet with a benchmark
of how often it is right on their files.

**Save is atomic and refuses to lie.** Write to a sibling temp file, fsync,
copy the target's mode bits, rename. `tempfile::NamedTempFile::persist` is
the primitive. A character the target encoding cannot represent is
`EncodeError::Unmappable { line, col, ch }`; `encoding_rs` would happily
emit `&#8364;` or `?` and Kate shows a warning dialog after the fact. We fail
before the first byte is written and the UI offers UTF-8. If the directory is
not writable, `SaveError::Io { step: CreateTemp }`, no fallback to an in-place
write that can leave a truncated file. Kate's `QSaveFile` has the same policy
and a `directWrite` fallback; we do not add the fallback until someone shows
a real directory where it is needed.

**Modified on disk.** `Document::FileState` stores `digest_on_disk` and
`mtime_on_disk`; a `notify` (FSEvents) watcher in `kate-ui` (it owns the
event loop) posts `Command::CheckDisk`, which re-digests and sets
`DiskChange::{Modified, Deleted, Created}` only when bytes actually differ,
so touching a file or `git checkout` of identical content does not nag.
Phase 2.

**Swap file / crash recovery.** Kate's `KateSwapFile` (670 lines) journals
edit primitives in its own binary format. Our undo entries already *are*
invertible transactions; a recovery journal is `HistoryEntry` serialized to
`~/Library/Application Support/KateM5Rust/recovery/<digest>.jsonl`,
appended per transaction, truncated on save. No second edit format. Phase 3.

---

## 8. Document, view, workspace (`kate-core`) — PKT-0012

```rust
slotmap::new_key_type! { pub struct DocumentId; pub struct ViewId; }

pub struct Document {
    pub buffer: TextBuffer,
    pub history: History,
    pub file: Option<FileState>,
    pub syntax: SyntaxCache,                 // definition + Vec<StateId> per line + dirty_from
    pub marks: Vec<Mark>,                    // Mark { anchor: Anchor, kind: MarkKind }
    pub name: String,
    pub config: DocumentConfig,              // tab_width, indent_width, indent_with_spaces, trailing-space policy
}

pub struct View {
    pub document: DocumentId,
    pub selections: Selections,
    pub scroll: Scroll,                      // first_line, first_col
    pub folds: Vec<CharRange>,               // Phase 2: FoldRegion { start: Anchor, end: Anchor, folded }
    pub dynamic_wrap: bool,
}

pub struct Workspace {
    pub documents: SlotMap<DocumentId, Document>,
    pub views: SlotMap<ViewId, View>,
    pub layout: SplitTree,                   // Leaf { tabs: Vec<ViewId>, active } | Split { axis, ratio, first, second }
    pub active_view: Option<ViewId>,
    pub syntax: Repository,
    pub theme: Theme,
    pub config: EditorConfig,
    events: Vec<Event>,                      // outbox; the shell drains it after execute()
}

impl Workspace {
    pub fn execute(&mut self, view: ViewId, cmd: Command) -> Result<(), CommandError>;
    pub fn drain_events(&mut self) -> Vec<Event>;
}

pub enum Command { InsertText(String), Newline, DeleteBackward, DeleteForward, Move(Movement, Extend), MoveTo(Selections),
    SelectAll, AddCursorBelow, AddCursorAbove, RemoveSecondaryCursors, Undo, Redo, BeginUndoGroup, EndUndoGroup,
    Save, SaveAs(PathBuf), Reload, CheckDisk, SetSyntax(Option<DefId>), ToggleMark(MarkKind, u32), Scroll(i32), ScrollTo(u32) }

pub enum Event { DocumentOpened(DocumentId), DocumentChanged { doc: DocumentId, revision: Revision, lines: Range<u32> },
    DocumentSaved(DocumentId), DocumentClosed(DocumentId), SelectionsChanged(ViewId), DiskChanged(DocumentId, DiskChange) }
```

**What this replaces.** `KTextEditor::DocumentPrivate` (6,578 lines,
`katedocument.cpp`), `ViewPrivate` (5,325) and `KateViewInternal` (4,965),
`KateDocManager`, `KateViewManager`, `KateViewSpace`, `KateSplitter`, plus
Qt's signal/slot graph connecting all of them.

**One owning tree.** `Workspace` owns everything through `SlotMap`s with
generational keys: closing a view invalidates its `ViewId`; a stale id
returns `None` rather than a dangling pointer. A document with two views
(Kate's split-view-on-same-file) is a `Document` with two `View`s pointing at
its `DocumentId`; an edit through either view transforms the other's
selections via `Selections::transformed` inside `execute`. No `QObject`
parent chains, no `QPointer`, no `deleteLater`.

**One mutation entry point.** `execute(view, Command)` is the only way state
changes. It applies the transaction, records history, transforms every
view's selections on that document, updates the syntax cache's dirty line,
pushes `Event`s. Keybindings, menus, the command line, vi mode (later), LSP
edits (later) and tests all speak `Command`. That enum is the plugin API and
the scripting API, which is why it is data and not a trait.

**Events are an outbox, not callbacks.** The shell calls `execute`, then
`drain_events`, then redraws what the events name. No re-entrancy, no
"signal emitted while in the middle of an edit", no borrow conflicts.

**Incremental highlighting** lives in `SyntaxCache`: `on_transaction` splices
the per-line `Vec<StateId>` to the new line count and sets `dirty_from`;
`ensure(buf, highlighter, through_line)` re-highlights from `dirty_from`
until a line's new end state equals its cached one (or `through_line`), and
returns the changed line range for the renderer. Editing inside a function
re-highlights a handful of lines; opening a block comment re-highlights to
the end, as it must.

**Marks** are `Anchor`s at line starts, so they move with the text and
resolve to a line via `line_col()`. Kate's `QHash<int, Mark*>` keyed by line
number has to be rewritten on every line insertion
(`katedocument.h:535`); we do nothing.

---

## 9. Sessions and projects (`kate-session`) — Phase 2

```rust
#[derive(Serialize, Deserialize)]
pub struct Session {
    pub format_version: u32,                 // 1
    pub name: String,
    pub documents: Vec<SessionDocument>,     // path, encoding label, syntax name, cursor LineCol, scroll_line, bookmarks, pinned
    pub layout: SessionLayout,               // mirrors SplitTree with document indices
    pub window: Option<WindowGeometry>,
    pub last_used: SystemTime,
}
pub fn sessions_dir() -> PathBuf;            // ~/Library/Application Support/KateM5Rust/sessions/
pub fn load(name: &str) -> Result<Session, SessionError>;
pub fn save(s: &Session) -> Result<(), SessionError>;   // through kate_fileio::save for atomicity

#[derive(Deserialize)]                        // reads Kate's .kateproject JSON; never writes it
pub struct Project {
    pub name: String,
    pub base_dir: PathBuf,
    pub files: Vec<FileSource>,              // Git { }, Directory { filters, recursive, hidden }, List(Vec<PathBuf>)
    pub exclude_patterns: Vec<String>,
    pub build: Option<BuildConfig>,          // directory, targets[]
}
```

Kate sessions are KConfig INI files (`katesessionmanager.cpp`:
`[Open Documents]`, `[DocManager]`, `[KateViewManager]`, `[General]`) full
of Qt-serialized geometry. We do not read them; a one-shot importer can be a
packet when a user asks. Our format is JSON with `format_version`, written
atomically. `.kateproject` is JSON already, documented by its keys in
`kateprojectworker.cpp` (`files`, `git`, `directory`, `filters`, `recursive`,
`hidden`, `exclude_patterns`, `build`), and the owner has such files, so we
read them as-is. That is userspace; we do not break it.

---

## 10. Plugin boundary — Phase 3, by design not by trait

Kate has two plugin systems: C++ `KTextEditor::Plugin` shared objects
(36 addons, 100k lines) and QJSEngine scripts (indenters, commands). We have
neither in v1 and we are not building a plugin *framework*.

The boundary is the data already defined: `Command` in, `Event` out, the
public API of `Workspace` for reads. A "plugin" is a Rust crate linked at
build time (`kate-lsp`, `kate-project`, later `kate-vimode`) that holds its
own state, receives drained `Event`s from the shell, and issues `Command`s.
No ABI, no `dlopen`, no WASM runtime, no JS engine. When there are two such
crates with the same shape, a `trait Plugin` with `fn on_event(&mut self,
&Event, &mut Workspace)` may be extracted; until then it would have one
implementor and Gate 1 rejects it.

Indentation scripts are the one place Kate's JS matters to users. The
indenters (`upstream/ktexteditor/src/script/data/indentation/*.js`: cstyle,
python, cmake, xml, …) are ported to Rust one per packet, selected by the
syntax definition's `indenter` attribute exactly as today. v1 ships `normal`
(copy previous indent) and `cstyle`. Note `cstyle.js` is `LGPL-2.0-only`;
see `06-licensing.md`.

---

## 11. LSP client (`kate-lsp`) — Phase 3

```rust
#[derive(Deserialize)]                        // same keys as Kate's lspclient settings.json
pub struct ServerConfig { pub command: Vec<String>, pub root_indication_file_names: Vec<String>,
    pub highlighting_mode_regex: String, pub initialization_options: serde_json::Value,
    pub settings: serde_json::Value, pub url: Option<String>, pub use_: Option<String> }

pub enum PositionEncoding { Utf8, Utf16 }     // negotiated via positionEncodings (LSP 3.17); converted here and nowhere else

pub struct LspClient {
    child: std::process::Child,
    outbound: Sender<jsonrpc::Message>,        // writer thread
    inbound: Receiver<jsonrpc::Message>,       // reader thread
    next_id: i64,
    pending: HashMap<i64, PendingRequest>,
    capabilities: Option<lsp_types::ServerCapabilities>,
    encoding: PositionEncoding,
    docs: HashMap<DocumentId, DocumentSync>,   // uri, version, last Revision sent
}

/// Incremental didChange straight from a Transaction. No diffing, no full-text resend.
pub fn content_changes(before: &Rope, tx: &Transaction, enc: PositionEncoding) -> Vec<lsp_types::TextDocumentContentChangeEvent>;

pub struct Diagnostic { pub range: (Anchor, Anchor), pub severity: Severity, pub message: String, pub source: Option<String> }
```

**What this replaces.** `kate/addons/lspclient` (11,660 lines):
`LSPClientServer` (JSON-RPC over `QProcess`), `LSPClientServerManager`,
and a large plugin view. The configuration format (`settings.json` with a
`servers` map keyed by highlighting mode, `command`, `url`,
`rootIndicationFileNames`, `highlightingModeRegex`, `initializationOptions`)
is widely copied by users; we read the same shape so an existing
`settings.json` works unchanged.

Synchronous Rust over threads and pipes, no async runtime: the editor's
event loop belongs to the GUI toolkit, and an LSP client is two threads and
two channels. `lsp-types` for the wire types; the JSON-RPC framing is forty
lines. Transactions map to incremental `didChange` events directly because
an `Edit` *is* a `TextDocumentContentChangeEvent` with a range and new text;
the `Revision` is the LSP `version`. Diagnostics are stored with `Anchor`s so
they stay attached while the user types faster than the server answers, and
positions received from the server against an older `version` are mapped
forward with `TextBuffer::transform` under a revision lock, which is the
reason that mechanism exists.

Semantic tokens, inlay hints, code actions, rename and formatting are each
their own packet after the request/response skeleton lands.
