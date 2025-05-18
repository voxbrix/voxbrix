use proc_macro::{
    self,
    TokenStream,
};
use quote::quote;
use syn::{
    Data,
    DeriveInput,
    Fields,
    parse_macro_input,
};

#[proc_macro_derive(SystemData, attributes(my_trait))]
pub fn derive(input: TokenStream) -> TokenStream {
    let input: DeriveInput = parse_macro_input!(input);

    let type_name = input.ident;

    assert_eq!(
        input.generics.lifetimes().count(),
        1,
        "only system data with exactly single lifetime parameter are supported"
    );

    let mut field_list = vec![];
    let mut field_assigns = vec![];

    if let Data::Struct(data_struct) = input.data {
        if let Fields::Named(fields) = data_struct.fields {
            for field in &fields.named {
                if let syn::Type::Reference(reference) = &field.ty {
                    let ref_type = &reference.elem;

                    if reference.mutability.is_some() {
                        field_list.push(quote! {
                            ::voxbrix_world::Request::Write(::core::any::TypeId::of::<#ref_type>()),
                        });
                    } else {
                        field_list.push(quote! {
                            ::voxbrix_world::Request::Read(::core::any::TypeId::of::<#ref_type>()),
                        });
                    }
                } else {
                    panic!("only reference fields of the struct are supported");
                }
            }

            for field in &fields.named {
                let field_name = field.ident.as_ref().unwrap();

                if let syn::Type::Reference(reference) = &field.ty {
                    if reference.mutability.is_some() {
                        field_assigns.push(quote! {
                            #field_name: resources.next()
                                .expect("incorrect number of resources")
                                .downcast_mut(),
                        });
                    } else {
                        field_assigns.push(quote! {
                            #field_name: resources.next()
                                .expect("incorrect number of resources")
                                .downcast_ref(),
                        });
                    }
                } else {
                    panic!("only reference fields of the struct are supported");
                }
            }
        } else {
            panic!("only structs with named fields are supported");
        }
    } else {
        panic!("only structs are supported");
    }

    let expanded = quote! {
        impl<'a> ::voxbrix_world::SystemData<'a> for #type_name<'a> {
            fn required_resources() -> impl Iterator<Item = ::voxbrix_world::Request<::core::any::TypeId>> {
                [
                    #(#field_list)*
                ].into_iter()
            }

            fn from_resources(
                mut resources: impl Iterator<Item = ::voxbrix_world::Access<'a, dyn ::core::any::Any + Send + Sync>>,
            ) -> Self {
                Self {
                    #(#field_assigns)*
                }
            }
        }
    };

    TokenStream::from(expanded)
}
