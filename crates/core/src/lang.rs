//! Languages delve recognises, and the creatures their functions become.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Lang {
    Rust,
    Python,
    JavaScript,
    TypeScript,
    Go,
    Java,
    Kotlin,
    Scala,
    C,
    Cpp,
    CSharp,
    Ruby,
    Php,
    Swift,
    Shell,
    Lua,
    Elixir,
    Haskell,
    OCaml,
    Zig,
    Gleam,
    Dart,
}

/// How a language marks the extent of a function body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blocks {
    /// `{ ... }`
    Braces,
    /// The body is everything indented deeper than the definition.
    Indent,
    /// `do ... end` / `def ... end`: everything until the matching `end`.
    End,
}

impl Lang {
    pub fn from_path(path: &str) -> Option<Lang> {
        let name = path.rsplit('/').next().unwrap_or(path);
        let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase())?;
        Some(match ext.as_str() {
            "rs" => Lang::Rust,
            "py" | "pyi" => Lang::Python,
            "js" | "mjs" | "cjs" | "jsx" => Lang::JavaScript,
            "ts" | "tsx" | "mts" | "cts" => Lang::TypeScript,
            "go" => Lang::Go,
            "java" => Lang::Java,
            "kt" | "kts" => Lang::Kotlin,
            "scala" => Lang::Scala,
            "c" | "h" => Lang::C,
            "cc" | "cpp" | "cxx" | "hpp" | "hh" | "hxx" => Lang::Cpp,
            "cs" => Lang::CSharp,
            "rb" => Lang::Ruby,
            "php" => Lang::Php,
            "swift" => Lang::Swift,
            "sh" | "bash" | "zsh" => Lang::Shell,
            "lua" => Lang::Lua,
            "ex" | "exs" => Lang::Elixir,
            "hs" => Lang::Haskell,
            "ml" | "mli" => Lang::OCaml,
            "zig" => Lang::Zig,
            "gleam" => Lang::Gleam,
            "dart" => Lang::Dart,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Lang::Rust => "Rust",
            Lang::Python => "Python",
            Lang::JavaScript => "JavaScript",
            Lang::TypeScript => "TypeScript",
            Lang::Go => "Go",
            Lang::Java => "Java",
            Lang::Kotlin => "Kotlin",
            Lang::Scala => "Scala",
            Lang::C => "C",
            Lang::Cpp => "C++",
            Lang::CSharp => "C#",
            Lang::Ruby => "Ruby",
            Lang::Php => "PHP",
            Lang::Swift => "Swift",
            Lang::Shell => "Shell",
            Lang::Lua => "Lua",
            Lang::Elixir => "Elixir",
            Lang::Haskell => "Haskell",
            Lang::OCaml => "OCaml",
            Lang::Zig => "Zig",
            Lang::Gleam => "Gleam",
            Lang::Dart => "Dart",
        }
    }

    /// The creature a function in this language turns into.
    pub fn creature(self) -> &'static str {
        match self {
            Lang::Rust => "crab",
            Lang::Python => "serpent",
            Lang::JavaScript => "goblin",
            Lang::TypeScript => "troll",
            Lang::Go => "gopher",
            Lang::Java => "ogre",
            Lang::Kotlin => "kobold",
            Lang::Scala => "scarab",
            Lang::C => "wraith",
            Lang::Cpp => "chimera",
            Lang::CSharp => "harpy",
            Lang::Ruby => "imp",
            Lang::Php => "phantom",
            Lang::Swift => "falcon",
            Lang::Shell => "slime",
            Lang::Lua => "lich",
            Lang::Elixir => "elemental",
            Lang::Haskell => "hydra",
            Lang::OCaml => "camel",
            Lang::Zig => "zombie",
            Lang::Gleam => "nymph",
            Lang::Dart => "drake",
        }
    }

    pub fn blocks(self) -> Blocks {
        match self {
            Lang::Python => Blocks::Indent,
            Lang::Ruby | Lang::Elixir | Lang::Lua => Blocks::End,
            Lang::Haskell | Lang::OCaml => Blocks::Indent,
            Lang::Shell => Blocks::Braces,
            _ => Blocks::Braces,
        }
    }

    /// Line-comment markers.
    pub fn comments(self) -> &'static [&'static str] {
        match self {
            Lang::Python | Lang::Ruby | Lang::Shell | Lang::Elixir => &["#"],
            Lang::Lua | Lang::Haskell => &["--"],
            Lang::Php => &["//", "#"],
            Lang::OCaml => &["(*"],
            _ => &["//", "/*", "*"],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extensions() {
        assert_eq!(Lang::from_path("src/main.rs"), Some(Lang::Rust));
        assert_eq!(Lang::from_path("a/b/App.TSX"), Some(Lang::TypeScript));
        assert_eq!(Lang::from_path("Makefile"), None);
        assert_eq!(Lang::from_path("README.md"), None);
        assert_eq!(Lang::from_path(".hidden/x.py"), Some(Lang::Python));
    }
}
