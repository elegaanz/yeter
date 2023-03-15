use proc_macro2::{Ident, Span};
use quote::quote;
use syn::punctuated::Punctuated;
use syn::{
    parse_macro_input, Expr, ExprField, ExprPath, ExprTuple, FnArg, Index, ItemFn, Member, Pat,
    PatIdent, PatType, Path, ReturnType, Token, Type, TypeTuple,
};

fn fn_arg_to_type<'a>(arg: &FnArg) -> &Type {
    match arg {
        FnArg::Receiver(_) => unimplemented!(),
        FnArg::Typed(arg) => arg.ty.as_ref(),
    }
}

fn build_type_tuple(types: impl Iterator<Item = Type>) -> Type {
    let mut elems = types.collect::<Punctuated<_, Token![,]>>();
    if !elems.is_empty() {
        elems.push_punct(Default::default());
    }

    Type::Tuple(TypeTuple {
        paren_token: Default::default(),
        elems,
    })
}

fn build_unit_tuple() -> Type {
    build_type_tuple([].into_iter())
}

fn arg_names(count: u32) -> Vec<Ident> {
    (0..count)
        .map(|n| Ident::new(&format!("arg{n}"), Span::mixed_site()))
        .collect()
}

fn calling_tuple_args(idents: impl Iterator<Item = (Ident, Type)>) -> Punctuated<FnArg, Token![,]> {
    idents
        .map(|(name, typ)| {
            FnArg::Typed(PatType {
                attrs: Default::default(),
                pat: Box::new(Pat::Ident(PatIdent {
                    attrs: Default::default(),
                    by_ref: None,
                    mutability: None,
                    subpat: None,
                    ident: name,
                })),
                colon_token: Default::default(),
                ty: Box::new(typ),
            })
        })
        .collect()
}

fn build_ident_tuple(idents: impl Iterator<Item = Ident>) -> Expr {
    let mut elems = idents
        .map(ident_to_expr)
        .collect::<Punctuated<_, Token![,]>>();
    if !elems.is_empty() {
        elems.push_punct(Default::default());
    }

    ExprTuple {
        attrs: Default::default(),
        paren_token: Default::default(),
        elems,
    }
    .into()
}

fn ident_to_expr(id: Ident) -> Expr {
    ExprPath {
        attrs: Default::default(),
        qself: Default::default(),
        path: Path::from(id),
    }
    .into()
}

#[proc_macro_attribute]
pub fn query(
    attr: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    assert!(
        attr.is_empty(),
        "#[yeter::query] doesn't accept any attributes"
    );

    let mut function = parse_macro_input!(item as ItemFn);
    let query_attrs = std::mem::take(&mut function.attrs);
    let fn_args = &function.sig.inputs;
    let query_args = fn_args
        .iter()
        .skip(1)
        .map(fn_arg_to_type)
        .cloned()
        .collect::<Vec<_>>();

    let db_ident = match fn_args
        .first()
        .expect("query must take a database as its first argument")
    {
        // self, &self, &mut self
        FnArg::Receiver(_) => panic!("#[yeter::query] can't be used on instance methods"),
        FnArg::Typed(pat_type) => match pat_type.pat.as_ref() {
            Pat::Ident(ident) => &ident.ident,
            _ => panic!("simple database argument pattern expected"),
        },
    };

    let query_arg_count = fn_args.len() as u32 - 1;

    let unit_type;

    let query_vis = &function.vis;
    let query_name = &function.sig.ident;

    let input_type = build_type_tuple(query_args.iter().cloned());
    let output_type = match &function.sig.output {
        ReturnType::Default => {
            unit_type = build_unit_tuple();
            &unit_type
        }
        ReturnType::Type(_, typ) => typ.as_ref(),
    };

    let calling_arg_names = arg_names(query_arg_count);

    let calling_tuple_args = calling_tuple_args(calling_arg_names.iter().cloned().zip(query_args));
    let calling_tuple = build_ident_tuple(calling_arg_names.into_iter());

    let input_ident = Ident::new("input", Span::mixed_site());
    let input_ident_expr = Box::new(ident_to_expr(input_ident.clone()));
    let calling_args = std::iter::once(ident_to_expr(db_ident.clone()))
        .chain((0..query_arg_count).map(|n| {
            Expr::Field(ExprField {
                attrs: Default::default(),
                base: input_ident_expr.clone(),
                dot_token: Default::default(),
                member: Member::Unnamed(Index {
                    index: n,
                    span: Span::mixed_site(),
                }),
            })
        }))
        .collect::<Punctuated<_, Token![,]>>();

    let expanded = quote! {
        #(#query_attrs)*
        #query_vis fn #query_name(#db_ident: &::yeter::Database, #calling_tuple_args) -> ::std::rc::Rc<#output_type> {
            use ::yeter::QueryDef;
            #db_ident.run::<#input_type, #output_type>(#query_name::PATH, #calling_tuple)
        }

        #[allow(non_camel_case_types)]
        #[doc(hidden)]
        #query_vis enum #query_name {}

        impl ::yeter::QueryDef for #query_name {
            const PATH: &'static str = stringify!(#query_name);
            type Input = #input_type;
            type Output = #output_type;
        }

        impl ::yeter::ImplementedQueryDef for #query_name {
            #[inline]
            fn run(#db_ident: &::yeter::Database, #input_ident: Self::Input) -> Self::Output {
                #function
                #query_name(#calling_args)
            }
        }
    };

    expanded.into()
}
