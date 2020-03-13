use crate::{
    ast,
    context::{Context, Symbol},
};

/// Populate a context with a parsed grammar AST.
pub fn add_ast(ctx: &mut Context, ast: ast::Grammar) {
    info!("Adding grammar with {} NTs to context", ast.nts.len());

    // Register the nonterminal names.
    for nt in &ast.nts {
        let nonterm = ctx.intern_nonterm(&nt.name);
        if nt.public {
            ctx.root_nonterms.insert(nonterm);
            info!("Root nonterminal {}", nonterm);
        }
        trace!("Declared nonterm {}", nonterm);
    }

    // Populate the productions.
    for nt in &ast.nts {
        let nonterm = ctx.intern_nonterm(&nt.name);
        for choice in &nt.choices {
            trace!("Adding {} with {} symbols", nonterm, choice.len());
            let syms = map_symbols(ctx, choice);
            ctx.add_production(nonterm, syms);
        }
    }
}

fn map_symbols<'a>(ctx: &mut Context<'a>, syms: &[ast::Symbol]) -> Vec<Symbol<'a>> {
    let mut output = vec![];
    for sym in syms {
        output.push(map_symbol(ctx, sym));
    }
    output
}

fn map_symbol<'a>(ctx: &mut Context<'a>, sym: &ast::Symbol) -> Symbol<'a> {
    match sym {
        ast::Symbol::Epsilon => Symbol::Epsilon,
        ast::Symbol::Token(name) => ctx
            .lookup_symbol(name)
            .unwrap_or_else(|| ctx.intern_term(name).into()),
        ast::Symbol::Group(syms) => {
            let nonterm = ctx.anonymous_nonterm();
            trace!("Adding group {} with {} symbols", nonterm, syms.len());
            let syms = map_symbols(ctx, syms);
            ctx.add_production(nonterm, syms);
            nonterm.into()
        }
        ast::Symbol::Maybe(sym) => {
            let inner = map_symbol(ctx, sym);
            let outer = ctx.anonymous_nonterm();
            trace!("Adding maybe {} around {}", outer, inner);
            ctx.add_production(outer, vec![Symbol::Epsilon]);
            ctx.add_production(outer, vec![inner]);
            outer.into()
        }
        ast::Symbol::Any(sym) => {
            let inner = map_symbol(ctx, sym);
            let outer_some = ctx.anonymous_nonterm();
            let outer_any = ctx.anonymous_nonterm();
            trace!("Adding any {}/{} around {}", outer_some, outer_any, inner);
            ctx.add_production(outer_some, vec![inner]);
            ctx.add_production(outer_some, vec![outer_some.into(), inner]);
            ctx.add_production(outer_any, vec![outer_some.into()]);
            ctx.add_production(outer_any, vec![Symbol::Epsilon]);
            outer_any.into()
        }
        ast::Symbol::Some(sym) => {
            let inner = map_symbol(ctx, sym);
            let outer = ctx.anonymous_nonterm();
            trace!("Adding some {} around {}", outer, inner);
            ctx.add_production(outer, vec![inner]);
            ctx.add_production(outer, vec![outer.into(), inner]);
            outer.into()
        }
    }
}