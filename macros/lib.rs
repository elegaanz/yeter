use quote::quote;
use syn::{FnArg, ItemFn, parse_macro_input, ReturnType, Type, TypeTuple};
use syn::punctuated::Punctuated;

fn input_type(arg: &FnArg) -> &Type {
    match arg {
        // self, &self, &mut self
        FnArg::Receiver(_) => panic!("#[yeter::query] can't be used on instance methods"),
        FnArg::Typed(arg) => &arg.ty,
    }
}

#[proc_macro_attribute]
pub fn query(attr: proc_macro::TokenStream, item: proc_macro::TokenStream) -> proc_macro::TokenStream {
    assert!(attr.is_empty(), "#[yeter::query] doesn't accept any attributes");

    let function = parse_macro_input!(item as ItemFn);
    assert_eq!(function.sig.inputs.len(), 2, "query functions must take exactly 2 arguments");

    let unit_type = Type::Tuple(TypeTuple {
        paren_token: Default::default(),
        elems: Punctuated::new(),
    });

    let query_vis = &function.vis;
    let query_name = &function.sig.ident;

    let input_type = input_type(&function.sig.inputs.last().unwrap());
    let output_type = match &function.sig.output {
        ReturnType::Default => &unit_type,
        ReturnType::Type(_, typ) => typ.as_ref(),
    };

    let expanded = quote! {
        #query_vis fn #query_name(db: &::yeter::Database, input: #input_type) -> ::std::rc::Rc<#output_type> {
            use yeter::QueryDef;
            db.run::<#input_type, #output_type>(#query_name::PATH, input)
        }

        #[allow(non_camel_case_types)]
        #[doc(hidden)]
        #query_vis enum #query_name {}

        impl yeter::QueryDef for #query_name {
            const PATH: &'static str = stringify!(#query_name);
            type Input = #input_type;
            type Output = #output_type;
        }

        impl ::yeter::ImplementedQueryDef for #query_name {
          #[inline]
          fn run(db: &::yeter::Database, input: Self::Input) -> Self::Output {
            #function
            #query_name(db, input)
          }
        }
    };

    expanded.into()
}
