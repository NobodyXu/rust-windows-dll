use std::iter::once;
use proc_macro::TokenStream;
use syn::{
    Result,
    parse,
    Lit,
    LitStr,
    ItemForeignMod,
    ForeignItem,
    ForeignItemFn,
    FnDecl,
    FnArg,
    Meta,
    NestedMeta,
};
use quote::quote;

pub fn parse_windows_dll(metadata: TokenStream, input: TokenStream) -> Result<proc_macro2::TokenStream> {
    let dll_name = parse_dll_name(metadata)?;
    let functions = parse_extern_block(&dll_name, input)?;
    Ok(functions)
}

pub fn parse_dll_name(metadata: TokenStream) -> Result<String> {
    let dll_name: LitStr = parse(metadata)?;
    Ok(dll_name.value())
}

pub fn parse_extern_block(dll_name: &str, input: TokenStream) -> Result<proc_macro2::TokenStream> {
    let ItemForeignMod { abi, items, .. } = parse(input)?;

    let functions = items.into_iter().map(|i| {
        match i {
            ForeignItem::Fn(ForeignItemFn { attrs, vis, ident, decl, .. }) => {
                let attr = attrs.iter().find_map(|attr| {
                    let meta = attr.parse_meta().ok()?;
                    if meta.name().to_string() == "link_ordinal" {
                        Some(meta)
                    } else {
                        None
                    }
                });
                let link_ordinal = match attr {
                    Some(Meta::List(mut list)) => {
                        if list.nested.len() == 1 {
                            list
                                .nested
                                .pop()
                                .and_then(|pair| {
                                    match pair.into_value() {
                                        NestedMeta::Literal(Lit::Int(int)) => Some(int),
                                        _ => None,
                                    }
                                })
                        } else {
                            None
                        }
                    },
                    Some(Meta::NameValue(name_value)) => {
                        match name_value.lit {
                            Lit::Int(int) => Some(int),
                            _ => None,
                        }
                    },
                    _ => None,
                };
                let FnDecl { generics, inputs, variadic, output, .. } = &*decl;

                let wide_dll_name = dll_name.encode_utf16().chain(once(0));

                let argument_names = inputs.iter().map(|i| {
                    match i {
                        FnArg::Captured(arg) => &arg.pat,
                        FnArg::Inferred(pat) => pat,
                        _ => panic!("Argument type not supported"),
                    }
                });

                let error = format!("Could not load function {} from {}", &ident, dll_name);

                quote! {
                    #vis unsafe fn #ident ( #(#inputs),* ) #output {
                        use {
                            core::mem::transmute,
                            windows_dll::load_dll_proc,
                        };

                        let func_ptr = load_dll_proc(
                            (&[#(#wide_dll_name),*]).as_ptr(),
                            #link_ordinal,
                        )
                        .expect(#error);

                        let func: unsafe #abi fn( #(#inputs),* ) #output = transmute(func_ptr);

                        func( #(#argument_names),* )
                    }
                }
            },
            _ => panic!("Not a function"),
        }
    });
    Ok(functions.collect())
}