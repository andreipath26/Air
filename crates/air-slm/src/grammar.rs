//! Output grammars for structured, filler-free local SLM completions.
//!
//! The GBNF definitions intentionally describe practical code/diff subsets rather
//! than the complete Rust or Python languages. They are passed to llama.cpp for
//! token-constrained generation; `filter_output` also rejects obvious framing
//! text so deterministic/headless backends exercise the same contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputGrammar {
    Rust,
    Python,
    UnifiedDiff,
}

impl OutputGrammar {
    /// GBNF grammar accepted by llama.cpp's `/v1/chat/completions` endpoint.
    pub const fn gbnf(self) -> &'static str {
        match self {
            Self::Rust => RUST_GBNF,
            Self::Python => PYTHON_GBNF,
            Self::UnifiedDiff => UNIFIED_DIFF_GBNF,
        }
    }

    /// Reject conversational wrappers and output that does not start in the
    /// selected structured format. This complements, but cannot replace, the
    /// server-side token grammar.
    pub fn filter_output(self, text: &str) -> Result<String, String> {
        let output = text.trim();
        if output.is_empty() {
            return Err("grammar-constrained output is empty".into());
        }
        if output.contains("```") {
            return Err("grammar-constrained output must not contain Markdown fences".into());
        }
        let first = output.lines().find(|line| !line.trim().is_empty()).unwrap_or("");
        let valid = match self {
            Self::Rust => {
                let line = first.trim_start();
                [
                    "pub ", "pub(", "fn ", "async fn ", "struct ", "enum ",
                    "trait ", "impl ", "use ", "mod ", "type ", "const ",
                    "static ", "#[", "//",
                ].iter().any(|prefix| line.starts_with(prefix))
            }
            Self::Python => {
                let line = first.trim_start();
                [
                    "def ", "async def ", "class ", "import ", "from ",
                    "@", "#", "if ", "for ", "while ", "try:", "with ",
                    "match ", "assert ", "raise ", "return ",
                ].iter().any(|prefix| line.starts_with(prefix))
            }
            Self::UnifiedDiff => {
                first.starts_with("--- ") || first.starts_with("diff --git ")
            }
        };
        if !valid {
            return Err(format!("output does not begin with the selected {:?} grammar", self));
        }
        if self == Self::UnifiedDiff
            && !output.lines().any(|line| line.starts_with("+++ "))
                && !output.starts_with("diff --git ")
        {
            return Err("unified diff is missing its +++ file header".into());
        }
        Ok(output.to_owned())
    }
}

// A structured source subset covering declarations, function signatures, and
// balanced code blocks. The block rule is recursive so nested Rust blocks work.
const RUST_GBNF: &str = r#"root ::= ws item (ws item)*
item ::= function | struct_decl | enum_decl | trait_decl | use_decl | type_decl | const_decl | impl_decl | comment
function ::= visibility "async "? "fn " identifier generics? "(" text ")" return_type? ws block
struct_decl ::= visibility "struct " identifier generics? ws (";" | "{" text "}")
enum_decl ::= visibility "enum " identifier generics? ws "{" text "}"
trait_decl ::= visibility "trait " identifier generics? ws "{" text "}"
impl_decl ::= "impl " generics? text "{" (block | text)* "}"
use_decl ::= visibility "use " text ";"
type_decl ::= visibility "type " identifier generics? ws "=" text ";"
const_decl ::= visibility ("const " | "static ") identifier ws ":" text "=" text ";"
comment ::= "//" text
visibility ::= "pub " | "pub(crate) " | ""
return_type ::= " -> " text
generics ::= "<" text ">"
identifier ::= [A-Za-z_][A-Za-z0-9_]*
block ::= "{" (block | block_char)* "}"
block_char ::= [^{}]
text ::= [^;{}()]+
ws ::= [ \t\r\n]*
"#;

const PYTHON_GBNF: &str = r#"root ::= statement (newline statement)*
statement ::= function | async_function | class | import | assignment | control | comment
function ::= indent? "def " identifier "(" text ")" text? ":"
async_function ::= indent? "async def " identifier "(" text ")" text? ":"
class ::= indent? "class " identifier text? ":"
import ::= indent? ("import " | "from ") text
assignment ::= indent? identifier text? "=" text
control ::= indent? ("if " | "elif " | "for " | "while " | "with " | "return " | "raise " | "assert ") text? ":"
comment ::= indent? "#" text
identifier ::= [A-Za-z_][A-Za-z0-9_]*
indent ::= " "*
text ::= [^\r\n]+
newline ::= "\r"? "\n"
"#;

const UNIFIED_DIFF_GBNF: &str = r#"root ::= file_header hunk+
file_header ::= "--- " line newline "+++ " line newline
hunk ::= "@@ " line "@@" newline diff_line*
diff_line ::= ("+" | "-" | " ") line newline | "\\ No newline at end of file" newline
line ::= [^\r\n]*
newline ::= "\r"? "\n"
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_grammars_have_root_rules_and_structured_start() {
        for grammar in [OutputGrammar::Rust, OutputGrammar::Python, OutputGrammar::UnifiedDiff] {
            assert!(grammar.gbnf().starts_with("root ::= "));
            assert!(grammar.gbnf().contains("\n"));
        }
        assert!(OutputGrammar::Rust.filter_output("pub fn answer() -> u32 { 42 }").is_ok());
        assert!(OutputGrammar::Python.filter_output("def answer():\n    return 42").is_ok());
        assert!(OutputGrammar::UnifiedDiff.filter_output("--- a/src.rs\n+++ b/src.rs\n@@ -1 +1 @@\n-old\n+new").is_ok());
    }

    #[test]
    fn grammar_filter_rejects_filler_and_wrong_format() {
        assert!(OutputGrammar::Rust.filter_output("Here is the Rust code:\npub fn f() {}").is_err());
        assert!(OutputGrammar::Rust.filter_output("```rust\npub fn f() {}\n```").is_err());
        assert!(OutputGrammar::Python.filter_output("Here you go:\ndef f(): pass").is_err());
        assert!(OutputGrammar::UnifiedDiff.filter_output("This is the patch:\n--- a\n+++ b").is_err());
        assert!(OutputGrammar::UnifiedDiff.filter_output("--- a\n@@ -1 +1 @@\n+x").is_err());
    }
}
