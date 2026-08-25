use crate::token::{Keyword, TokenKind};
use crate::tree::{
    Declaration, DeclarationSignature, GenericParam, Item, NotationDecl, NotationFixity, Suite,
    UseTree,
};
use emath_core::Span;

use super::suite_has_section;

impl super::Parser {
    // ---- items ---------------------------------------------------------

    pub(super) fn parse_items(&mut self) {
        self.finish_line();
        while self.peek() != &TokenKind::Eof {
            match self.peek() {
                TokenKind::Keyword(Keyword::Package) => {
                    if let Some(item) = self.parse_package_item() {
                        self.tree_items.push(item);
                    }
                }
                TokenKind::Keyword(Keyword::Use) => {
                    if let Some(item) = self.parse_use_item() {
                        self.tree_items.push(item);
                    }
                }
                TokenKind::Ident(name) if name == "notation" => {
                    if let Some(item) = self.parse_notation_item() {
                        self.tree_items.push(item);
                    }
                }
                TokenKind::Keyword(Keyword::Emath) => match self.parse_declaration() {
                    Some(decl) => self.tree_items.push(Item::Declaration(decl)),
                    None => self.skip_to_line_end(),
                },
                TokenKind::Keyword(Keyword::Extern) => match self.parse_extern_item() {
                    Some(decl) => self.tree_items.push(Item::Declaration(decl)),
                    None => self.skip_to_line_end(),
                },
                TokenKind::Newline | TokenKind::Indent | TokenKind::Dedent => {
                    self.advance();
                }
                _ => {
                    self.error_here("E-SYN-101", "expected an `emath` declaration or `use` item");
                    self.skip_to_line_end();
                }
            }
            self.finish_line();
            self.skip_dedents();
        }
    }

    /// `package examples.square`: package identity line.
    fn parse_package_item(&mut self) -> Option<Item> {
        let start = self.current_span();
        self.advance(); // `package`
        let mut segments = Vec::new();
        loop {
            match self.peek() {
                TokenKind::Ident(name) => {
                    segments.push(name.clone());
                    self.advance();
                }
                TokenKind::PathSep | TokenKind::Dot => {
                    self.advance();
                }
                _ => break,
            }
        }
        if segments.is_empty() {
            self.error_here("E-SYN-110", "expected a package path after `package`");
            return None;
        }
        let mut span = start.cover(self.last_span());
        if self.peek() == &TokenKind::Colon {
            span = span.cover(self.current_span());
        }
        Some(Item::Package {
            path: segments,
            source: span,
        })
    }

    fn parse_use_item(&mut self) -> Option<Item> {
        let start = self.current_span();
        self.advance(); // `use`
        let mut segments = Vec::new();
        let mut tree = None;
        loop {
            match self.peek() {
                TokenKind::Ident(name) => {
                    // `use std.units.{A, B}`: stop before a brace group.
                    if matches!(self.peek_at(1), TokenKind::LBrace) {
                        break;
                    }
                    segments.push(name.clone());
                    self.advance();
                }
                TokenKind::Keyword(Keyword::As) => {
                    self.advance();
                    if let TokenKind::Ident(_) = self.peek() {
                        self.advance();
                    }
                }
                TokenKind::Star => {
                    self.advance();
                    tree = Some(UseTree::All);
                }
                // Uses dotted paths (`use std.numeric.Real`).
                TokenKind::PathSep | TokenKind::Dot => {
                    self.advance();
                }
                TokenKind::LBrace => {
                    self.advance();
                    let mut names = Vec::new();
                    while !matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) {
                        match self.peek() {
                            TokenKind::Ident(name) => {
                                let name = name.clone();
                                self.advance();
                                let alias = if self.eat_keyword(Keyword::As) {
                                    match self.peek().clone() {
                                        TokenKind::Ident(alias) => {
                                            self.advance();
                                            Some(alias)
                                        }
                                        _ => None,
                                    }
                                } else {
                                    None
                                };
                                names.push((name, alias));
                            }
                            TokenKind::Comma => {
                                self.advance();
                            }
                            _ => {
                                self.error_here(
                                    "E-SYN-101",
                                    "expected an identifier or `,` in `use` group",
                                );
                                self.skip_to_line_end();
                                break;
                            }
                        }
                    }
                    self.eat(&TokenKind::RBrace);
                    tree = Some(UseTree::Named(names));
                }
                _ => break,
            }
        }
        if segments.is_empty() && tree.is_none() {
            self.error_here("E-SYN-110", "expected a path after `use`");
            return None;
        }
        Some(Item::Use {
            path: segments,
            tree: tree.unwrap_or(UseTree::Named(Vec::new())),
            source: start.cover(self.last_span()),
        })
    }

    /// Declaration head (unified form): `emath <kind> <Name<Params>>:`;
    /// `emath custom Name:` is the bare custom-kind form. The legacy
    /// `... as kind:` spelling is gone.
    fn parse_declaration(&mut self) -> Option<Declaration> {
        let start = self.current_span();
        self.advance(); // `emath`
                        // The declaration kind is the next word (`custom`,
                        // `function`, `policy`, `record`, `model`, `kind`,
                        // `search`, `experiment`, `type`, or a user kind).
        let item_kind = match self.peek().clone() {
            TokenKind::Ident(item_kind) => item_kind,
            TokenKind::Keyword(Keyword::Custom) => "custom".to_string(),
            _ => {
                self.error_here("E-SYN-101", "expected a declaration kind after `emath`");
                return None;
            }
        };
        self.advance();
        let TokenKind::Ident(name) = self.peek().clone() else {
            self.error_here("E-SYN-101", "expected a declaration name");
            return None;
        };
        self.advance();
        let generics = if matches!(self.peek(), TokenKind::Lt) {
            self.parse_generic_params()
        } else {
            Vec::new()
        };
        // Stateless `emath function name(args) -> T:` head-args. Untyped
        // names (`(x)`) store the `Infer` marker; admission defaults them
        // to Float64 (N-TYPE-001), same as a bare `inputs:` field.
        let signature = if matches!(self.peek(), TokenKind::LParen) {
            let (params, ret) = self.parse_params_after_name_flag(true)?;
            Some(DeclarationSignature { params, ret })
        } else {
            None
        };
        if !self.eat(&TokenKind::Colon) {
            self.error_here("E-SYN-111", "expected `:` after the declaration head");
            return None;
        }
        let suite = self.parse_suite()?;
        // Phase 1 elaboration (admission, goal extraction, codegen) runs off
        // the `custom` compat lane keyed by `as_kind`, so every kind spelled
        // in the unified surface is canonicalized to that representation;
        // `emath custom Name:` keeps an empty `as_kind`.
        let (item_kind, as_kind) = if item_kind == "custom" {
            (item_kind, String::new())
        } else {
            ("custom".to_string(), item_kind)
        };
        if let Some(signature) = &signature {
            self.refuse_head_signature(&as_kind, signature, &suite, start);
        }
        Some(Declaration {
            name,
            generics,
            item_kind,
            as_kind,
            attributes: Vec::new(),
            body: suite.statements,
            signature,
            source: start.cover(self.last_span()),
            head_source: start.cover(self.last_span()),
        })
    }

    /// Head-args are identity-equivalent to an `inputs:` section (`-> T`
    /// to a named output); mixing spellings is a typed refusal.
    fn refuse_head_signature(
        &mut self,
        as_kind: &str,
        signature: &DeclarationSignature,
        suite: &Suite,
        span: Span,
    ) {
        for param in &signature.params {
            if param.by_ref {
                self.diagnostics.error(
                    "E-SYN-101",
                    "by-ref declaration head arguments are outside the Phase 1 subset",
                    param.source,
                );
            }
            if param.default.is_some() {
                self.diagnostics.error(
                    "E-SYN-101",
                    "default values on declaration head arguments are outside the Phase 1 subset",
                    param.source,
                );
            }
        }
        let stateful =
            suite_has_section(suite, "state") || suite_has_section(suite, "constructors");
        if as_kind != "function" || stateful {
            self.diagnostics.error(
                "E-SYN-123",
                "declaration head arguments are only admitted on stateless `emath function` declarations (no `state:` or `constructors:`)",
                span,
            );
        }
        if suite_has_section(suite, "inputs") {
            self.diagnostics.error(
                "E-SYN-122",
                "declaration head arguments cannot be mixed with an `inputs:` section; use one spelling",
                span,
            );
        }
        if signature.ret.is_some() && suite_has_section(suite, "outputs") {
            self.diagnostics.error(
                "E-SYN-122",
                "declaration head `->` return type cannot be mixed with an `outputs:` section; use one spelling",
                span,
            );
        }
    }

    /// `notation infixl 40 "⋅" => core::math::dot [alias "*"]`
    /// Scoped to the package; alias is an alternative spelling; notation is
    /// typography (removing it never changes semantic identity).
    fn parse_notation_item(&mut self) -> Option<Item> {
        let start = self.current_span();
        self.advance(); // `notation`

        // Fixity: prefix | postfix | infixl | infixr | infix
        let fixity = match self.peek() {
            TokenKind::Ident(s) if s == "prefix" => NotationFixity::Prefix,
            TokenKind::Ident(s) if s == "postfix" => NotationFixity::Postfix,
            TokenKind::Ident(s) if s == "infixl" => NotationFixity::InfixLeft,
            TokenKind::Ident(s) if s == "infixr" => NotationFixity::InfixRight,
            TokenKind::Ident(s) if s == "infix" => NotationFixity::Infix,
            _ => {
                self.error_here(
                    "E-SYN-101",
                    "expected fixity (prefix|postfix|infixl|infixr|infix) in notation declaration",
                );
                return None;
            }
        };
        self.advance();

        // Precedence: integer
        let precedence = match self.peek() {
            TokenKind::Int(n) => {
                let n = n.clone();
                self.advance();
                n.parse::<u32>().unwrap_or(0)
            }
            _ => {
                self.error_here("E-SYN-101", "expected precedence integer in notation declaration");
                return None;
            }
        };

        // Glyph: string literal
        let glyph = match self.peek() {
            TokenKind::Str(s) => {
                let s = s.clone();
                self.advance();
                s
            }
            _ => {
                self.error_here("E-SYN-101", "expected glyph string in notation declaration");
                return None;
            }
        };

        // Arrow: => (lexed as TokenKind::Arrow, same as ->)
        if !self.eat(&TokenKind::Arrow) {
            self.error_here("E-SYN-101", "expected `=>` in notation declaration");
            return None;
        }

        // Target path: ident (:: ident)*
        // Stop if we see `alias` followed by a string (N2 alias clause).
        let mut target = Vec::new();
        loop {
            match self.peek() {
                TokenKind::Ident(name) => {
                    // Don't consume `alias` as a path segment — it starts
                    // the optional N2 alias clause.
                    if name == "alias" && matches!(self.peek_at(1), TokenKind::Str(_)) {
                        break;
                    }
                    target.push(name.clone());
                    self.advance();
                }
                TokenKind::PathSep | TokenKind::Dot => {
                    self.advance();
                }
                _ => break,
            }
        }
        if target.is_empty() {
            self.error_here("E-SYN-110", "expected target path after `=>` in notation");
            return None;
        }

        // N2: Optional alias clause: `alias "*"`
        let alias = if self.peek() == &TokenKind::Ident("alias".to_string()) {
            self.advance();
            match self.peek() {
                TokenKind::Str(s) => {
                    let s = s.clone();
                    self.advance();
                    Some(s)
                }
                _ => {
                    self.error_here("E-SYN-101", "expected alias string after `alias`");
                    return None;
                }
            }
        } else {
            None
        };

        Some(Item::Notation(NotationDecl {
            fixity,
            precedence,
            glyph,
            target,
            alias,
            source: start.cover(self.last_span()),
        }))
    }

    /// Top-level `extern operator name<Generics>(params) -> Ret:` `suite`
    /// (`09_parametric_provider`). Becomes a declaration of kind
    /// `extern` / `operator` so the rest of the pipeline sees one shape.
    fn parse_extern_item(&mut self) -> Option<Declaration> {
        let start = self.current_span();
        self.advance(); // `extern`
        let TokenKind::Ident(as_kind) = self.peek().clone() else {
            self.error_here("E-SYN-110", "expected `operator` or `fn` after `extern`");
            return None;
        };
        self.advance();
        let TokenKind::Ident(name) = self.peek().clone() else {
            self.error_here("E-SYN-110", "expected an operator name after `extern`");
            return None;
        };
        self.advance();
        let generics = if matches!(self.peek(), TokenKind::Lt) {
            self.parse_generic_params()
        } else {
            Vec::new()
        };
        let (params, ret) = self.parse_params_after_name()?;
        let suite = if self.eat(&TokenKind::Colon) {
            self.parse_suite()
        } else {
            None
        };
        let source = start.cover(self.last_span());
        Some(Declaration {
            name,
            generics,
            item_kind: "extern".to_string(),
            as_kind,
            attributes: Vec::new(),
            body: suite.map_or_else(Vec::new, |suite| suite.statements),
            signature: Some(DeclarationSignature { params, ret }),
            head_source: source,
            source,
        })
    }

    fn parse_generic_params(&mut self) -> Vec<GenericParam> {
        let mut params = Vec::new();
        if !self.eat(&TokenKind::Lt) {
            return params;
        }
        loop {
            if matches!(self.peek(), TokenKind::Gt) {
                self.advance();
                break;
            }
            let start = self.current_span();
            let TokenKind::Ident(name) = self.peek().clone() else {
                self.error_here("E-SYN-101", "expected a generic parameter name");
                break;
            };
            self.advance();
            let bound = if self.eat(&TokenKind::Colon) {
                self.parse_type_expr()
            } else {
                None
            };
            params.push(GenericParam {
                name,
                bound,
                source: start.cover(self.last_span()),
            });
            if self.eat(&TokenKind::Comma) {
                continue;
            }
            if matches!(self.peek(), TokenKind::Gt) {
                self.advance();
                break;
            }
            self.error_here("E-SYN-101", "expected `,` or `>` in generic parameter list");
            break;
        }
        params
    }
}
