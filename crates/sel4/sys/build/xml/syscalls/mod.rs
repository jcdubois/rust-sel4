use std::path::Path;

use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use super::{parse_xml, Condition};

mod parse;

use parse::*;

pub fn generate_rust(syscalls_xml_path: impl AsRef<Path>) -> TokenStream {
    let syscalls = Syscalls::parse(&parse_xml(syscalls_xml_path));
    let ty = quote!(i32);
    let mut i = -1i32;
    let mut toks = quote!();
    for api in [&syscalls.api_master, &syscalls.debug].into_iter() {
        for block in api.iter() {
            for syscall in block.syscalls.iter() {
                if Condition::eval_option(&block.condition) {
                    let ident = format_ident!("{}", syscall);
                    toks.extend(quote! {
                        pub const #ident: #ty = #i;
                    });
                }
                i -= 1;
            }
        }
    }
    toks
}
