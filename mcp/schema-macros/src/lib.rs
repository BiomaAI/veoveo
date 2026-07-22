use proc_macro::TokenStream;
use quote::quote;
use syn::{FnArg, GenericArgument, ImplItemFn, PathArguments, Type, parse_macro_input};

/// Publishes an MCP tool using Veoveo's canonical client-facing input schema.
///
/// The handler remains an ordinary `rmcp` tool. This wrapper only selects the
/// shared schema generator for a `Parameters<T>` argument.
#[proc_macro_attribute]
pub fn tool(attributes: TokenStream, item: TokenStream) -> TokenStream {
    let function = parse_macro_input!(item as ImplItemFn);
    let attributes = proc_macro2::TokenStream::from(attributes);
    let Some(parameter_type) = parameters_type(&function) else {
        return quote! {
            #[::rmcp::tool(
                #attributes,
                input_schema = ::veoveo_mcp_contract::mcp_empty_input_schema()
            )]
            #function
        }
        .into();
    };

    quote! {
        #[::rmcp::tool(
            #attributes,
            input_schema = ::veoveo_mcp_contract::mcp_input_schema::<#parameter_type>()
        )]
        #function
    }
    .into()
}

fn parameters_type(function: &ImplItemFn) -> Option<&Type> {
    function.sig.inputs.iter().find_map(|argument| {
        let FnArg::Typed(argument) = argument else {
            return None;
        };
        parameters_inner_type(&argument.ty)
    })
}

fn parameters_inner_type(ty: &Type) -> Option<&Type> {
    let Type::Path(path) = ty else {
        return None;
    };
    let segment = path.path.segments.last()?;
    if segment.ident != "Parameters" {
        return None;
    }
    let PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return None;
    };
    arguments.args.iter().find_map(|argument| match argument {
        GenericArgument::Type(ty) => Some(ty),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;
    use syn::parse_quote;

    #[test]
    fn finds_the_rmcp_parameters_type() {
        let function: ImplItemFn = parse_quote! {
            async fn run(
                &self,
                Parameters(request): Parameters<crate::Request>,
                context: RequestContext<RoleServer>,
            ) -> Result<(), Error> {
                todo!()
            }
        };
        let parameter_type = parameters_type(&function).expect("Parameters<T> is present");
        assert_eq!(
            quote!(#parameter_type).to_string(),
            quote!(crate::Request).to_string()
        );
    }

    #[test]
    fn parameter_lookup_ignores_tools_without_arguments() {
        let function: ImplItemFn = parse_quote! {
            async fn status(&self) -> Result<(), Error> {
                todo!()
            }
        };
        assert!(parameters_type(&function).is_none());
    }
}
