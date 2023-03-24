use proc_macro2::{Ident, Span, TokenStream};
use proc_macro_error::*;
use quote::quote;
use syn::punctuated::Punctuated;
use syn::{
    parse_quote, Attribute, Expr, ExprField, ExprPath, ExprTuple, FnArg, ForeignItemFn,
    GenericArgument, GenericParam, Index, ItemFn, Member, Pat, PatIdent, PatType, Path,
    PathArguments, PathSegment, ReturnType, Signature, Token, Type, TypePath, TypeReference,
    TypeTuple, Visibility, WhereClause,
};

fn fn_arg_to_type(arg: &FnArg) -> &Type {
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

fn arg_name(arg: &FnArg) -> Option<Ident> {
    match arg {
        FnArg::Receiver(_) => Some(Ident::new("self", Span::call_site())),
        FnArg::Typed(pat_type) => {
            if let Pat::Ident(name) = pat_type.pat.as_ref() {
                Some(name.ident.clone())
            } else {
                None
            }
        }
    }
}

fn arg_names<'a>(args: impl Iterator<Item = &'a FnArg>) -> Vec<Ident> {
    args.enumerate()
        .map(|(n, arg)| {
            arg_name(arg).unwrap_or_else(|| Ident::new(&format!("arg{n}"), Span::mixed_site()))
        })
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

/// Converts generic arguments to generic params (effectively dismissing all ": ???" bounds)
fn use_generic_args(
    generics: &Punctuated<GenericParam, Token![,]>,
) -> Punctuated<GenericArgument, Token![,]> {
    generics
        .iter()
        .map(|p| match p {
            GenericParam::Type(t) => GenericArgument::Type(
                TypePath {
                    qself: None,
                    path: t.ident.clone().into(),
                }
                .into(),
            ),
            GenericParam::Lifetime(l) => GenericArgument::Lifetime(l.lifetime.clone()),
            GenericParam::Const(c) => GenericArgument::Const(ident_to_expr(c.ident.clone())),
        })
        .collect()
}

fn generic_args_phantom(generics: &Punctuated<GenericArgument, Token![,]>) -> Type {
    build_type_tuple(generics.iter().filter_map(|a| {
        match a {
            GenericArgument::Binding(_) => unreachable!(),
            GenericArgument::Constraint(_) => unreachable!(),
            GenericArgument::Type(t) => Some(t.clone()),
            GenericArgument::Const(_) => None,
            GenericArgument::Lifetime(lt) => Some(
                TypeReference {
                    lifetime: Some(lt.clone()),
                    mutability: None,
                    and_token: Default::default(),
                    elem: Box::new(build_unit_tuple()),
                }
                .into(),
            ),
        }
    }))
}

#[proc_macro_error]
#[proc_macro_attribute]
pub fn query(
    attr: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    if !attr.is_empty() {
        emit_error!(
            TokenStream::from(attr),
            "#[yeter::query] doesn't expect any attributes"
        );
    }

    let mut function_no_impl;
    let mut function_impl;
    let function = {
        if let Ok(f) = syn::parse::<ForeignItemFn>(item.clone()) {
            function_no_impl = f;
            &mut function_no_impl as &mut dyn FunctionItem
        } else if let Ok(f) = syn::parse::<ItemFn>(item.clone()) {
            function_impl = f;
            &mut function_impl as &mut dyn FunctionItem
        } else {
            let item = TokenStream::from(item);
            return (quote! { compile_error!("expected fn item"); #item }).into();
        }
    };

    let query_attrs = function.take_attrs();
    let fn_args = &function.sig().inputs;
    let query_args = fn_args
        .iter()
        .skip(1)
        .map(fn_arg_to_type)
        .cloned()
        .collect::<Vec<_>>();

    let db_ident_fallback = Ident::new("db", Span::call_site());
    match fn_args.first() {
        // self, &self, &mut self
        Some(receiver @ FnArg::Receiver(_)) => {
            emit_error!(
                receiver,
                "#[yeter::query] can't be used on instance methods";
                hint = "did you mean `db: &yeter::Database`?";
            );

            &db_ident_fallback
        }
        Some(FnArg::Typed(pat_type)) => match pat_type.pat.as_ref() {
            Pat::Ident(ident) => &ident.ident,
            _ => {
                emit_error!(
                    pat_type.pat,
                    "simple database argument pattern expected";
                    help = "use a simple argument declaration such as `db: &yeter::Database`";
                );

                &db_ident_fallback
            }
        },
        None => {
            emit_error!(
                function.sig(), "a query must take a database as its first argument";
                note = "no arguments were specified";
            );

            &db_ident_fallback
        }
    };

    let fn_arg_count = fn_args.len() as u32;
    let query_arg_count = if fn_arg_count == 0 {
        0
    } else {
        fn_arg_count - 1
    };

    let unit_type;

    let query_vis = &function.vis();
    let query_name = &function.sig().ident;
    let generics = &function.sig().generics;
    let generics_params = &generics.params;
    let generics_where = &generics.where_clause;
    let generics_args = use_generic_args(generics_params);
    let generics_phantom = generic_args_phantom(&generics_args);

    let input_type = build_type_tuple(query_args.iter().cloned());
    let output_type = match &function.sig().output {
        ReturnType::Default => {
            unit_type = build_unit_tuple();
            &unit_type
        }
        ReturnType::Type(_, typ) => typ.as_ref(),
    };

    let calling_arg_names = arg_names(fn_args.iter().skip(1));

    let calling_tuple_args = calling_tuple_args(calling_arg_names.iter().cloned().zip(query_args));
    let calling_tuple = build_ident_tuple(calling_arg_names.into_iter());

    let call_ident_span = Span::call_site().located_at(query_name.span());
    // When Span::def_site is stable, we will be able to properly create hygienic idents
    let call_ident = Ident::new(&format!("__yeter_{query_name}"), call_ident_span);

    let to_function_impl = function.to_function_impl(&call_ident, generics_params, output_type);
    let to_function_call = function.to_function_call(&call_ident, query_arg_count);
    let to_additional_impl = function.to_additional_impl(
        query_name,
        generics_params,
        &generics_args,
        generics_where,
        output_type,
    );

    let expanded = quote! {
        #(#query_attrs)*
        #query_vis fn #query_name<#generics_params>(db: &::yeter::Database, #calling_tuple_args) -> ::std::rc::Rc<#output_type>
            #generics_where
        {
            #to_function_impl
            db.run::<_, #query_name::<#generics_args>>(#to_function_call, #calling_tuple)
        }

        #[allow(non_camel_case_types)]
        #[doc(hidden)]
        #query_vis enum #query_name<#generics_params> {
            Phantom(std::convert::Infallible, std::marker::PhantomData<#generics_phantom>),
        }

        impl<#generics_params> ::yeter::QueryDef for #query_name<#generics_args> #generics_where {
            type Input = #input_type;
            type Output = #output_type;
        }

        #to_additional_impl
    };

    set_dummy(expanded.clone()); // Still produce these tokens if an error was emitted
    expanded.into()
}

trait FunctionItem {
    fn take_attrs(&mut self) -> Vec<Attribute>;
    fn vis(&self) -> &Visibility;
    fn sig(&self) -> &Signature;

    fn to_function_impl(
        &self,
        _call_ident: &Ident,
        _generics_params: &Punctuated<GenericParam, Token![,]>,
        _output_type: &Type,
    ) -> TokenStream {
        quote! {}
    }

    fn to_function_call(&self, _call_ident: &Ident, _query_arg_count: u32) -> TokenStream;

    fn to_additional_impl(
        &self,
        _query_name: &Ident,
        _generics_params: &Punctuated<GenericParam, Token![,]>,
        _generics_args: &Punctuated<GenericArgument, Token![,]>,
        _generics_where: &Option<WhereClause>,
        _output_type: &Type,
    ) -> TokenStream {
        quote! {}
    }
}

fn guess_option_inner_type(option: &Type) -> Type {
    if let Type::Path(path) = option {
        match path.path.segments.last() {
            Some(seg) if seg.ident != "Option" => {
                emit_error!(seg.ident, "expected `Option` type",);
            }
            Some(PathSegment {
                arguments: PathArguments::AngleBracketed(angle),
                ..
            }) if angle.args.len() == 1 => match angle.args.first().unwrap() {
                GenericArgument::Type(t) => return t.clone(),
                o => {
                    emit_error!(o, "unexpected generic argument for Option type",);
                }
            },
            Some(seg) => {
                emit_error!(seg, "expected Option<T> return type",);
            }
            None => {
                emit_error!(path, "expected Option<T> return type",);
            }
        }
    };

    parse_quote! { Option<()> }
}

impl FunctionItem for ForeignItemFn {
    fn take_attrs(&mut self) -> Vec<Attribute> {
        std::mem::take(&mut self.attrs)
    }

    fn vis(&self) -> &Visibility {
        &self.vis
    }

    fn sig(&self) -> &Signature {
        &self.sig
    }

    fn to_function_call(&self, _call_ident: &Ident, _query_arg_count: u32) -> TokenStream {
        quote! {
            |_db, _input| None
        }
    }

    fn to_additional_impl(
        &self,
        query_name: &Ident,
        generics_params: &Punctuated<GenericParam, Token![,]>,
        generics_args: &Punctuated<GenericArgument, Token![,]>,
        generics_where: &Option<WhereClause>,
        output_type: &Type,
    ) -> TokenStream {
        // Output should be an option; we try to guess what could be inside
        let output_type = guess_option_inner_type(output_type);

        quote! {
            impl<#generics_params> ::yeter::InputQueryDef for #query_name<#generics_args> #generics_where {
                type OptionalOutput = #output_type;
            }
        }
    }
}

impl FunctionItem for ItemFn {
    fn take_attrs(&mut self) -> Vec<Attribute> {
        std::mem::take(&mut self.attrs)
    }

    fn vis(&self) -> &Visibility {
        &self.vis
    }

    fn sig(&self) -> &Signature {
        &self.sig
    }

    fn to_function_impl(
        &self,
        call_ident: &Ident,
        _generics_params: &Punctuated<GenericParam, Token![,]>,
        _output_type: &Type,
    ) -> TokenStream {
        let mut s = self.clone();
        s.sig.ident = call_ident.clone();

        quote! {
            #[allow(clippy::needless_lifetimes)]
            #s
        }
    }

    fn to_function_call(&self, call_ident: &Ident, query_arg_count: u32) -> TokenStream {
        let db_ident = Ident::new("db", Span::mixed_site());
        let input_ident = Ident::new("input", Span::mixed_site());
        let input_ident_expr = Box::new(ident_to_expr(input_ident.clone()));
        let calling_args = (0..query_arg_count)
            .map(|n| {
                Expr::Field(ExprField {
                    attrs: Default::default(),
                    base: input_ident_expr.clone(),
                    dot_token: Default::default(),
                    member: Member::Unnamed(Index {
                        index: n,
                        span: Span::mixed_site(),
                    }),
                })
            })
            .collect::<Punctuated<_, Token![,]>>();

        quote! {
            |#db_ident, #input_ident| #call_ident(#db_ident, #calling_args)
        }
    }
}
