//! Reading source code: where functions are and how long they run, and the
//! TODO / FIXME / HACK / XXX / BUG comments left behind.
//!
//! This is deliberately a set of fast line-based heuristics, not a parser
//! per language: delve has to read a whole repository in well under a
//! second, and a dungeon only needs to know roughly how big each function
//! is.

use crate::lang::{Blocks, Lang};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Function {
    pub name: String,
    /// 1-based line of the definition.
    pub line: usize,
    /// Number of lines in the definition, including the signature.
    pub len: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum NoteKind {
    Todo,
    Fixme,
    Hack,
    Xxx,
    Bug,
}

impl NoteKind {
    pub fn tag(self) -> &'static str {
        match self {
            NoteKind::Todo => "TODO",
            NoteKind::Fixme => "FIXME",
            NoteKind::Hack => "HACK",
            NoteKind::Xxx => "XXX",
            NoteKind::Bug => "BUG",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Note {
    pub kind: NoteKind,
    pub text: String,
    pub line: usize,
}

#[derive(Clone, Debug, Default)]
pub struct Analysis {
    pub lines: usize,
    pub functions: Vec<Function>,
    pub notes: Vec<Note>,
}

pub fn analyze(lang: Lang, source: &str) -> Analysis {
    let lines: Vec<&str> = source.lines().collect();
    let mut functions = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if let Some(name) = definition(lang, line, lines.get(i + 1).copied()) {
            let len = extent(lang, &lines, i);
            if len >= 1 {
                functions.push(Function {
                    name,
                    line: i + 1,
                    len,
                });
            }
        }
    }
    let notes = lines
        .iter()
        .enumerate()
        .filter_map(|(i, line)| {
            note(lang, line).map(|(kind, text)| Note {
                kind,
                text,
                line: i + 1,
            })
        })
        .collect();
    Analysis {
        lines: lines.len(),
        functions,
        notes,
    }
}

// ---------------------------------------------------------------------------
// Definitions
// ---------------------------------------------------------------------------

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$'
}

/// The identifier at the start of `s` (after leading spaces).
fn ident(s: &str) -> Option<String> {
    let s = s.trim_start();
    let name: String = s
        .chars()
        .take_while(|&c| is_ident_char(c) || c == '!' || c == '?')
        .collect();
    let name = name.trim_end_matches(['!', '?']).to_string();
    if name.is_empty() || name.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        None
    } else {
        Some(name)
    }
}

/// If `keyword` appears as a whole word with only modifier words before it
/// (no `=`, `(`, quotes or comments), return what follows it.
fn after_keyword<'a>(line: &'a str, keyword: &str) -> Option<&'a str> {
    let mut search = 0;
    while let Some(found) = line[search..].find(keyword) {
        let at = search + found;
        let end = at + keyword.len();
        let before_ok = at == 0 || !is_ident_char(line[..at].chars().last().unwrap_or(' '));
        let after_ok = line[end..].starts_with([' ', '\t']);
        if before_ok && after_ok {
            let prefix = &line[..at];
            if prefix.contains(['=', '(', '"', '\'', '`', '#']) || prefix.contains("//") {
                return None;
            }
            return Some(&line[end..]);
        }
        search = end;
    }
    None
}

const CONTROL: &[&str] = &[
    "if",
    "for",
    "while",
    "switch",
    "catch",
    "return",
    "function",
    "else",
    "do",
    "try",
    "new",
    "await",
    "throw",
    "case",
    "when",
    "match",
    "sizeof",
    "typeof",
    "elif",
    "until",
    "unless",
    "foreach",
    "using",
    "lock",
    "synchronized",
    "defer",
    "go",
    "select",
    "with",
    "assert",
];

/// A C-family definition such as `static int parse(char *s) {` or a method
/// `render(props) {`: an identifier followed by `(`, no assignment or
/// semicolon, and a body opening on this line or the next.
fn c_like(line: &str, next: Option<&str>) -> Option<String> {
    let t = line.trim();
    if t.is_empty()
        || t.starts_with(['#', '/', '*', '@', '}', '.', ')'])
        || t.ends_with(';')
        || t.ends_with(',')
    {
        return None;
    }
    let opens = t.ends_with('{') || next.is_some_and(|n| n.trim() == "{");
    if !opens {
        return None;
    }
    let paren = t.find('(')?;
    let head = &t[..paren];
    if head.contains(['=', '.', ':', '"', '\'', '<', '>', '!', '|', '&']) && !head.contains("::") {
        // `a = b(` is a call; `x.y(` is a method call. C++ `Foo::bar(` is fine.
        if !(head.contains('<') && head.contains('>')) {
            return None;
        }
    }
    let name: String = head
        .trim_end()
        .rsplit(|c: char| !is_ident_char(c) && c != '~')
        .next()?
        .to_string();
    let name = name.trim_start_matches('~').to_string();
    if name.is_empty() || CONTROL.contains(&name.as_str()) || name.chars().next()?.is_ascii_digit()
    {
        return None;
    }
    Some(name)
}

/// The name defined on this line, if any.
pub fn definition(lang: Lang, line: &str, next: Option<&str>) -> Option<String> {
    let t = line.trim_start();
    // comment lines never define anything (`#` starts comments in these)
    let hash_comment = matches!(lang, Lang::Python | Lang::Ruby | Lang::Shell | Lang::Elixir)
        && t.starts_with('#');
    if t.starts_with("//") || hash_comment {
        return None;
    }
    match lang {
        Lang::Rust | Lang::Zig | Lang::Gleam => after_keyword(t, "fn").and_then(ident),
        Lang::Go => {
            let rest = after_keyword(t, "func")?;
            let rest = rest.trim_start();
            if let Some(receiver) = rest.strip_prefix('(') {
                // method: func (r *T) Name(
                let close = receiver.find(')')?;
                ident(&receiver[close + 1..])
            } else {
                ident(rest)
            }
        }
        Lang::Swift => after_keyword(t, "func").and_then(ident),
        Lang::Kotlin => {
            let rest = after_keyword(t, "fun")?;
            // extension functions: fun String.shout()
            let rest = rest.trim_start();
            let rest = match (rest.find('.'), rest.find('(')) {
                (Some(dot), Some(paren)) if dot < paren => &rest[dot + 1..],
                _ => rest,
            };
            ident(rest)
        }
        Lang::Scala => after_keyword(t, "def").and_then(ident),
        Lang::Python => after_keyword(t, "def").and_then(ident),
        Lang::Ruby => {
            let rest = after_keyword(t, "def")?.trim_start();
            let rest = rest.strip_prefix("self.").unwrap_or(rest);
            ident(rest)
        }
        Lang::Elixir => after_keyword(t, "defp")
            .or_else(|| after_keyword(t, "def"))
            .or_else(|| after_keyword(t, "defmacro"))
            .and_then(ident),
        Lang::Lua => {
            let rest = after_keyword(t, "function")?;
            let name: String = rest
                .trim_start()
                .chars()
                .take_while(|&c| is_ident_char(c) || c == '.' || c == ':')
                .collect();
            name.rsplit(['.', ':'])
                .next()
                .filter(|n| !n.is_empty())
                .map(str::to_string)
        }
        Lang::Php => after_keyword(t, "function").and_then(ident),
        Lang::Shell => {
            if let Some(rest) = after_keyword(t, "function") {
                return ident(rest);
            }
            let name = ident(t)?;
            let rest = t[name.len()..].trim_start();
            if rest.starts_with("()")
                && (rest.ends_with('{') || next.is_some_and(|n| n.trim() == "{"))
            {
                Some(name)
            } else {
                None
            }
        }
        Lang::JavaScript | Lang::TypeScript => {
            if let Some(rest) = after_keyword(t, "function") {
                let rest = rest.trim_start().trim_start_matches('*');
                return ident(rest);
            }
            // const name = (...) => / const name = async function
            for kw in ["const", "let", "var"] {
                if let Some(rest) = after_keyword(t, kw) {
                    let name = ident(rest)?;
                    let after = &rest.trim_start()[name.len()..];
                    let value = after.split_once('=').map(|(_, v)| v.trim_start())?;
                    if value.starts_with("function")
                        || value.starts_with("async")
                        || (value.contains("=>")
                            && (value.starts_with('(')
                                || value.starts_with(&*name)
                                || value.chars().next().is_some_and(is_ident_char)))
                    {
                        return Some(name);
                    }
                    return None;
                }
            }
            c_like(line, next)
        }
        Lang::Haskell => {
            if line.starts_with([' ', '\t']) || t.is_empty() {
                return None;
            }
            let name = ident(t)?;
            const KW: &[&str] = &[
                "import", "module", "data", "type", "class", "instance", "newtype", "deriving",
                "where", "let", "in", "infixl", "infixr", "infix",
            ];
            if KW.contains(&name.as_str()) || !name.chars().next()?.is_lowercase() {
                return None;
            }
            let rest = &t[name.len()..];
            if rest.trim_start().starts_with("::") {
                None // a type signature; the equation below defines it
            } else if rest.contains('=') {
                Some(name)
            } else {
                None
            }
        }
        Lang::OCaml => {
            if line.starts_with([' ', '\t']) {
                return None;
            }
            let rest = after_keyword(t, "let").or_else(|| after_keyword(t, "and"))?;
            let rest = rest.trim_start();
            let rest = rest.strip_prefix("rec ").unwrap_or(rest);
            let name = ident(rest)?;
            if name == "_" || name == "open" {
                None
            } else {
                Some(name)
            }
        }
        Lang::Java | Lang::CSharp | Lang::C | Lang::Cpp | Lang::Dart => c_like(line, next),
    }
}

// ---------------------------------------------------------------------------
// Extents
// ---------------------------------------------------------------------------

fn indent_of(line: &str) -> usize {
    line.chars()
        .take_while(|c| c.is_whitespace())
        .map(|c| if c == '\t' { 4 } else { 1 })
        .sum()
}

/// How many lines the definition starting at `start` spans.
pub fn extent(lang: Lang, lines: &[&str], start: usize) -> usize {
    match lang.blocks() {
        Blocks::Braces => braces(lines, start),
        Blocks::Indent => indented(lang, lines, start),
        Blocks::End => ended(lines, start),
    }
}

fn braces(lines: &[&str], start: usize) -> usize {
    let mut depth: i32 = 0;
    let mut opened = false;
    for (offset, line) in lines[start..].iter().enumerate() {
        let mut in_string: Option<char> = None;
        let mut prev = ' ';
        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            match in_string {
                Some(q) => {
                    if c == q && prev != '\\' {
                        in_string = None;
                    }
                }
                None => {
                    if c == '/' && chars.get(i + 1) == Some(&'/') {
                        break;
                    }
                    match c {
                        '"' | '`' => in_string = Some(c),
                        '\'' => {
                            // a char literal like '{' — skip it; lifetimes ('a) are harmless
                            if chars.get(i + 2) == Some(&'\'') {
                                i += 2;
                            } else if chars.get(i + 1) == Some(&'\\')
                                && chars.get(i + 3) == Some(&'\'')
                            {
                                i += 3;
                            }
                        }
                        '{' => {
                            depth += 1;
                            opened = true;
                        }
                        '}' => depth -= 1,
                        _ => {}
                    }
                }
            }
            prev = if prev == '\\' && c == '\\' { ' ' } else { c };
            i += 1;
        }
        if opened && depth <= 0 {
            return offset + 1;
        }
        if !opened && offset >= 3 {
            // a declaration without a body
            return 0;
        }
    }
    if opened {
        lines.len() - start
    } else {
        0
    }
}

fn indented(lang: Lang, lines: &[&str], start: usize) -> usize {
    let base = indent_of(lines[start]);
    let mut last = start;
    for (i, line) in lines.iter().enumerate().skip(start + 1) {
        if line.trim().is_empty() {
            continue;
        }
        let deeper = indent_of(line) > base;
        // Haskell/OCaml equations continue on lines that start with `|` or are indented
        let continues =
            matches!(lang, Lang::Haskell | Lang::OCaml) && line.trim_start().starts_with('|');
        if deeper || continues || line.trim_start().starts_with(')') {
            last = i;
        } else {
            break;
        }
    }
    last - start + 1
}

fn ended(lines: &[&str], start: usize) -> usize {
    let opener = |t: &str| {
        let first = t.split_whitespace().next().unwrap_or("");
        let opens_keyword = matches!(
            first,
            "def"
                | "defp"
                | "defmacro"
                | "defmodule"
                | "if"
                | "unless"
                | "case"
                | "cond"
                | "while"
                | "until"
                | "for"
                | "begin"
                | "class"
                | "module"
                | "function"
                | "repeat"
                | "receive"
                | "try"
                | "with"
                | "quote"
                | "fn"
        );
        let single_line =
            t.ends_with(" end") || t.contains(", do:") || t.ends_with(" then return end");
        let ends_do =
            t.ends_with(" do") || t.ends_with(" then") || (t.contains(" do |") && t.ends_with('|'));
        (opens_keyword || ends_do) && !single_line
    };
    let mut depth = 0i32;
    for (offset, line) in lines[start..].iter().enumerate() {
        let t = line.trim();
        if offset == 0 {
            if t.contains(", do:") || t.ends_with(" end") {
                return 1; // one-line definition
            }
            depth = 1;
            continue;
        }
        if t == "end"
            || t.starts_with("end ")
            || t.starts_with("end)")
            || t.starts_with("end,")
            || t == "end;"
        {
            depth -= 1;
            if depth == 0 {
                return offset + 1;
            }
        } else if opener(t) {
            depth += 1;
        }
    }
    lines.len() - start
}

// ---------------------------------------------------------------------------
// Notes
// ---------------------------------------------------------------------------

/// A TODO-style note in a comment on this line.
pub fn note(lang: Lang, line: &str) -> Option<(NoteKind, String)> {
    let comment_at = lang
        .comments()
        .iter()
        .filter_map(|marker| line.find(marker))
        .min()?;
    let comment = &line[comment_at..];
    const TAGS: [(&str, NoteKind); 5] = [
        ("FIXME", NoteKind::Fixme),
        ("TODO", NoteKind::Todo),
        ("HACK", NoteKind::Hack),
        ("XXX", NoteKind::Xxx),
        ("BUG", NoteKind::Bug),
    ];
    let (pos, tag, kind) = TAGS
        .iter()
        .filter_map(|(tag, kind)| {
            let mut search = 0;
            while let Some(found) = comment[search..].find(tag) {
                let at = search + found;
                let end = at + tag.len();
                let before = comment[..at].chars().last();
                let after = comment[end..].chars().next();
                let word = before.is_none_or(|c| !c.is_alphanumeric())
                    && after.is_none_or(|c| !c.is_alphanumeric());
                if word {
                    return Some((at, *tag, *kind));
                }
                search = end;
            }
            None
        })
        .min_by_key(|t| t.0)?;
    let rest = &comment[pos + tag.len()..];
    // TODO(alice): ...  /  TODO: ...  /  TODO - ...
    let rest = if rest.starts_with('(') {
        rest.find(')').map(|i| &rest[i + 1..]).unwrap_or(rest)
    } else {
        rest
    };
    let text = rest
        .trim_start_matches([':', '-', ' ', '!', '\t'])
        .trim_end_matches(['*', '/', ' ', ')', '-'])
        .trim();
    let text: String = if text.chars().count() > 70 {
        let cut: String = text.chars().take(67).collect();
        format!("{}...", cut.trim_end())
    } else {
        text.to_string()
    };
    Some((kind, text))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(lang: Lang, src: &str) -> Vec<(String, usize)> {
        analyze(lang, src)
            .functions
            .into_iter()
            .map(|f| (f.name, f.len))
            .collect()
    }

    #[test]
    fn rust() {
        let src = "pub fn parse(s: &str) -> Result<(), E> {\n    if x { y }\n    let c = '{';\n}\n\nasync fn go() {}\nfn decl();\n";
        assert_eq!(
            names(Lang::Rust, src),
            vec![("parse".into(), 4), ("go".into(), 1)]
        );
    }

    #[test]
    fn python() {
        let src = "class A:\n    def run(self):\n        x = 1\n\n        return x\n    async def stop(self): pass\ndef top():\n    pass\n";
        assert_eq!(
            names(Lang::Python, src),
            vec![("run".into(), 4), ("stop".into(), 1), ("top".into(), 2)]
        );
    }

    #[test]
    fn javascript() {
        let src = "export default function App(props) {\n  return 1;\n}\nconst add = (a, b) => {\n  return a + b;\n};\nclass X {\n  render() {\n    if (a) {\n    }\n  }\n}\nfoo(bar);\n";
        assert_eq!(
            names(Lang::JavaScript, src),
            vec![("App".into(), 3), ("add".into(), 3), ("render".into(), 4)]
        );
    }

    #[test]
    fn go_and_c() {
        let src =
            "func (s *Server) Serve(l net.Listener) error {\n\treturn nil\n}\nfunc main() {\n}\n";
        assert_eq!(
            names(Lang::Go, src),
            vec![("Serve".into(), 3), ("main".into(), 2)]
        );
        let c = "static int\nparse(const char *s)\n{\n  return 0;\n}\nint main(void) {\n  if (x) {\n  }\n  return 0;\n}\n";
        assert_eq!(
            names(Lang::C, c),
            vec![("parse".into(), 4), ("main".into(), 5)]
        );
    }

    #[test]
    fn ruby_and_elixir() {
        let rb = "class A\n  def call(x)\n    if x\n      1\n    end\n  end\n  def self.build = new\nend\n";
        assert_eq!(names(Lang::Ruby, rb)[0], ("call".into(), 5));
        let ex = "defmodule M do\n  def run(x) do\n    case x do\n      1 -> :a\n    end\n  end\n  defp p(x), do: x\nend\n";
        assert_eq!(
            names(Lang::Elixir, ex),
            vec![("run".into(), 5), ("p".into(), 1)]
        );
    }

    #[test]
    fn notes() {
        let src = "// TODO(bob): handle unicode\nlet todo = 1; // FIXME this breaks on Windows\n/* HACK */\n// not a TODOS\nlet s = \"TODO in a string\";\n";
        let n = analyze(Lang::Rust, src).notes;
        assert_eq!(n.len(), 3);
        assert_eq!(n[0].kind, NoteKind::Todo);
        assert_eq!(n[0].text, "handle unicode");
        assert_eq!(n[1].kind, NoteKind::Fixme);
        assert_eq!(n[1].text, "this breaks on Windows");
        assert_eq!(n[2].kind, NoteKind::Hack);
        assert_eq!(n[2].text, "");
        assert_eq!(n[1].line, 2);
    }
}
